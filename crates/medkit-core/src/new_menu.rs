//! 「新建」菜单：软件在右键「新建」里加的项（新建 Word 文档、新建 WPS 表格……），不用的能关掉，随时能恢复。
//!
//! 资源管理器从扩展名下的 ShellNew 键生成「新建」菜单（见 scripts/shell/new-menu-list.ps1）：`<.ext>\ShellNew`，
//! 或者 `<.ext>\<ProgID>\ShellNew`（用扩展名当前的 ProgID 下的那个）；键里有 FileName、Command、Data、NullFile
//! 其中一个值才算一项（Handler 是生成它的 COM 类，也算上）。
//!
//! 关掉：把这些值改名（前面加 [`HIDDEN_PREFIX`]，类型和数据不变），资源管理器就不认这一项了；恢复：改回来。
//! 每个值的改名是两条注册表修改（写新名字的值、删掉旧名字的值），都记进修改日志，能撤销。一个扩展名下
//! 所有 ShellNew 键（机器的和用户的、直接的和 ProgID 下的）一起改，免得换了默认程序以后又冒出来。
//! 改完删掉资源管理器对「新建」菜单的缓存（[`CACHE_KEY`] 里的值，它会重建），不记进修改日志：缓存不是设置，
//! 撤销时也不该放回旧的。

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

pub use crate::context_menu::Hive;

pub const LIST_SCRIPT: &str = "shell/new-menu-list.ps1";
/// 修改日志里的功能 ID
pub const FEATURE_ID: &str = "new-menu";
/// 关掉时给值改的名字的前缀
pub const HIDDEN_PREFIX: &str = "MedkitHidden.";
/// 让一项出现在「新建」菜单里的值
pub const DEFINING: &[&str] = &["FileName", "Command", "Data", "NullFile", "Handler"];
/// 资源管理器对「新建」菜单的缓存（登录用户的注册表里）
pub const CACHE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Discardable\PostSetup\ShellNew";
pub const CACHE_VALUES: &[&str] = &["Classes", "~reserved~"];
/// 不列、也不给关的：快捷方式和库是 Windows 自己的基本功能（文件夹不在扩展名下，脚本不列）
const PROTECTED: &[&str] = &[".lnk", ".library-ms"];
/// Windows 自带的（界面上标出来，也能关）
const WINDOWS_OWN: &[&str] = &[".bmp", ".contact", ".jnt", ".rtf", ".txt", ".zip"];

/// 脚本列出的一个 ShellNew 键。
#[derive(Debug, Clone, Deserialize)]
pub struct RawEntry {
    pub hive: Hive,
    pub ext: String,
    /// ProgID 下的写法里的 ProgID；直接写在扩展名下是空的
    #[serde(default)]
    pub progid: String,
    /// 扩展名现在的 ProgID（用户的优先）
    #[serde(default)]
    pub current_progid: String,
    /// 键里所有值的名字（只有一个时 PowerShell 5.1 可能输出成字符串）
    #[serde(default, deserialize_with = "one_or_many")]
    pub values: Vec<String>,
    #[serde(default)]
    pub item_name: String,
    #[serde(default)]
    pub menu_text: String,
    /// 现在的 ProgID 的类型名（FriendlyTypeName，没有就是默认值）
    #[serde(default)]
    pub type_name: String,
}

fn one_or_many<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Null => Vec::new(),
        Value::String(s) => vec![s],
        Value::Array(items) => items.into_iter().filter_map(|v| v.as_str().map(str::to_owned)).collect(),
        _ => Vec::new(),
    })
}

/// 脚本的输出：`{ result: 'ok', items: [...] }`（只有一项时 PowerShell 5.1 会输出成对象，两种都认）。
pub fn parse_list(v: &Value) -> Result<Vec<RawEntry>, String> {
    match v.get("result").and_then(Value::as_str) {
        Some("ok") => {}
        other => return Err(format!("列「新建」菜单的脚本返回了没有定义的结果：{}", other.unwrap_or("（空）"))),
    }
    let items: Vec<&Value> = match v.get("items") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(list)) => list.iter().collect(),
        Some(obj @ Value::Object(_)) => vec![obj],
        Some(other) => return Err(format!("「新建」菜单列表的格式不对：{other}")),
    };
    items
        .into_iter()
        .map(|i| serde_json::from_value::<RawEntry>(i.clone()).map_err(|e| format!("「新建」菜单项目的格式不对：{e}")))
        .filter(|r| !matches!(r, Ok(e) if !valid(e)))
        .collect()
}

/// 扩展名、ProgID 只能是注册表里正常的名字（拼进键路径里用）
fn valid(e: &RawEntry) -> bool {
    let name_ok = |s: &str| !s.contains('\\') && !s.chars().any(char::is_control) && s.chars().count() <= 200;
    e.ext.starts_with('.') && e.ext.len() > 1 && name_ok(&e.ext) && name_ok(&e.progid)
}

/// 一个 ShellNew 键。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub hive: Hive,
    pub ext: String,
    pub progid: String,
    pub values: Vec<String>,
}

impl Location {
    /// 完整的键（`HKLM\SOFTWARE\Classes\…` 或 `HKCU\Software\Classes\…`，HKCU 由引擎换成登录用户的）。
    pub fn key(&self) -> String {
        let root = match self.hive {
            Hive::Machine => r"HKLM\SOFTWARE\Classes",
            Hive::User => r"HKCU\Software\Classes",
        };
        if self.progid.is_empty() {
            format!(r"{root}\{}\ShellNew", self.ext)
        } else {
            format!(r"{root}\{}\{}\ShellNew", self.ext, self.progid)
        }
    }

    /// 让这一项出现的值（按键里的写法，大小写可能和 [`DEFINING`] 不同）
    pub fn defining(&self) -> Vec<&str> {
        self.values.iter().map(String::as_str).filter(|n| is_defining(n)).collect()
    }

    /// 关掉时改过名的值
    pub fn hidden(&self) -> Vec<&str> {
        self.values.iter().map(String::as_str).filter(|n| original_name(n).is_some()).collect()
    }

    /// 资源管理器用的是这个键：直接写在扩展名下的，或者扩展名现在的 ProgID 下的
    pub fn counts(&self, current_progid: &str) -> bool {
        self.progid.is_empty() || self.progid.eq_ignore_ascii_case(current_progid)
    }
}

fn is_defining(name: &str) -> bool {
    DEFINING.iter().any(|d| d.eq_ignore_ascii_case(name))
}

/// 关掉时的新名字
pub fn hidden_name(name: &str) -> String {
    format!("{HIDDEN_PREFIX}{name}")
}

/// 关掉时改过名的值原来的名字
pub fn original_name(name: &str) -> Option<&str> {
    let prefix = name.get(..HIDDEN_PREFIX.len())?;
    let rest = &name[HIDDEN_PREFIX.len()..];
    (prefix.eq_ignore_ascii_case(HIDDEN_PREFIX) && is_defining(rest)).then_some(rest)
}

/// 一个扩展名在「新建」菜单里的那一项。
#[derive(Debug, Clone)]
pub struct Group {
    /// 小写的扩展名，界面改开关时原样传回来
    pub ext: String,
    pub current_progid: String,
    pub locations: Vec<Location>,
    /// 菜单上的字：MenuText、类型名、ItemName（新建出来的文件的名字），可能是 `@文件,-编号`，引擎解开
    pub menu_text: String,
    pub item_name: String,
    pub type_name: String,
}

impl Group {
    /// 现在显示：资源管理器用的键里有让它出现的值
    pub fn visible(&self) -> bool {
        self.locations.iter().any(|l| l.counts(&self.current_progid) && !l.defining().is_empty())
    }

    /// 被关掉了（小药箱或者之前用小药箱改的）
    pub fn hidden(&self) -> bool {
        !self.visible() && self.locations.iter().any(|l| !l.hidden().is_empty())
    }

    pub fn windows_own(&self) -> bool {
        WINDOWS_OWN.contains(&self.ext.as_str())
    }

    /// 在谁的注册表里：「所有用户」「当前用户」或者两个都有
    pub fn scope(&self) -> &'static str {
        let machine = self.locations.iter().any(|l| l.hive == Hive::Machine);
        let user = self.locations.iter().any(|l| l.hive == Hive::User);
        match (machine, user) {
            (true, true) => "所有用户和当前用户",
            (false, true) => "当前用户",
            _ => "所有用户",
        }
    }
}

/// 按扩展名合起来；不列 [`PROTECTED`] 的，也不列既没显示、也没被关掉的（比如只在别的 ProgID 下有、资源管理器不用的）。
pub fn group(entries: Vec<RawEntry>) -> Vec<Group> {
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    for e in entries {
        let ext = e.ext.to_ascii_lowercase();
        if PROTECTED.contains(&ext.as_str()) {
            continue;
        }
        let counts = e.progid.is_empty() || e.progid.eq_ignore_ascii_case(&e.current_progid);
        let g = groups.entry(ext.clone()).or_insert_with(|| Group {
            ext: ext.clone(),
            current_progid: e.current_progid.clone(),
            locations: Vec::new(),
            menu_text: String::new(),
            item_name: String::new(),
            type_name: e.type_name.clone(),
        });
        // 菜单上的字取资源管理器用的那个键里的
        if counts {
            if g.menu_text.is_empty() {
                g.menu_text = e.menu_text.clone();
            }
            if g.item_name.is_empty() {
                g.item_name = e.item_name.clone();
            }
        }
        g.locations.push(Location { hive: e.hive, ext: e.ext.clone(), progid: e.progid, values: e.values });
    }
    groups.into_values().filter(|g| g.visible() || g.hidden()).collect()
}

/// 修改日志里的一条记录是不是「新建」菜单的开关（键是去掉根的部分）。
pub fn is_new_menu_target(key: &str, name: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.starts_with(r"software\classes\.")
        && lower.ends_with(r"\shellnew")
        && (is_defining(name) || original_name(name).is_some())
}

/// 修改日志里的键对应的扩展名（`Software\Classes\.docx\…\ShellNew` → `.docx`）。
pub fn ext_of_key(key: &str) -> Option<String> {
    let lower = key.to_ascii_lowercase();
    let ext = lower.strip_prefix(r"software\classes\")?.split('\\').next()?;
    ext.starts_with('.').then(|| ext.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn raw(hive: &str, ext: &str, progid: &str, current: &str, values: Value) -> Value {
        json!({ "hive": hive, "ext": ext, "progid": progid, "current_progid": current, "values": values,
                "item_name": "", "menu_text": "", "type_name": "" })
    }

    #[test]
    fn a_single_item_and_a_single_value_are_accepted() {
        let v = json!({ "result": "ok", "items": raw("machine", ".txt", "", "txtfile", json!("NullFile")) });
        let list = parse_list(&v).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].values, vec!["NullFile"]);
        assert!(parse_list(&json!({ "result": "nope" })).is_err());
        let bad = json!({ "result": "ok", "items": [raw("machine", r".a\b", "", "", json!([]))] });
        assert!(parse_list(&bad).unwrap().is_empty(), "扩展名里不能有反斜杠");
    }

    #[test]
    fn only_the_key_explorer_uses_decides_what_is_shown() {
        let list = parse_list(&json!({ "result": "ok", "items": [
            // Word 被 WPS 换掉了：资源管理器用 KWPS 那个
            raw("machine", ".docx", "Word.Document.12", "KWPS.Document.12", json!(["NullFile"])),
            raw("machine", ".docx", "KWPS.Document.12", "KWPS.Document.12", json!(["FileName"])),
            // 只在别的 ProgID 下有：资源管理器不用，不列
            raw("machine", ".doc", "Word.Document.8", "KWPS.Document.9", json!(["NullFile"])),
            // 小药箱关掉的
            raw("user", ".xmind", "", "", json!(["MedkitHidden.NullFile", "ItemName"])),
            // 保护的
            raw("machine", ".lnk", "", "lnkfile", json!(["NullFile", "Handler"])),
            raw("machine", ".zip", "CompressedFolder", "CompressedFolder", json!(["Data", "ItemName"])),
        ] }))
        .unwrap();
        let groups = group(list);
        let exts: Vec<&str> = groups.iter().map(|g| g.ext.as_str()).collect();
        assert_eq!(exts, vec![".docx", ".xmind", ".zip"]);
        assert!(groups[0].visible() && groups[0].locations.len() == 2);
        assert!(!groups[1].visible() && groups[1].hidden());
        assert_eq!(groups[1].scope(), "当前用户");
        assert!(groups[2].windows_own() && !groups[0].windows_own());
    }

    #[test]
    fn keys_and_names() {
        let l = Location {
            hive: Hive::Machine,
            ext: ".zip".into(),
            progid: "CompressedFolder".into(),
            values: vec!["Data".into(), "ItemName".into(), "MedkitHidden.command".into()],
        };
        assert_eq!(l.key(), r"HKLM\SOFTWARE\Classes\.zip\CompressedFolder\ShellNew");
        assert_eq!(l.defining(), vec!["Data"]);
        assert_eq!(l.hidden(), vec!["MedkitHidden.command"]);
        let u = Location { hive: Hive::User, ext: ".txt".into(), progid: String::new(), values: Vec::new() };
        assert_eq!(u.key(), r"HKCU\Software\Classes\.txt\ShellNew");
        assert_eq!(hidden_name("command"), "MedkitHidden.command");
        assert_eq!(original_name("medkithidden.NullFile"), Some("NullFile"));
        assert_eq!(original_name("MedkitHidden.ItemName"), None, "只认让它出现的那几个值");
        assert_eq!(original_name("Med"), None);
    }

    #[test]
    fn journal_entries_are_recognized() {
        assert!(is_new_menu_target(r"SOFTWARE\Classes\.docx\Word.Document.12\ShellNew", "NullFile"));
        assert!(is_new_menu_target(r"Software\Classes\.txt\ShellNew", "MedkitHidden.NullFile"));
        assert!(!is_new_menu_target(r"Software\Classes\.txt\ShellNew", "ItemName"));
        assert!(!is_new_menu_target(r"Software\Classes\txtfile\shell\open", "NullFile"));
        assert_eq!(ext_of_key(r"SOFTWARE\Classes\.DOCX\Word.Document.12\ShellNew").as_deref(), Some(".docx"));
        assert_eq!(ext_of_key(r"Software\Classes\CLSID\x"), None);
    }
}
