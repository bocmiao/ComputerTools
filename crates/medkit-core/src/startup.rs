//! 开机启动项（docs/architecture.md 第 9 节）：列出登录时自动启动的程序，停用或恢复。
//!
//! 开关用的是和任务管理器「启动应用」同一个：`Explorer\StartupApproved` 下和启动项同名的二进制值。
//! 第一个字节是单数（03）表示禁用，后面 8 个字节是禁用的时间；没有这个值、或者第一个字节是双数（02）表示启用。
//! 只禁用、不删除启动项本身，用户在任务管理器里也能改回来。能写的只有下面 5 个位置，写在代码里；
//! 名字必须是刚列出来的、真实存在的启动项。

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::views::{StartupAdvice, StartupSignature};

/// 列出启动项的脚本（只读）。
pub const LIST_SCRIPT: &str = "startup/list.ps1";
/// 修改日志里启动项改动的「功能 ID」。
pub const FEATURE_ID: &str = "startup";

const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

/// 启动项在哪里登记的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    /// 登录用户的 `Software\Microsoft\Windows\CurrentVersion\Run`
    UserRun,
    /// `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run`
    MachineRun,
    /// `HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run`（32 位程序）
    MachineRun32,
    /// 登录用户的「启动」文件夹
    UserFolder,
    /// 所有用户的「启动」文件夹
    CommonFolder,
}

impl Source {
    /// 开关的位置，`HKCU\` 或 `HKLM\` 开头（HKCU 由引擎换成登录用户的）。
    pub fn approved_key(self) -> String {
        let (root, leaf) = match self {
            Self::UserRun => ("HKCU", "Run"),
            Self::MachineRun => ("HKLM", "Run"),
            Self::MachineRun32 => ("HKLM", "Run32"),
            Self::UserFolder => ("HKCU", "StartupFolder"),
            Self::CommonFolder => ("HKLM", "StartupFolder"),
        };
        format!(r"{root}\{APPROVED}\{leaf}")
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::UserRun => "user-run",
            Self::MachineRun => "machine-run",
            Self::MachineRun32 => "machine-run32",
            Self::UserFolder => "user-folder",
            Self::CommonFolder => "common-folder",
        }
    }

    /// 界面上说的位置。
    pub fn label(self) -> &'static str {
        match self {
            Self::UserRun => "当前用户（注册表）",
            Self::MachineRun | Self::MachineRun32 => "所有用户（注册表）",
            Self::UserFolder => "当前用户（启动文件夹）",
            Self::CommonFolder => "所有用户（启动文件夹）",
        }
    }
}

/// 界面用的 ID：来源加名字（名字里不会有控制字符，见 `valid_name`）。
pub fn item_id(source: Source, name: &str) -> String {
    hex::encode(format!("{}\n{name}", source.as_str()))
}

/// 修改日志里的一条记录是不是启动项的开关：键在 StartupApproved 下。
pub fn is_approved_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    let base = APPROVED.to_ascii_lowercase();
    ["run", "run32", "startupfolder"].iter().any(|leaf| key == format!(r"{base}\{leaf}"))
}

/// 和任务管理器写的一样：启用是 02 加 11 个 0；禁用是 03、3 个 0，再加禁用的时间（FILETIME）。
pub fn approved_value(enabled: bool, now: SystemTime) -> Vec<u8> {
    let mut v = vec![0u8; 12];
    if enabled {
        v[0] = 0x02;
    } else {
        v[0] = 0x03;
        v[4..].copy_from_slice(&filetime(now).to_le_bytes());
    }
    v
}

/// 没有这个值、或者第一个字节是双数，就是启用。
pub fn is_enabled(value: Option<&[u8]>) -> bool {
    value.and_then(|v| v.first()).is_none_or(|b| b & 1 == 0)
}

/// 1601-01-01 起的 100 纳秒数。
fn filetime(t: SystemTime) -> u64 {
    const EPOCH_DIFF_SECS: u64 = 11_644_473_600;
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    (d.as_secs() + EPOCH_DIFF_SECS) * 10_000_000 + u64::from(d.subsec_nanos() / 100)
}

/// 脚本列出的一项（路径只在本机显示，不进报告）。
#[derive(Debug, Clone, Deserialize)]
pub struct RawItem {
    pub source: Source,
    /// 注册表里的值名，或者「启动」文件夹里的文件名
    pub name: String,
    /// 启动的程序的完整路径；读不出来是空的
    #[serde(default)]
    pub path: String,
    /// 程序文件在不在
    #[serde(default)]
    pub exists: bool,
    /// 程序文件里写的说明（任务管理器显示的名字）
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub company: String,
    #[serde(default)]
    pub signature: StartupSignature,
    /// 签名的发布者
    #[serde(default)]
    pub signer: String,
    /// 程序在 Windows 目录下
    #[serde(default)]
    pub system: bool,
}

/// 脚本的输出：`{ result: 'ok', items: [...] }`（PowerShell 5.1 会把只有一项的数组变成对象，两种都认）。
pub fn parse_list(v: &Value) -> Result<Vec<RawItem>, String> {
    match v.get("result").and_then(Value::as_str) {
        Some("ok") => {}
        other => return Err(format!("列启动项的脚本返回了没有定义的结果：{}", other.unwrap_or("（空）"))),
    }
    let items: Vec<&Value> = match v.get("items") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(list)) => list.iter().collect(),
        Some(obj @ Value::Object(_)) => vec![obj],
        Some(other) => return Err(format!("启动项列表的格式不对：{other}")),
    };
    items
        .into_iter()
        .map(|i| serde_json::from_value::<RawItem>(i.clone()).map_err(|e| format!("启动项的格式不对：{e}")))
        .filter(|r| !matches!(r, Ok(i) if !valid_name(&i.name)))
        .collect()
}

/// 名字不能是空的、太长的或者带控制字符的（这样的名字写进注册表也对不上号）。
pub fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= 260 && !name.chars().any(char::is_control)
}

/// 杀毒软件的程序名。
const SECURITY_PROGRAMS: &[&str] = &[
    "360tray.exe",
    "360sd.exe",
    "zhudongfangyu.exe",
    "hipstray.exe",
    "wsctrl.exe",
    "qqpctray.exe",
    "qqpcrtp.exe",
    "kxetray.exe",
    "avp.exe",
    "avpui.exe",
    "egui.exe",
    "avastui.exe",
    "avgui.exe",
    "bdagent.exe",
    "mcuicnt.exe",
    "uihost.exe",
    "securityhealthsystray.exe",
];

/// 杀毒软件和电脑厂商、硬件厂商的名字（签名发布者或者文件里写的公司名，小写后按包含判断）。
const SECURITY_VENDORS: &[&str] = &[
    "qihu",
    "360.cn",
    "huorong",
    "kaspersky",
    "eset",
    "avast",
    "avg technologies",
    "bitdefender",
    "mcafee",
    "symantec",
    "nortonlifelock",
    "gen digital",
    "trend micro",
    "kingsoft internet security",
];
const HARDWARE_VENDORS: &[&str] = &[
    "realtek",
    "synaptics",
    "elan microelectronics",
    "alps",
    "dolby",
    "waves",
    "conexant",
    "cirrus logic",
    "nvidia",
    "advanced micro devices",
    "intel",
    "lenovo",
    "asustek",
    "hp inc",
    "hewlett",
    "dell",
    "acer",
    "huawei",
    "xiaomi",
    "honor device",
    "microsoft surface",
];

/// 给每一项一个建议。不说「建议禁用」：拿不准的时候不制造焦虑（计划书原则 7）。
pub fn advise(item: &RawItem) -> (StartupAdvice, &'static str) {
    let program = item.path.rsplit(['\\', '/']).next().unwrap_or_default().to_ascii_lowercase();
    let who = format!("{} {}", item.signer, item.company).to_ascii_lowercase();
    let text = format!("{} {} {}", item.name, item.description, program).to_lowercase();
    if !item.path.is_empty() && !item.exists {
        return (StartupAdvice::CanDisable, "找不到它要启动的程序，软件可能已经卸载了，停用它没有坏处。");
    }
    if SECURITY_PROGRAMS.contains(&program.as_str()) || SECURITY_VENDORS.iter().any(|v| who.contains(v)) {
        return (StartupAdvice::Keep, "杀毒或安全软件，停用后会少一层保护，建议保持原样。");
    }
    if text.contains("输入法") || text.contains("input method") || text.contains(" ime") || who.contains("sogou") {
        return (StartupAdvice::Keep, "输入法，停用后可能打不出中文，建议保持原样。");
    }
    if item.system {
        return (StartupAdvice::Keep, "Windows 自带的组件，建议保持原样。");
    }
    if HARDWARE_VENDORS.iter().any(|v| who.contains(v)) {
        return (
            StartupAdvice::Keep,
            "硬件驱动或电脑厂商的功能（触控板、声音、快捷键这类），停用后可能不好用，建议保持原样。",
        );
    }
    (StartupAdvice::CanDisable, "不需要一开机就用的话，可以停用；软件本身还在，想用时照样能打开。")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use super::*;

    fn item(name: &str, path: &str, signer: &str) -> RawItem {
        RawItem {
            source: Source::UserRun,
            name: name.to_owned(),
            path: path.to_owned(),
            exists: true,
            description: String::new(),
            company: String::new(),
            signature: StartupSignature::Valid,
            signer: signer.to_owned(),
            system: false,
        }
    }

    #[test]
    fn values_match_what_task_manager_writes() {
        let t = UNIX_EPOCH + Duration::from_secs(1_790_000_000);
        assert_eq!(approved_value(true, t), vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let off = approved_value(false, t);
        assert_eq!(&off[..4], &[3, 0, 0, 0]);
        assert_eq!(u64::from_le_bytes(off[4..].try_into().unwrap()), (1_790_000_000 + 11_644_473_600) * 10_000_000);
        assert!(is_enabled(None));
        assert!(is_enabled(Some(&[2, 0, 0, 0])));
        assert!(is_enabled(Some(&[6, 0])));
        assert!(!is_enabled(Some(&off)));
        assert!(!is_enabled(Some(&[7])));
        assert!(is_enabled(Some(&[])), "空值按启用算（和没有这个值一样）");
    }

    #[test]
    fn only_the_five_approved_keys_are_used() {
        let keys: Vec<String> =
            [Source::UserRun, Source::MachineRun, Source::MachineRun32, Source::UserFolder, Source::CommonFolder]
                .iter()
                .map(|s| s.approved_key())
                .collect();
        assert_eq!(keys[0], r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run");
        assert_eq!(keys[2], r"HKLM\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32");
        for k in &keys {
            assert!(is_approved_key(&k[5..]), "{k}");
        }
        assert!(!is_approved_key(r"Software\Microsoft\Windows\CurrentVersion\Run"));
        assert!(!is_approved_key(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run\x"));
    }

    #[test]
    fn script_output_is_parsed_and_bad_names_dropped() {
        let v = json!({ "result": "ok", "items": [
            { "source": "user-run", "name": "Steam", "path": "C:\\Steam\\steam.exe", "exists": true, "signature": "valid", "signer": "Valve Corp." },
            { "source": "common-folder", "name": "tool.lnk" },
            { "source": "machine-run", "name": " " },
            { "source": "machine-run32", "name": "bad\u{0001}name" }
        ]});
        let items = parse_list(&v).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].signature, StartupSignature::Unknown);
        // 只有一项时 PowerShell 5.1 会输出成对象
        let one = json!({ "result": "ok", "items": { "source": "user-folder", "name": "a.lnk" } });
        assert_eq!(parse_list(&one).unwrap().len(), 1);
        assert!(parse_list(&json!({ "result": "ok" })).unwrap().is_empty());
        assert!(parse_list(&json!({ "result": "boom" })).is_err());
        assert!(parse_list(&json!({ "result": "ok", "items": [{ "source": "somewhere", "name": "x" }] })).is_err());
    }

    #[test]
    fn advice_keeps_security_input_system_and_hardware_items() {
        let (a, _) = advise(&item("360Safetray", r"C:\Program Files\360\360Safe\safemon\360tray.exe", ""));
        assert_eq!(a, StartupAdvice::Keep);
        let (a, _) = advise(&item(
            "HipsTray",
            r"C:\Program Files\Huorong\Sysdiag\bin\HipsTray.exe",
            "Beijing Huorong Network Technology Co., Ltd.",
        ));
        assert_eq!(a, StartupAdvice::Keep);
        let mut ime = item(
            "SogouImeTray",
            r"C:\Program Files\SogouInput\x.exe",
            "Beijing Sogou Technology Development Co., Ltd.",
        );
        assert_eq!(advise(&ime).0, StartupAdvice::Keep);
        ime.signer.clear();
        ime.description = "搜狗输入法 工具".to_owned();
        assert_eq!(advise(&ime).0, StartupAdvice::Keep);
        let mut sys = item("SecurityHealth", r"C:\WINDOWS\system32\SecurityHealthSystray.exe", "Microsoft Windows");
        sys.system = true;
        assert_eq!(advise(&sys).0, StartupAdvice::Keep);
        let (a, _) = advise(&item("RtkAudUService", r"C:\WINDOWS\RtkAudUService64.exe", "Realtek Semiconductor Corp."));
        assert_eq!(a, StartupAdvice::Keep);
        let (a, _) = advise(&item(
            "WeChat",
            r"C:\Program Files\Tencent\WeChat\WeChat.exe",
            "Tencent Technology(Shenzhen) Company Limited",
        ));
        assert_eq!(a, StartupAdvice::CanDisable);
        let mut gone = item("OldTool", r"C:\Old\tool.exe", "");
        gone.exists = false;
        let (a, why) = advise(&gone);
        assert_eq!(a, StartupAdvice::CanDisable);
        assert!(why.contains("卸载"));
    }
}
