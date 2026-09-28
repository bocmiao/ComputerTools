//! 小工具（docs/architecture.md 第 11 节）：`open` 小工具能打开的程序和「设置」页面的名单，
//! 以及把 `info` 脚本返回的 sections 渲染成界面上的表格。
//!
//! 名单写在代码里而不是数据里：数据文件只能从名单里挑一个名字，不能写程序路径或参数。

use std::collections::BTreeMap;

use serde_json::Value;

use crate::model::{Text, ToolLabels};
use crate::views::{ToolRow, ToolSection};

/// 一个能打开的系统工具，都在 System32 下。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Program {
    /// System32 下的文件名
    pub exe: &'static str,
    /// 参数；以 `.msc` 结尾的会换成 System32 下的绝对路径
    pub args: &'static [&'static str],
    /// 不为空时（`exe` 是 cmd.exe）：在一个新的命令行窗口里依次运行这几条命令，窗口留着看结果。
    /// 每条是「System32 下的程序名 参数」，程序换成绝对路径（见 [`console_command_line`]）
    pub console: &'static [&'static str],
}

pub const OPEN_PROGRAMS: &[(&str, Program)] = &[
    ("task-manager", Program { exe: "Taskmgr.exe", args: &[], console: &[] }),
    ("device-manager", Program { exe: "mmc.exe", args: &["devmgmt.msc"], console: &[] }),
    ("disk-management", Program { exe: "mmc.exe", args: &["diskmgmt.msc"], console: &[] }),
    ("disk-cleanup", Program { exe: "cleanmgr.exe", args: &[], console: &[] }),
    ("system-restore", Program { exe: "rstrui.exe", args: &[], console: &[] }),
    ("reliability", Program { exe: "perfmon.exe", args: &["/rel"], console: &[] }),
    ("memory-diagnostic", Program { exe: "MdSched.exe", args: &[], console: &[] }),
    ("system-information", Program { exe: "msinfo32.exe", args: &[], console: &[] }),
    ("services", Program { exe: "mmc.exe", args: &["services.msc"], console: &[] }),
    ("event-viewer", Program { exe: "mmc.exe", args: &["eventvwr.msc"], console: &[] }),
    ("control-panel", Program { exe: "control.exe", args: &[], console: &[] }),
    ("uac-settings", Program { exe: "UserAccountControlSettings.exe", args: &[], console: &[] }),
    ("firewall", Program { exe: "control.exe", args: &["firewall.cpl"], console: &[] }),
    ("indexing-options", Program { exe: "control.exe", args: &["srchadmin.dll"], console: &[] }),
    ("windows-features", Program { exe: "OptionalFeatures.exe", args: &[], console: &[] }),
    // 「高级共享设置」：网络发现、文件和打印机共享的开关（Win11 22H2 起会转到「设置」里的同一页）
    (
        "advanced-sharing",
        Program {
            exe: "control.exe",
            args: &["/name", "Microsoft.NetworkAndSharingCenter", "/page", "Advanced"],
            console: &[],
        },
    ),
    // 「高级安全 Windows Defender 防火墙」：看入站规则（小药箱只查不改）
    ("firewall-advanced", Program { exe: "mmc.exe", args: &["wf.msc"], console: &[] }),
    // 控制面板的「区域」：「管理」页里改「非 Unicode 程序的语言」和 UTF-8（Beta）
    ("region", Program { exe: "control.exe", args: &["intl.cpl"], console: &[] }),
    // 微软《使用系统文件检查器工具修复丢失或损坏的系统文件》：先用 DISM 修复映像，再运行 sfc
    (
        "system-file-repair",
        Program {
            exe: "cmd.exe",
            args: &[],
            console: &["Dism.exe /Online /Cleanup-Image /RestoreHealth", "sfc.exe /scannow"],
        },
    ),
];

/// 「设置」里能打开的页面（ms-settings:<页面>）。
pub const SETTINGS_PAGES: &[&str] = &[
    "windowsupdate",
    "storagesense",
    "storagepolicies",
    "appsfeatures",
    "startupapps",
    "defaultapps",
    "network-status",
    "printers",
    "sound",
    "powersleep",
    "batterysaver-usagedetails",
    "display",
    "bluetooth",
    "recovery",
    "windowsdefender",
    "privacy-microphone",
    "privacy-webcam",
    "dateandtime",
    "easeofaccess-keyboard",
    "easeofaccess-mouse",
    "easeofaccess-colorfilter",
    "easeofaccess-highcontrast",
    "nightlight",
    "regionlanguage",
    "apps-volume",
    "notifications",
];

/// 命令行窗口里要运行的那一串：`/k ""<System32>\Dism.exe" /Online … & "<System32>\sfc.exe" /scannow"`。程序都写
/// 绝对路径（cmd 找程序时先找当前文件夹）；一条失败了下一条照样运行（`&`）。整串外面再包一层引号：里面有引号和 `&`
/// 时，cmd 只去掉最外面那一对（`cmd /?` 里说的规则）。没有这个程序时返回它的名字。
pub fn console_command_line(system32: &std::path::Path, commands: &[&str]) -> Result<String, String> {
    let mut line = String::new();
    for command in commands {
        let (name, rest) = command.split_once(' ').unwrap_or((command, ""));
        let program = system32.join(name);
        if !program.is_file() {
            return Err(name.to_owned());
        }
        if !line.is_empty() {
            line.push_str(" & ");
        }
        line.push_str(&format!("\"{}\"", program.display()));
        if !rest.is_empty() {
            line.push(' ');
            line.push_str(rest);
        }
    }
    Ok(format!("/k \"{line}\""))
}

pub fn program(name: &str) -> Option<&'static Program> {
    OPEN_PROGRAMS.iter().find(|(n, _)| *n == name).map(|(_, p)| p)
}

pub fn settings_page(page: &str) -> Option<&'static str> {
    SETTINGS_PAGES.iter().copied().find(|p| *p == page)
}

/// 把 `info` 脚本返回的 sections 按 labels 渲染成表格。
///
/// 缺标签的 id 或 code 照样显示原文，同时记进 `missing`（引擎把它写进结果的 error，CI 的冒烟测试会拦住）。
/// PowerShell 5.1 有时会把只有一项的数组输出成单个对象，这里两种都认。
pub fn render_sections(
    raw: Option<&Value>,
    labels: &ToolLabels,
    lang: &str,
    missing: &mut Vec<String>,
) -> Vec<ToolSection> {
    let mut out = Vec::new();
    for (i, s) in items(raw).into_iter().enumerate() {
        let Some(id) = s.get("id").and_then(Value::as_str) else {
            missing.push(format!("第 {} 个表格没有 id", i + 1));
            continue;
        };
        let base = label(&labels.sections, id, lang, "sections", missing);
        let title = match s.get("name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty()) {
            Some(name) => format!("{base}：{name}"),
            None => base,
        };
        let rows = items(s.get("rows")).into_iter().filter_map(|r| row(r, id, labels, lang, missing)).collect();
        out.push(ToolSection { title, rows });
    }
    out
}

fn items(v: Option<&Value>) -> Vec<&Value> {
    match v {
        Some(Value::Array(list)) => list.iter().collect(),
        Some(obj @ Value::Object(_)) => vec![obj],
        _ => Vec::new(),
    }
}

fn row(r: &Value, section: &str, labels: &ToolLabels, lang: &str, missing: &mut Vec<String>) -> Option<ToolRow> {
    let Some(id) = r.get("id").and_then(Value::as_str) else {
        missing.push(format!("表格 {section} 里有一行没有 id"));
        return None;
    };
    let label_text = label(&labels.rows, id, lang, "rows", missing);
    let value = match (r.get("code").and_then(Value::as_str), r.get("value")) {
        (Some(code), _) => label(&labels.values, code, lang, "values", missing),
        (None, Some(Value::String(s))) => s.clone(),
        (None, Some(Value::Number(n))) => n.to_string(),
        (None, Some(Value::Bool(b))) => if *b { "是" } else { "否" }.to_owned(),
        _ => {
            missing.push(format!("表格 {section} 的 {id} 没有 value 或 code"));
            return None;
        }
    };
    let secret = r.get("secret").and_then(Value::as_bool).unwrap_or(false);
    let qr = r.get("qr").and_then(Value::as_bool).unwrap_or(false);
    Some(ToolRow { label: label_text, value, secret, qr })
}

fn label(map: &BTreeMap<String, Text>, key: &str, lang: &str, kind: &str, missing: &mut Vec<String>) -> String {
    match map.get(key) {
        Some(t) => t.get(lang).to_owned(),
        None => {
            missing.push(format!("labels.{kind}.{key}"));
            key.to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn labels() -> ToolLabels {
        ToolLabels {
            sections: BTreeMap::from([("disk".into(), Text::zh_only("硬盘")), ("wifi".into(), Text::zh_only("WiFi"))]),
            rows: BTreeMap::from([
                ("size".into(), Text::zh_only("容量")),
                ("type".into(), Text::zh_only("类型")),
                ("password".into(), Text::zh_only("密码")),
            ]),
            values: BTreeMap::from([("ssd".into(), Text::zh_only("固态硬盘"))]),
        }
    }

    #[test]
    fn sections_are_rendered_with_labels() {
        let raw = json!([
            { "id": "disk", "name": "Samsung SSD 870", "rows": [
                { "id": "size", "value": "466 GB" }, { "id": "type", "code": "ssd" }, { "id": "size", "value": 12 }
            ] },
            { "id": "wifi", "name": "我家", "rows": [
                { "id": "password", "value": "12345678", "secret": true },
                { "id": "password", "value": "WIFI:T:WPA;S:我家;P:12345678;;", "secret": true, "qr": true }
            ] }
        ]);
        let mut missing = Vec::new();
        let s = render_sections(Some(&raw), &labels(), "zh-CN", &mut missing);
        assert!(missing.is_empty(), "{missing:?}");
        assert_eq!(s[0].title, "硬盘：Samsung SSD 870");
        assert_eq!((s[0].rows[1].label.as_str(), s[0].rows[1].value.as_str()), ("类型", "固态硬盘"));
        assert_eq!(s[0].rows[2].value, "12");
        assert!(!s[0].rows[0].secret && s[1].rows[0].secret);
        // 二维码的行：原样带着 qr，界面画成二维码
        assert!(!s[1].rows[0].qr && s[1].rows[1].qr && s[1].rows[1].secret);
        assert_eq!(s[1].title, "WiFi：我家");
    }

    #[test]
    fn missing_labels_show_the_raw_text_and_are_reported() {
        let raw = json!([{ "id": "gpu", "rows": [ { "id": "vram", "code": "big" }, { "id": "size" } ] }]);
        let mut missing = Vec::new();
        let s = render_sections(Some(&raw), &labels(), "zh-CN", &mut missing);
        assert_eq!(s[0].title, "gpu");
        assert_eq!((s[0].rows[0].label.as_str(), s[0].rows[0].value.as_str()), ("vram", "big"));
        assert_eq!(s[0].rows.len(), 1, "没有值的行不显示");
        assert_eq!(missing.len(), 4, "{missing:?}");
    }

    #[test]
    fn single_objects_count_as_one_item_lists() {
        // PowerShell 5.1 会把只有一项的数组输出成对象
        let raw = json!({ "id": "disk", "rows": { "id": "size", "value": "1 TB" } });
        let mut missing = Vec::new();
        let s = render_sections(Some(&raw), &labels(), "zh-CN", &mut missing);
        assert_eq!((s.len(), s[0].rows.len()), (1, 1));
        assert!(render_sections(None, &labels(), "zh-CN", &mut missing).is_empty());
    }

    #[test]
    fn allowlists_resolve_names() {
        assert_eq!(program("device-manager").map(|p| p.exe), Some("mmc.exe"));
        assert!(program("cmd").is_none());
        assert_eq!(settings_page("windowsupdate"), Some("windowsupdate"));
        assert!(settings_page("../x").is_none());
    }

    #[test]
    fn console_commands_use_absolute_paths_and_one_outer_pair_of_quotes() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["Dism.exe", "sfc.exe"] {
            std::fs::write(dir.path().join(name), b"").unwrap();
        }
        let repair = program("system-file-repair").unwrap();
        assert_eq!(repair.exe, "cmd.exe");
        let line = console_command_line(dir.path(), repair.console).unwrap();
        let d = dir.path().display();
        assert_eq!(
            line,
            format!(
                r#"/k ""{d}{sep}Dism.exe" /Online /Cleanup-Image /RestoreHealth & "{d}{sep}sfc.exe" /scannow""#,
                sep = std::path::MAIN_SEPARATOR
            )
        );
        assert_eq!(
            console_command_line(dir.path(), &["chkdsk.exe /scan"]),
            Err("chkdsk.exe".to_owned()),
            "没有的程序不拼"
        );
        // 别的程序都不是命令行窗口
        assert!(OPEN_PROGRAMS.iter().filter(|(_, p)| !p.console.is_empty()).all(|(_, p)| p.exe == "cmd.exe"));
    }
}
