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
    // 「系统属性」的「系统保护」页（微软《Executing Control Panel Items》）：开关系统保护、手动创建还原点
    ("system-protection", Program { exe: "SystemPropertiesProtection.exe", args: &[], console: &[] }),
    // 「系统属性」的「高级」页（同一篇文档）：下面的「环境变量」按钮改 Path 这些
    ("environment-variables", Program { exe: "SystemPropertiesAdvanced.exe", args: &[], console: &[] }),
    ("reliability", Program { exe: "perfmon.exe", args: &["/rel"], console: &[] }),
    ("memory-diagnostic", Program { exe: "MdSched.exe", args: &[], console: &[] }),
    ("system-information", Program { exe: "msinfo32.exe", args: &[], console: &[] }),
    ("services", Program { exe: "mmc.exe", args: &["services.msc"], console: &[] }),
    ("event-viewer", Program { exe: "mmc.exe", args: &["eventvwr.msc"], console: &[] }),
    // 「任务计划程序」：禁用、启用计划任务（隐藏的要在「查看」菜单里勾上「显示隐藏的任务」）
    ("task-scheduler", Program { exe: "mmc.exe", args: &["taskschd.msc"], console: &[] }),
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
    // 控制面板的「网络连接」（微软《Executing Control Panel Items》里的 control.exe netconnections）：网卡的禁用、启用
    // 和属性（「配置」里有「电源管理」「高级」两页）
    ("network-connections", Program { exe: "control.exe", args: &["netconnections"], console: &[] }),
    // 重置 Microsoft Store 的缓存（微软《Microsoft Store 应用疑难解答》里的 wsreset）：先出现一个空白的命令行窗口，
    // 十几秒后自己关掉、商店自动打开；不删已经装的应用
    ("store-reset", Program { exe: "WSReset.exe", args: &[], console: &[] }),
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

/// 工具箱「屏幕、键盘、鼠标、声音测试」里的几项。链接写 `test:<名字>`，界面上是跳到那一项的按钮。
/// 界面里每一项的元素 id 是 `device-test-<名字>`（app/src/components/DeviceTests.vue；tests/repo_catalog.rs 核对）。
pub const DEVICE_TESTS: &[&str] = &["screen", "mouse", "keyboard", "speaker", "microphone", "camera"];

/// 「设置」里能打开的页面（ms-settings:<页面>）。
pub const SETTINGS_PAGES: &[&str] = &[
    "windowsupdate",
    // 「Windows 更新 → 高级选项 → 可选更新」：驱动程序更新在这里（Windows 10 2004 起）
    "windowsupdate-optionalupdates",
    // 「更新历史记录」：最下面的「卸载更新」；「使用时段」：Windows 在这段时间里不自动重启
    "windowsupdate-history",
    "windowsupdate-activehours",
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
    // 「图形设置」（Windows 11：「屏幕 → 显示卡」）：给每个程序选用集成显卡还是独立显卡（微软：只有支持的设备上有）
    "display-advancedgraphics",
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
    // 「个性化 → 任务栏」：右下角显示哪些图标（Windows 10 还有「打开或关闭系统图标」）、任务栏的位置和自动隐藏
    "taskbar",
    // 「账户 → 登录选项」：密码、PIN、离开后要不要重新登录、动态锁；「个性化 → 锁屏界面」：最下面是「屏幕保护程序」
    "signinoptions",
    "lockscreen",
    // 「网络和 Internet → 移动热点」；「触摸板」（只有带触摸板的电脑有这一页）
    "network-mobilehotspot",
    "devices-touchpad",
    // 「疑难解答」：Windows 11 在「系统」里（「其他疑难解答」是「获取帮助」的那几个），Windows 10 在「更新和安全」里
    "troubleshoot",
    // 某个自带应用的「高级选项」：终止、修复、重置（微软《Launch Windows Settings》：ms-settings:appsfeatures-app?<包系列名>；
    // 办法照微软《修复 Windows 中的应用和程序》）。包系列名照微软《Keep removed apps from returning during an update》，
    // Windows 10、11 一样：照片、计算器、相机、Microsoft Store、媒体播放器（Windows 10 上叫 Groove 音乐，同一个包）
    "appsfeatures-app?Microsoft.Windows.Photos_8wekyb3d8bbwe",
    "appsfeatures-app?Microsoft.WindowsCalculator_8wekyb3d8bbwe",
    "appsfeatures-app?Microsoft.WindowsCamera_8wekyb3d8bbwe",
    "appsfeatures-app?Microsoft.WindowsStore_8wekyb3d8bbwe",
    "appsfeatures-app?Microsoft.ZuneMusic_8wekyb3d8bbwe",
];

/// 「获取帮助」应用里微软的自动疑难解答（`ms-contact-support://smc-to-emerald/<名字>`），照微软《Windows
/// troubleshooters》页面列的 10 个。小药箱只负责打开，检查和修复由「获取帮助」完成。
pub const TROUBLESHOOTERS: &[&str] = &[
    "AudioTroubleshooter",
    "BITSTroubleshooter",
    "BluetoothTroubleshooter",
    "TroubleshootCamera",
    "NetworkAndInternetTroubleshooter",
    "PrinterTroubleshooter",
    "ProgramCompatTroubleshooter",
    "VideoPlaybackTroubleshooter",
    "WMPTroubleshooter",
    "WUTroubleshooter",
];

pub fn troubleshooter(name: &str) -> Option<&'static str> {
    TROUBLESHOOTERS.iter().copied().find(|t| *t == name)
}

/// 能打开的网页（`open.website` 的名字）。网址不写在数据文件里：由引擎按这台电脑的情况从下面的表里挑。
pub const WEBSITES: &[&str] = &[
    // 电脑品牌官网的驱动下载页：按 BIOS 里写的厂商认品牌（见 [`oem_match`]）
    "oem-drivers",
];

pub fn website(name: &str) -> Option<&'static str> {
    WEBSITES.iter().copied().find(|w| *w == name)
}

/// 一个电脑（或主板）品牌在中国大陆的官方驱动下载页。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OemSite {
    pub key: &'static str,
    /// 界面上说的品牌名
    pub brand: &'static str,
    /// 官方的驱动下载页（https）。2026 年 9 月逐个打开核实过；戴尔、宏碁、微星、技嘉的网站不让程序访问，是按搜索
    /// 结果里的官方页面标题核实的
    pub drivers: &'static str,
}

const fn site(key: &'static str, brand: &'static str, drivers: &'static str) -> OemSite {
    OemSite { key, brand, drivers }
}

static LENOVO: OemSite = site("lenovo", "联想", "https://newsupport.lenovo.com.cn/driveDownloads_index.html");
static HP: OemSite = site("hp", "惠普", "https://support.hp.com/cn-zh/drivers");
static DELL: OemSite = site("dell", "戴尔", "https://www.dell.com/support/home/zh-cn?app=drivers");
static ASUS: OemSite = site("asus", "华硕", "https://www.asus.com.cn/support/download-center/");
static ACER: OemSite = site("acer", "宏碁", "https://www.acer.com.cn/support.html?type=1");
static HUAWEI: OemSite = site("huawei", "华为", "https://consumer.huawei.com/cn/support/");
static HONOR: OemSite = site("honor", "荣耀", "https://www.honor.com/cn/support/");
static XIAOMI: OemSite = site("xiaomi", "小米", "https://www.mi.com/service/notebook/drivers");
static MSI: OemSite = site("msi", "微星", "https://cn.msi.com/service/download");
static SAMSUNG: OemSite = site("samsung", "三星", "https://www.samsung.com.cn/support/");
static GIGABYTE: OemSite = site("gigabyte", "技嘉", "https://www.gigabyte.cn/Support/Consumer/Download");
static ASROCK: OemSite = site("asrock", "华擎", "https://www.asrock.com/support/index.cn.asp");
static SURFACE: OemSite = site(
    "surface",
    "微软 Surface",
    "https://support.microsoft.com/zh-cn/surface/drivers-firmware/download-drivers-and-firmware-for-surface",
);

/// 所有品牌（校验、测试、文档用）。
pub static OEM_SITES: &[&OemSite] =
    &[&LENOVO, &HP, &DELL, &ASUS, &ACER, &HUAWEI, &HONOR, &XIAOMI, &MSI, &SAMSUNG, &GIGABYTE, &ASROCK, &SURFACE];

/// 系统厂商（[`norm`] 以后）→ 品牌。`true`：整串相等才算（太短的名字，免得把别的厂商认错）；`false`：以它开头就算。
/// 写法照 systemd 的 hwdb 和 Linux 内核里按 DMI 认笔记本的表：联想 `LENOVO`，惠普 `HP`、`Hewlett-Packard`，
/// 戴尔 `Dell Inc.`（外星人写 `Alienware`），华硕 `ASUSTeK COMPUTER INC.`、`ASUS`，小米 `TIMI`、`Xiaomi Inc`，
/// 微星 `Micro-Star International Co., Ltd.`，技嘉 `GIGABYTE`、`Gigabyte Technology Co.,Ltd.`。
/// Surface 另外看产品名（见 [`oem_match`]）。
const SYSTEM_BRANDS: &[(&str, bool, &OemSite)] = &[
    ("LENOVO", false, &LENOVO),
    ("HP", true, &HP),
    ("HEWLETTPACKARD", false, &HP),
    ("DELL", false, &DELL),
    ("ALIENWARE", false, &DELL),
    ("ASUSTEK", false, &ASUS),
    ("ASUS", true, &ASUS),
    ("ACER", false, &ACER),
    ("HUAWEI", false, &HUAWEI),
    ("HONOR", false, &HONOR),
    ("TIMI", true, &XIAOMI),
    ("XIAOMI", false, &XIAOMI),
    ("MICROSTAR", false, &MSI),
    ("MSI", true, &MSI),
    ("SAMSUNG", false, &SAMSUNG),
    ("GIGABYTE", false, &GIGABYTE),
    ("ASROCK", false, &ASROCK),
];

/// 自己组装的电脑：系统厂商认不出来（多半是 `To be filled by O.E.M.`、`System manufacturer` 这样的占位文字），
/// 按主板厂商找。华擎的主板厂商写 `ASRock`（Linux 内核 nct6775 驱动里的写法）。
const BOARD_BRANDS: &[(&str, bool, &OemSite)] = &[
    ("ASUSTEK", false, &ASUS),
    ("ASUS", true, &ASUS),
    ("MICROSTAR", false, &MSI),
    ("MSI", true, &MSI),
    ("GIGABYTE", false, &GIGABYTE),
    ("ASROCK", false, &ASROCK),
];

/// 虚拟机在 BIOS 里写的名字：照 systemd 的 src/basic/virt.c（`dmi_vendor_table`），产品名、系统厂商、主板厂商、
/// BIOS 厂商、产品版本这 5 项里有一项以它们开头，就是虚拟机。
const VM_PREFIXES: &[&str] = &[
    "KVM",
    "OpenStack",
    "KubeVirt",
    "Alibaba Cloud ECS",
    "Amazon EC2",
    "QEMU",
    "VMware",
    "VMW",
    "innotek GmbH",
    "VirtualBox",
    "Oracle Corporation",
    "Xen",
    "Bochs",
    "Parallels",
    "BHYVE",
    "Hyper-V",
    "Apple Virtualization",
    "Google Compute Engine",
];

/// BIOS 里没填时常见的占位文字（[`norm`] 以后）：systemd 的 hwdb 和 Linux 内核的 DMI 表里出现过的那些。
const PLACEHOLDERS: &[&str] = &[
    "DEFAULTSTRING",
    "TOBEFILLEDBYOEM",
    "OEM",
    "SYSTEMMANUFACTURER",
    "SYSTEMPRODUCTNAME",
    "SYSTEMVERSION",
    "NONE",
    "NOTAPPLICABLE",
    "NOTSPECIFIED",
];

/// BIOS 里写的厂商和型号：注册表 `HKLM\HARDWARE\DESCRIPTION\System\BIOS`，每次开机由系统从 SMBIOS 抄过来。
/// 这里没有序列号。读不到的是空字符串。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BiosInfo {
    /// SystemManufacturer
    pub system_manufacturer: String,
    /// SystemProductName
    pub system_product: String,
    /// SystemVersion（联想在这里写 `ThinkPad X1 Carbon Gen 9` 这样的型号名）
    pub system_version: String,
    /// SystemFamily
    pub system_family: String,
    /// BaseBoardManufacturer
    pub board_manufacturer: String,
    /// BaseBoardProduct
    pub board_product: String,
    /// BIOSVendor
    pub bios_vendor: String,
}

/// 注册表里 [`BiosInfo`] 各项的值名。
pub const BIOS_KEY: &str = r"HARDWARE\DESCRIPTION\System\BIOS";

/// 按 BIOS 认出来的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OemMatch {
    /// 品牌机：品牌官网；`model` 是让用户在网页上搜的型号
    Brand { site: &'static OemSite, model: Option<String> },
    /// 系统厂商没写品牌（多半是自己组装的电脑），按主板品牌；`model` 是主板型号
    Board { site: &'static OemSite, model: Option<String> },
    /// 虚拟机：驱动由虚拟机软件提供
    VirtualMachine,
    /// 认不出：BIOS 里写的系统厂商（没写、或者是占位文字时为空）
    Unknown { manufacturer: String },
}

/// 只留字母和数字，转大写：`Micro-Star International Co., Ltd.` → `MICROSTARINTERNATIONALCOLTD`。
fn norm(s: &str) -> String {
    s.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_uppercase()).collect()
}

fn is_placeholder(s: &str) -> bool {
    let n = norm(s);
    n.is_empty() || PLACEHOLDERS.contains(&n.as_str())
}

fn find_brand(table: &[(&str, bool, &'static OemSite)], vendor: &str) -> Option<&'static OemSite> {
    let n = norm(vendor);
    // HPE（慧与）的服务器不是惠普的电脑
    if n.starts_with("HEWLETTPACKARDENTERPRISE") {
        return None;
    }
    table.iter().find(|(name, exact, _)| if *exact { n == *name } else { n.starts_with(name) }).map(|(_, _, s)| *s)
}

/// 第一个不是占位文字的值。
fn first_real(values: &[&str]) -> Option<String> {
    values.iter().map(|v| v.trim()).find(|v| !is_placeholder(v)).map(str::to_owned)
}

/// 按 BIOS 里写的厂商认出品牌：先认虚拟机，再按系统厂商认品牌机，认不出再按主板厂商（自己组装的电脑）。
pub fn oem_match(bios: &BiosInfo) -> OemMatch {
    let vm_fields = [
        &bios.system_product,
        &bios.system_manufacturer,
        &bios.board_manufacturer,
        &bios.bios_vendor,
        &bios.system_version,
    ];
    // Hyper-V 的虚拟机：厂商是 Microsoft Corporation，产品名是 Virtual Machine（Linux 内核 video_detect.c 的写法）
    let hyper_v =
        norm(&bios.system_manufacturer).starts_with("MICROSOFT") && norm(&bios.system_product) == "VIRTUALMACHINE";
    if hyper_v || vm_fields.iter().any(|f| VM_PREFIXES.iter().any(|p| f.trim_start().starts_with(p))) {
        return OemMatch::VirtualMachine;
    }
    if norm(&bios.system_manufacturer).starts_with("MICROSOFT") && norm(&bios.system_product).starts_with("SURFACE") {
        return OemMatch::Brand { site: &SURFACE, model: first_real(&[&bios.system_product]) };
    }
    if let Some(site) = find_brand(SYSTEM_BRANDS, &bios.system_manufacturer) {
        // 联想的产品名是机器类型编号（20XWCTO1WW 这样），型号名写在 SystemVersion、SystemFamily 里
        let model = if site.key == LENOVO.key {
            first_real(&[&bios.system_version, &bios.system_family, &bios.system_product])
        } else {
            first_real(&[&bios.system_product])
        };
        return OemMatch::Brand { site, model };
    }
    if let Some(site) = find_brand(BOARD_BRANDS, &bios.board_manufacturer) {
        return OemMatch::Board { site, model: first_real(&[&bios.board_product]) };
    }
    OemMatch::Unknown { manufacturer: first_real(&[&bios.system_manufacturer]).unwrap_or_default() }
}

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
        assert_eq!(troubleshooter("AudioTroubleshooter"), Some("AudioTroubleshooter"));
        assert!(troubleshooter("audiotroubleshooter").is_none(), "名字要一字不差");
        assert!(troubleshooter("Audio/../x").is_none());
    }

    fn bios(manufacturer: &str, product: &str) -> BiosInfo {
        BiosInfo { system_manufacturer: manufacturer.into(), system_product: product.into(), ..Default::default() }
    }

    fn brand_key(b: &BiosInfo) -> Option<&'static str> {
        match oem_match(b) {
            OemMatch::Brand { site, .. } => Some(site.key),
            _ => None,
        }
    }

    #[test]
    fn oem_brands_are_recognised_from_what_real_machines_write() {
        // 真实机器的写法（systemd 的 hwdb、Linux 内核按 DMI 认笔记本的表里的）
        for (vendor, key) in [
            ("LENOVO", "lenovo"),
            ("HP", "hp"),
            ("Hewlett-Packard", "hp"),
            ("Dell Inc.", "dell"),
            ("Dell Computer Corporation", "dell"),
            ("Alienware", "dell"),
            ("ASUSTeK COMPUTER INC.", "asus"),
            ("ASUSTeK Computer Inc.", "asus"),
            ("ASUS", "asus"),
            ("Acer", "acer"),
            ("HUAWEI", "huawei"),
            ("HONOR", "honor"),
            ("TIMI", "xiaomi"),
            ("Xiaomi Inc", "xiaomi"),
            ("Micro-Star International Co., Ltd.", "msi"),
            ("MICRO-STAR INTERNATIONAL CO., LTD", "msi"),
            ("SAMSUNG ELECTRONICS CO., LTD.", "samsung"),
            ("GIGABYTE", "gigabyte"),
            ("Gigabyte Technology Co.,Ltd.", "gigabyte"),
            ("ASRock", "asrock"),
        ] {
            assert_eq!(brand_key(&bios(vendor, "Some Model")), Some(key), "{vendor}");
        }
        // 太短的名字要整串相等：HPE（慧与）的服务器、名字里带 HP 的别家都不算惠普
        for vendor in ["HPE", "Hewlett Packard Enterprise", "HPC Systems", "TIMI Tech", "MSI-X"] {
            assert_eq!(brand_key(&bios(vendor, "x")), None, "{vendor}");
        }
    }

    #[test]
    fn microsoft_is_surface_only_when_the_product_says_so() {
        let surface = oem_match(&bios("Microsoft Corporation", "Surface Pro 7"));
        assert_eq!(
            surface,
            OemMatch::Brand { site: &SURFACE, model: Some("Surface Pro 7".into()) },
            "Surface 的型号就是产品名"
        );
        assert_eq!(oem_match(&bios("Microsoft Corporation", "Virtual Machine")), OemMatch::VirtualMachine);
        assert_eq!(
            oem_match(&bios("Microsoft Corporation", "Xbox")),
            OemMatch::Unknown { manufacturer: "Microsoft Corporation".into() }
        );
    }

    #[test]
    fn virtual_machines_are_recognised_from_any_of_the_five_fields() {
        // 照 systemd：产品名、系统厂商、主板厂商、BIOS 厂商、产品版本
        let cases = [
            bios("VMware, Inc.", "VMware7,1"),
            bios("innotek GmbH", "VirtualBox"),
            bios("QEMU", "Standard PC (Q35 + ICH9, 2009)"),
            bios("Red Hat", "KVM"),
            bios("Xen", "HVM domU"),
            bios("Parallels Software International Inc.", "Parallels Virtual Platform"),
            bios("Alibaba Cloud", "Alibaba Cloud ECS"),
            BiosInfo { board_manufacturer: "Oracle Corporation".into(), ..bios("Some OEM", "x") },
            BiosInfo { bios_vendor: "Xen".into(), ..bios("Amazon", "t3.large") },
            BiosInfo { system_version: "Hyper-V UEFI Release v4.1".into(), ..bios("Microsoft Corporation", "x") },
        ];
        for b in cases {
            assert_eq!(oem_match(&b), OemMatch::VirtualMachine, "{b:?}");
        }
        // 真机不能认成虚拟机：联想的笔记本、自己组装的电脑
        assert_ne!(oem_match(&bios("LENOVO", "20XWCTO1WW")), OemMatch::VirtualMachine);
    }

    #[test]
    fn lenovo_models_come_from_the_version_or_family() {
        let b = BiosInfo { system_version: "ThinkPad X1 Carbon Gen 9".into(), ..bios("LENOVO", "20XWCTO1WW") };
        assert_eq!(oem_match(&b), OemMatch::Brand { site: &LENOVO, model: Some("ThinkPad X1 Carbon Gen 9".into()) });
        let b = BiosInfo { system_family: "IdeaPad 5 14ALC05".into(), ..bios("LENOVO", "82LM") };
        assert_eq!(oem_match(&b), OemMatch::Brand { site: &LENOVO, model: Some("IdeaPad 5 14ALC05".into()) });
        // 都没有时退回产品名；别家的型号就是产品名，占位文字不算型号
        assert_eq!(oem_match(&bios("LENOVO", "82LM")), OemMatch::Brand { site: &LENOVO, model: Some("82LM".into()) });
        assert_eq!(
            oem_match(&bios("HP", " HP Pavilion Laptop 15-eg0xxx ")),
            OemMatch::Brand { site: &HP, model: Some("HP Pavilion Laptop 15-eg0xxx".into()) }
        );
        assert_eq!(oem_match(&bios("Acer", "Default string")), OemMatch::Brand { site: &ACER, model: None });
    }

    #[test]
    fn home_built_pcs_are_found_by_their_motherboard() {
        for placeholder in
            ["To be filled by O.E.M.", "To Be Filled By O.E.M.", "System manufacturer", "Default string", ""]
        {
            let b = BiosInfo {
                board_manufacturer: "ASRock".into(),
                board_product: "B450M Steel Legend".into(),
                ..bios(placeholder, "System Product Name")
            };
            assert_eq!(
                oem_match(&b),
                OemMatch::Board { site: &ASROCK, model: Some("B450M Steel Legend".into()) },
                "{placeholder:?}"
            );
        }
        let b = BiosInfo {
            board_manufacturer: "ASUSTeK COMPUTER INC.".into(),
            board_product: "To be filled by O.E.M.".into(),
            ..bios("System manufacturer", "System Product Name")
        };
        assert_eq!(oem_match(&b), OemMatch::Board { site: &ASUS, model: None });
        // 系统厂商写了别的品牌（小药箱没收的），主板也不是那几家：认不出，说出 BIOS 里写的厂商
        let b = BiosInfo { board_manufacturer: "MECHREVO".into(), ..bios("MECHREVO", "Jiaolong17KS Series GM7XG0M") };
        assert_eq!(oem_match(&b), OemMatch::Unknown { manufacturer: "MECHREVO".into() });
        // 什么都没写
        assert_eq!(oem_match(&BiosInfo::default()), OemMatch::Unknown { manufacturer: String::new() });
        assert_eq!(oem_match(&bios("To be filled by O.E.M.", "x")), OemMatch::Unknown { manufacturer: String::new() });
    }

    #[test]
    fn oem_sites_are_https_and_distinct() {
        let mut keys: Vec<&str> = OEM_SITES.iter().map(|s| s.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), OEM_SITES.len(), "品牌不能重复");
        for s in OEM_SITES {
            assert!(s.drivers.starts_with("https://") && !s.drivers.contains(' '), "{s:?}");
        }
        // 表里的品牌都在 OEM_SITES 里
        for (_, _, s) in SYSTEM_BRANDS.iter().chain(BOARD_BRANDS) {
            assert!(OEM_SITES.contains(s), "{s:?}");
        }
        assert_eq!(website("oem-drivers"), Some("oem-drivers"));
        assert!(website("https://example.com").is_none());
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
