//! 右键菜单管理（docs/architecture.md 第 9 节）：列出软件加进右键菜单的项目，能拿掉、能恢复。
//!
//! 脚本 `shell/context-menu-list.ps1`（只读）列出三类登记：
//! - 命令（verb）：`<范围>\shell\<名字>`。拿掉 = 在这个键上写空的 `ProgrammaticAccessOnly`（微软文档：菜单里不显示，
//!   程序照样能调用）；恢复 = 删掉它（别的工具写的 `LegacyDisable` 也一起删）。写在它登记的地方
//!   （所有用户的在 HKLM，当前用户的在登录用户的 HKCU），下次右键就生效。
//! - 外壳扩展（handler）：`<范围>\shellex\ContextMenuHandlers\<名字>`，靠 CLSID 找到它的 DLL；
//! - Windows 11 新菜单里应用加的项目（packaged）：应用清单里的 `windows.fileExplorerContextMenus`。
//!   这两类都按 CLSID 拿掉：`HKLM\...\Shell Extensions\Blocked` 下写一个名字是 CLSID 的空字符串值
//!   （NirSoft ShellExView 等工具一直这么做，微软没有写进文档）；恢复 = 删掉它（登录用户 HKCU 下的也删）。
//!   同一个 CLSID 在几个范围里登记的，合成一项（拿掉的是这个扩展本身）。要重启资源管理器才生效。
//!
//! 只列第三方软件的：Windows 自带的（程序在 Windows 目录里、微软签名的外壳扩展和命令、系统包）不列，
//! 「打开方式」「发送到」这些 CLSID 写死了也不列。只拿掉、不删除，卸载小药箱以后用别的工具也能改回来。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::views::StartupSignature;

/// 列出右键菜单项目的脚本（只读）。
pub const LIST_SCRIPT: &str = "shell/context-menu-list.ps1";
/// 修改日志里右键菜单改动的「功能 ID」。
pub const FEATURE_ID: &str = "context-menu";

/// 拿掉命令用的值（微软文档里的）；恢复时连别的工具写的 `LegacyDisable` 一起删。
pub const HIDE_VALUE: &str = "ProgrammaticAccessOnly";
pub const LEGACY_HIDE_VALUE: &str = "LegacyDisable";
const BLOCKED: &str = r"Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked";

/// 脚本查的范围（`Software\Classes` 下的键）和界面上的说法。
pub const SCOPES: &[(&str, &str)] = &[
    ("*", "文件"),
    ("AllFilesystemObjects", "文件和文件夹"),
    ("Directory", "文件夹"),
    ("Folder", "文件夹"),
    (r"Directory\Background", "文件夹空白处"),
    ("DesktopBackground", "桌面空白处"),
    ("Drive", "磁盘"),
];

/// 不管是谁登记的都不列：Windows 自己的常用项目（CLSID 大写）。程序在 Windows 目录里的本来就不列，这里再挡一道。
const PROTECTED_CLSIDS: &[&str] = &[
    // 打开方式
    "{09799AFB-AD67-11D1-ABCD-00C04FC30936}",
    // 发送到
    "{7BA4C740-9E81-11CF-99D3-00AA004AE837}",
    // 固定到「开始」屏幕、任务栏
    "{A2A9545D-A0C2-42B4-9708-A0B2BADD77C8}",
    "{90AA3A4E-1CBA-4233-B8BB-535773D48449}",
    // 快捷方式的「打开」
    "{00021401-0000-0000-C000-000000000046}",
    // 以前的版本（同一个 CLSID 还管属性里的「以前的版本」页）
    "{596AB062-B4D2-4215-9F74-E9109B0A8153}",
    // 共享（同一个 CLSID 还管属性里的「共享」页）
    "{F81E9010-6EA4-11CE-A7FF-00AA003CA9F6}",
    // 包含到库中
    "{3DAD6C5D-2167-4CAE-9914-F99E41C12CFA}",
    // Microsoft Defender 扫描
    "{09A47860-11B0-4DA5-AFA5-26D86198A780}",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Verb,
    Handler,
    Packaged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Hive {
    /// `HKLM\SOFTWARE\Classes`（所有用户）
    Machine,
    /// 登录用户的 `Software\Classes`
    User,
}

impl Hive {
    fn root(self) -> &'static str {
        match self {
            Self::Machine => "HKLM",
            Self::User => "HKCU",
        }
    }
}

/// 脚本列出的一条登记（路径只在本机显示，不进报告）。
#[derive(Debug, Clone, Deserialize)]
pub struct RawEntry {
    pub kind: Kind,
    #[serde(default = "default_hive")]
    pub hive: Hive,
    #[serde(default)]
    pub scope: String,
    /// 命令或外壳扩展的键名；应用的是清单里的 Verb Id
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub clsid: String,
    /// 命令的 MUIVerb 或默认值、外壳扩展的类名、应用的 DisplayName（可能是 `@…`、`ms-resource:` 这类间接字符串）
    #[serde(default)]
    pub text: String,
    /// 外壳扩展：CLSID 有没有登记（没有就是卸载后留下的空壳）
    #[serde(default = "yes")]
    pub class_found: bool,
    #[serde(default)]
    pub package: String,
    #[serde(default)]
    pub package_name: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub package_system: bool,
    #[serde(default)]
    pub subcommands: bool,
    #[serde(default)]
    pub extended: bool,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub exists: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub company: String,
    #[serde(default)]
    pub signature: StartupSignature,
    #[serde(default)]
    pub signer: String,
    /// 文件在 Windows 目录里（不是 rundll32 这类宿主程序在运行别的文件）
    #[serde(default)]
    pub system: bool,
}

fn default_hive() -> Hive {
    Hive::User
}

fn yes() -> bool {
    true
}

/// 脚本的输出：`{ result: 'ok', items: [...] }`（只有一项时 PowerShell 5.1 会输出成对象，两种都认）。
pub fn parse_list(v: &Value) -> Result<Vec<RawEntry>, String> {
    match v.get("result").and_then(Value::as_str) {
        Some("ok") => {}
        other => return Err(format!("列右键菜单的脚本返回了没有定义的结果：{}", other.unwrap_or("（空）"))),
    }
    let items: Vec<&Value> = match v.get("items") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(list)) => list.iter().collect(),
        Some(obj @ Value::Object(_)) => vec![obj],
        Some(other) => return Err(format!("右键菜单列表的格式不对：{other}")),
    };
    items
        .into_iter()
        .map(|i| serde_json::from_value::<RawEntry>(i.clone()).map_err(|e| format!("右键菜单项目的格式不对：{e}")))
        .filter(|r| !matches!(r, Ok(e) if !valid(e)))
        .collect()
}

fn valid_text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.chars().count() <= max && !s.chars().any(char::is_control)
}

/// 大写、带花括号的 GUID。
fn is_clsid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 38
        && b[0] == b'{'
        && b[37] == b'}'
        && s[1..37].char_indices().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) { c == '-' } else { c.is_ascii_digit() || ('A'..='F').contains(&c) }
        })
}

/// 能拿来拼注册表路径的才要：命令的范围必须是脚本查的那几个，键名不能带反斜杠；CLSID 格式要对。
fn valid(e: &RawEntry) -> bool {
    match e.kind {
        Kind::Verb => SCOPES.iter().any(|(s, _)| *s == e.scope) && valid_text(&e.key, 255) && !e.key.contains('\\'),
        Kind::Handler => SCOPES.iter().any(|(s, _)| *s == e.scope) && is_clsid(&e.clsid),
        Kind::Packaged => is_clsid(&e.clsid),
    }
}

fn is_microsoft(e: &RawEntry) -> bool {
    let signed_by = e.signer.to_ascii_lowercase();
    let company = e.company.to_ascii_lowercase();
    signed_by.starts_with("microsoft") || company.starts_with("microsoft")
}

/// Windows 自带的（不列）：程序在 Windows 目录里、微软的命令和外壳扩展、系统包、写死的几个 CLSID。
/// 应用商店里微软的应用（终端、剪辑）不算：它们是应用，拿掉菜单项不影响系统。
pub fn is_windows_own(e: &RawEntry) -> bool {
    if PROTECTED_CLSIDS.contains(&e.clsid.as_str()) {
        return true;
    }
    match e.kind {
        Kind::Packaged => e.package_system,
        // CLSID 都没登记的外壳扩展：卸载后留下的空壳，谁的都一样，可以拿掉
        Kind::Handler if !e.class_found => false,
        // 对不上程序的（没有命令、类里没写 DLL）不知道是谁加的，不列
        Kind::Verb | Kind::Handler => e.system || is_microsoft(e) || e.path.is_empty(),
    }
}

/// 列表里的一项：一个命令，或者一个外壳扩展（按 CLSID 合并了它登记的几个范围）。
#[derive(Debug, Clone)]
pub struct Group {
    pub target: Target,
    /// 合并前的登记，第一条用来取名字、程序
    pub entries: Vec<RawEntry>,
}

/// 开关写在哪里。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Verb { hive: Hive, scope: String, key: String },
    Extension { clsid: String },
}

impl Target {
    /// 界面用的 ID。
    pub fn id(&self) -> String {
        match self {
            Self::Verb { hive, scope, key } => {
                let hive = if *hive == Hive::Machine { "machine" } else { "user" };
                hex::encode(format!("verb\n{hive}\n{scope}\n{key}"))
            }
            Self::Extension { clsid } => hex::encode(format!("clsid\n{clsid}")),
        }
    }
}

/// 过滤掉 Windows 自带的，外壳扩展和应用的项目按 CLSID 合并，按名字排好。
pub fn group(entries: Vec<RawEntry>) -> Vec<Group> {
    let mut groups: BTreeMap<Target, Vec<RawEntry>> = BTreeMap::new();
    for e in entries.into_iter().filter(|e| !is_windows_own(e)) {
        let target = match e.kind {
            Kind::Verb => Target::Verb { hive: e.hive, scope: e.scope.clone(), key: e.key.clone() },
            Kind::Handler | Kind::Packaged => Target::Extension { clsid: e.clsid.clone() },
        };
        groups.entry(target).or_default().push(e);
    }
    groups
        .into_iter()
        .map(|(target, mut entries)| {
            // 应用的登记排在前面：它们的名字（应用名）比外壳扩展的类名好懂
            entries.sort_by_key(|e| e.kind != Kind::Packaged);
            Group { target, entries }
        })
        .collect()
}

/// 命令的键（`HKLM\SOFTWARE\Classes\…` 或 `HKCU\Software\Classes\…`，HKCU 由引擎换成登录用户的）。
pub fn verb_key(hive: Hive, scope: &str, key: &str) -> String {
    let classes = if hive == Hive::Machine { r"SOFTWARE\Classes" } else { r"Software\Classes" };
    format!(r"{}\{classes}\{scope}\shell\{key}", hive.root())
}

/// 拿掉外壳扩展的地方：`Shell Extensions\Blocked`。
pub fn blocked_key(hive: Hive) -> String {
    format!(r"{}\{BLOCKED}", hive.root())
}

/// 修改日志里的一条记录是不是右键菜单的开关（键是去掉根的部分）。
pub fn is_menu_target(key: &str, name: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    if lower == BLOCKED.to_ascii_lowercase() {
        return is_clsid(&name.to_ascii_uppercase());
    }
    (name.eq_ignore_ascii_case(HIDE_VALUE) || name.eq_ignore_ascii_case(LEGACY_HIDE_VALUE))
        && lower.starts_with(r"software\classes\")
        && lower.contains(r"\shell\")
}

/// 界面上说的范围：「文件、文件夹」这样合起来说，去掉重复的。
pub fn scope_labels(entries: &[RawEntry]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for e in entries {
        let label = match e.kind {
            Kind::Packaged => match e.scope.as_str() {
                "*" => "文件".to_owned(),
                "Directory" => "文件夹".to_owned(),
                r"Directory\Background" => "文件夹空白处".to_owned(),
                other if other.starts_with('.') => format!("{other} 文件"),
                _ => "Windows 11 的新菜单".to_owned(),
            },
            _ => SCOPES.iter().find(|(s, _)| *s == e.scope).map_or_else(|| e.scope.clone(), |(_, l)| (*l).to_owned()),
        };
        if !out.contains(&label) {
            out.push(label);
        }
    }
    out
}

/// 菜单上的字去掉快捷键标记（`&E` 的 `&`；`&&` 是真的 `&`）。
pub fn strip_accelerator(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            if chars.peek() == Some(&'&') {
                out.push('&');
                chars.next();
            }
            continue;
        }
        out.push(c);
    }
    out.trim().to_owned()
}

/// 应用清单里的 DisplayName 是 `ms-resource:` 时，拼成 SHLoadIndirectString 认的样子。
pub fn package_resource(text: &str, package_full_name: &str, package_name: &str) -> Option<String> {
    let rest = text.strip_prefix("ms-resource:")?;
    if package_full_name.is_empty() {
        return None;
    }
    let uri = if rest.starts_with("//") {
        format!("ms-resource:{rest}")
    } else if rest.starts_with('/') {
        format!("ms-resource://{package_name}{rest}")
    } else {
        format!("ms-resource://{package_name}/Resources/{rest}")
    };
    Some(format!("@{{{package_full_name}?{uri}}}"))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn entry(v: Value) -> RawEntry {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn script_output_is_parsed_and_bad_entries_dropped() {
        let v = json!({ "result": "ok", "items": [
            { "kind": "verb", "hive": "machine", "scope": "Directory", "key": "git_shell", "path": "C:\\Program Files\\Git\\git-bash.exe", "exists": true },
            { "kind": "handler", "hive": "machine", "scope": "*", "key": "7-Zip", "clsid": "{23170F69-40C1-278A-1000-000100020000}" },
            { "kind": "packaged", "scope": "Directory", "key": "Cmd", "clsid": "{B298D29A-A6ED-11DE-BA8C-A68E55D89593}", "package": "x" },
            { "kind": "verb", "hive": "machine", "scope": "Unknown", "key": "x" },
            { "kind": "verb", "hive": "user", "scope": "*", "key": "a\\b" },
            { "kind": "handler", "hive": "user", "scope": "*", "key": "bad", "clsid": "{not-a-guid}" }
        ]});
        let items = parse_list(&v).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[2].hive, Hive::User, "应用的项目没写 hive 时按登录用户");
        let one =
            json!({ "result": "ok", "items": { "kind": "verb", "hive": "user", "scope": "Drive", "key": "Scan" } });
        assert_eq!(parse_list(&one).unwrap().len(), 1);
        assert!(parse_list(&json!({ "result": "ok" })).unwrap().is_empty());
        assert!(parse_list(&json!({ "result": "boom" })).is_err());
        assert!(parse_list(&json!({ "result": "ok", "items": [{ "kind": "thing" }] })).is_err());
    }

    #[test]
    fn windows_own_entries_are_not_listed() {
        let own = [
            json!({ "kind": "verb", "hive": "machine", "scope": "Directory", "key": "cmd", "path": "C:\\Windows\\System32\\cmd.exe", "system": true }),
            json!({ "kind": "verb", "hive": "machine", "scope": "Directory", "key": "Powershell", "path": "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe", "signer": "Microsoft Windows", "signature": "valid" }),
            json!({ "kind": "handler", "hive": "machine", "scope": "*", "key": "EPP", "clsid": "{09A47860-11B0-4DA5-AFA5-26D86198A780}", "path": "C:\\Program Files\\Windows Defender\\shellext.dll" }),
            json!({ "kind": "handler", "hive": "user", "scope": "*", "key": "FileSyncEx", "clsid": "{CB3D0F55-BC2C-4C1A-85ED-23ED75B5106B}", "company": "Microsoft Corporation" }),
            json!({ "kind": "packaged", "scope": "*", "key": "x", "clsid": "{11111111-2222-3333-4444-555555555555}", "package_system": true }),
            // 没有程序可以对上号的：不知道是谁加的，不列
            json!({ "kind": "verb", "hive": "machine", "scope": "Folder", "key": "opennewwindow" }),
            json!({ "kind": "handler", "hive": "machine", "scope": "Folder", "key": "x", "clsid": "{21EC2020-3AEA-1069-A2DD-08002B30309D}" }),
        ];
        for v in own {
            assert!(is_windows_own(&entry(v.clone())), "{v}");
        }
        let third = [
            json!({ "kind": "verb", "hive": "machine", "scope": "Directory", "key": "git_shell", "path": "C:\\Program Files\\Git\\git-bash.exe", "signer": "Johannes Schindelin" }),
            json!({ "kind": "handler", "hive": "machine", "scope": "*", "key": "WinRAR", "clsid": "{B41DB860-64E4-11D2-9906-E49FADC173CA}", "path": "C:\\Program Files\\WinRAR\\rarext.dll" }),
            // 卸载后留下的空壳
            json!({ "kind": "handler", "hive": "machine", "scope": "*", "key": "Old", "clsid": "{AAAAAAAA-2222-3333-4444-555555555555}", "class_found": false }),
            // 应用商店里微软的应用也是应用
            json!({ "kind": "packaged", "scope": "Directory", "key": "OpenTerminalHere", "clsid": "{9F156763-7844-4DC4-B2B1-901F640F5155}", "publisher": "Microsoft Corporation" }),
            // rundll32 运行的第三方 DLL
            json!({ "kind": "verb", "hive": "user", "scope": "*", "key": "scan", "path": "C:\\Tools\\scan.dll", "system": false }),
        ];
        for v in third {
            assert!(!is_windows_own(&entry(v.clone())), "{v}");
        }
    }

    #[test]
    fn extensions_are_merged_by_clsid_and_verbs_kept_apart() {
        let winrar = |scope: &str| {
            entry(
                json!({ "kind": "handler", "hive": "machine", "scope": scope, "key": "WinRAR", "clsid": "{B41DB860-64E4-11D2-9906-E49FADC173CA}", "path": "C:\\WinRAR\\rarext.dll" }),
            )
        };
        let packaged = entry(
            json!({ "kind": "packaged", "scope": "*", "key": "WinRAR", "clsid": "{B41DB860-64E4-11D2-9906-E49FADC173CA}", "text": "WinRAR" }),
        );
        let git = |hive: &str| {
            entry(
                json!({ "kind": "verb", "hive": hive, "scope": "Directory", "key": "git_shell", "path": "C:\\Git\\git-bash.exe" }),
            )
        };
        let groups =
            group(vec![winrar("*"), winrar("Directory"), winrar("Drive"), packaged, git("machine"), git("user")]);
        assert_eq!(groups.len(), 3);
        let rar = groups.iter().find(|g| matches!(g.target, Target::Extension { .. })).unwrap();
        assert_eq!(rar.entries.len(), 4);
        assert_eq!(rar.entries[0].kind, Kind::Packaged, "应用的登记排前面，名字取它的");
        assert_eq!(scope_labels(&rar.entries), ["文件", "文件夹", "磁盘"]);
        let ids: Vec<String> = groups.iter().map(|g| g.target.id()).collect();
        assert_eq!(ids.len(), ids.iter().collect::<std::collections::HashSet<_>>().len(), "ID 不重复");
    }

    #[test]
    fn switches_are_written_where_the_entry_lives() {
        assert_eq!(
            verb_key(Hive::Machine, "Directory", "git_shell"),
            r"HKLM\SOFTWARE\Classes\Directory\shell\git_shell"
        );
        assert_eq!(
            verb_key(Hive::User, r"Directory\Background", "VSCode"),
            r"HKCU\Software\Classes\Directory\Background\shell\VSCode"
        );
        assert_eq!(
            blocked_key(Hive::Machine),
            r"HKLM\Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked"
        );
        assert!(is_menu_target(
            r"Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked",
            "{B41DB860-64E4-11D2-9906-E49FADC173CA}"
        ));
        assert!(!is_menu_target(r"Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked", "Other"));
        assert!(is_menu_target(r"SOFTWARE\Classes\*\shell\x", "ProgrammaticAccessOnly"));
        assert!(is_menu_target(r"Software\Classes\Directory\shell\x", "LegacyDisable"));
        assert!(!is_menu_target(r"Software\Classes\Directory\shell\x", "Icon"));
        assert!(!is_menu_target(r"Software\Microsoft\Windows\CurrentVersion\Run", "ProgrammaticAccessOnly"));
    }

    #[test]
    fn menu_text_is_cleaned_up() {
        assert_eq!(strip_accelerator("Edit with &Notepad++"), "Edit with Notepad++");
        assert_eq!(strip_accelerator("Save && Exit"), "Save & Exit");
        assert_eq!(strip_accelerator("用 360 &强力删除"), "用 360 强力删除");
        assert_eq!(
            package_resource("ms-resource:AppName", "NanaZip_5.0.1252.0_x64__gnj4mf6z9tkrc", "NanaZip").as_deref(),
            Some("@{NanaZip_5.0.1252.0_x64__gnj4mf6z9tkrc?ms-resource://NanaZip/Resources/AppName}")
        );
        assert_eq!(
            package_resource("ms-resource://Foo/Resources/Name", "Foo_1_x64__abc", "Foo").as_deref(),
            Some("@{Foo_1_x64__abc?ms-resource://Foo/Resources/Name}")
        );
        assert_eq!(package_resource("NanaZip", "x", "y"), None);
        assert!(is_clsid("{B41DB860-64E4-11D2-9906-E49FADC173CA}"));
        assert!(!is_clsid("{b41db860-64e4-11d2-9906-e49fadc173ca}"), "脚本输出的是大写");
        assert!(!is_clsid("B41DB860-64E4-11D2-9906-E49FADC173CA"));
    }
}
