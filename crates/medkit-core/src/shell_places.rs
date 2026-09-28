//! 资源管理器里软件加的图标：左边导航栏最上面一层（和「此电脑」「网络」并列的网盘、OneDrive）和「此电脑」里的
//! （WPS 云文档、百度网盘这些）。不用的能隐藏，随时能恢复；软件本身不受影响。
//!
//! 在哪登记（见 scripts/shell/shell-places-list.ps1）：
//! - 导航栏：`…\Explorer\Desktop\NameSpace\{CLSID}`（所有用户的在 HKLM，当前用户的在 HKCU）。显示不显示看这个
//!   CLSID 键里的 [`PINNED_VALUE`]：1 显示，0 不显示（微软《Integrate a Cloud Storage Provider》：设成 0 不会删掉
//!   扩展，只是不显示）。资源管理器读的是合并视图 HKEY_CLASSES_ROOT：用户的 `Software\Classes\CLSID\{CLSID}`
//!   在，就用它里面的值（机器的那份键里的值看不到了），不在才用机器的（微软《Merged View of HKEY_CLASSES_ROOT》）。
//!   32 位程序（它们的打开、保存对话框）读的是 `WOW6432Node\CLSID` 下的那一份。
//! - 「此电脑」：`…\Explorer\MyComputer\NameSpace\{CLSID}`；登录用户的 [`HIDE_PC_KEY`] 里有一个名字是 `{CLSID}`、
//!   值是 1 的 DWORD，就不显示。
//!
//! 隐藏：导航栏的，在用户的那份 CLSID 键里写 0（没有这个键就新建一个、只放这一个值，和资源管理器「显示库」选项的
//! 做法一样），只影响当前用户；32 位程序看到的那一份也一样改。「此电脑」的，写 HideMyComputerIcons。
//! 恢复：导航栏的，用户那份键是只放了这一个值的（多半是隐藏时新建的）、机器的那份是显示的，就删掉这个值和空键，
//! 回到软件自己登记的样子；不然写 1（只有机器的那份、是 0 的，写在机器的那份里）。「此电脑」的删掉那个值。
//! 每一处都记进修改日志，能撤销。
//!
//! 只列软件加的：Windows 自己的基本位置（[`PROTECTED`]，还有程序在 Windows 目录里、又不是指向一个文件夹的）不列；
//! [`WINDOWS_HIDEABLE`] 里的几个 Windows 自带的列出来（界面上标明），也能隐藏。

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

pub use crate::context_menu::Hive;
pub use crate::views::ShellPlace;

pub const LIST_SCRIPT: &str = "shell/shell-places-list.ps1";
/// 修改日志里的功能 ID
pub const FEATURE_ID: &str = "shell-places";
/// CLSID 键里决定导航栏显示不显示的值（DWORD）
pub const PINNED_VALUE: &str = "System.IsPinnedToNameSpaceTree";
/// 「此电脑」里不显示的图标（登录用户的注册表里；值的名字是 CLSID，DWORD 1）
pub const HIDE_PC_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\HideMyComputerIcons";

/// Windows 自带、列出来也能隐藏的（界面上标出来）
const WINDOWS_HIDEABLE: &[&str] = &[
    "{018D5C66-4533-4307-9B53-224DE2ED1FE6}", // OneDrive
    "{E88865EA-0E1C-4E20-9AA6-EDCD0212C87C}", // 图库（Windows 11）
    "{0DB7E03F-FC29-4DC6-9020-FF41B59E513A}", // 3D 对象（Windows 10）
    "{B2B4A4D1-2754-4140-A2EB-9A76D9D7CDC6}", // Linux（装了 WSL 才有）
];

/// Windows 自己的基本位置：不列，也不给隐藏（看上去像软件加的也一样）
const PROTECTED: &[&str] = &[
    "{20D04FE0-3AEA-1069-A2D8-08002B30309D}", // 此电脑
    "{F02C1A0D-BE21-4350-88B0-7367FC96EF3C}", // 网络
    "{645FF040-5081-101B-9F08-00AA002F954E}", // 回收站
    "{F874310E-B6B7-47DC-BC84-B9E6B38F5903}", // 主文件夹（Windows 11）
    "{679F85CB-0220-4080-B29B-5540CC05AAB6}", // 快速访问（Windows 10）
    "{031E4825-7B94-4DC3-B131-E946B44C8DD5}", // 库（资源管理器的选项里有开关）
    "{59031A47-3F72-44A7-89C5-5595FE6B30EE}", // 用户文件夹
    "{26EE0668-A00A-44D7-9371-BEB064C98683}", // 控制面板
    "{5399E694-6CE5-4D6C-8FCE-1D8870FDCBA0}", // 控制面板
    "{21EC2020-3AEA-1069-A2DD-08002B30309D}", // 所有控制面板项
    "{B4BFCC3A-DB2C-424C-B029-7FE99A87C641}", // 桌面
    "{D3162B92-9365-467A-956B-92703ACA08AF}", // 文档
    "{A8CDFF1C-4878-43BE-B5FD-F8091C1C60D0}", // 文档（旧的）
    "{088E3905-0323-4B02-9826-5D99428E115F}", // 下载
    "{374DE290-123F-4565-9164-39C4925E467B}", // 下载（旧的）
    "{3DFDF296-DBEC-4FB4-81D1-6A3438BCF4DE}", // 音乐
    "{1CF1260C-4DD0-4EBB-811F-33C572699FDE}", // 音乐（旧的）
    "{24AD3AD4-A569-4530-98E1-AB02F9417AA8}", // 图片
    "{3ADD1653-EB32-4CB0-BBD7-DFA0ABB5ACCA}", // 图片（旧的）
    "{F86FA3AB-70D2-4FC7-9C99-FCBF05467F3A}", // 视频
    "{A0953C92-50DC-43BF-BE83-3742FED03C9C}", // 视频（旧的）
];

/// 脚本读到的一份 CLSID 键。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct ClassKey {
    pub exists: bool,
    /// [`PINNED_VALUE`]，没有是 -1
    pub pinned: i64,
    /// 键里只有这一个值，没有别的值和子键
    pub only_pinned: bool,
}

impl Default for ClassKey {
    fn default() -> Self {
        Self { exists: false, pinned: -1, only_pinned: false }
    }
}

/// 脚本列出的一处登记。
#[derive(Debug, Clone, Deserialize)]
pub struct RawPlace {
    pub place: ShellPlace,
    pub clsid: String,
    /// 登记在谁的注册表里
    pub hive: Hive,
    /// NameSpace 键的默认值
    #[serde(default)]
    pub name: String,
    /// CLSID 键的默认值和 LocalizedString，可能是 `@文件,-编号`，引擎解开
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub localized: String,
    #[serde(default)]
    pub user: ClassKey,
    #[serde(default)]
    pub machine: ClassKey,
    #[serde(default)]
    pub wow_user: ClassKey,
    #[serde(default)]
    pub wow_machine: ClassKey,
    /// 程序（InProcServer32）在 Windows 目录里
    #[serde(default)]
    pub system_server: bool,
    /// 指向一个文件夹（网盘的做法）
    #[serde(default)]
    pub folder_target: bool,
}

/// 脚本的输出：`{ result: 'ok', items: [...] }`（只有一项时 PowerShell 5.1 会输出成对象，两种都认）。
pub fn parse_list(v: &Value) -> Result<Vec<RawPlace>, String> {
    match v.get("result").and_then(Value::as_str) {
        Some("ok") => {}
        other => return Err(format!("列资源管理器图标的脚本返回了没有定义的结果：{}", other.unwrap_or("（空）"))),
    }
    let items: Vec<&Value> = match v.get("items") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(list)) => list.iter().collect(),
        Some(obj @ Value::Object(_)) => vec![obj],
        Some(other) => return Err(format!("资源管理器图标列表的格式不对：{other}")),
    };
    let mut out = Vec::new();
    for i in items {
        let mut raw: RawPlace =
            serde_json::from_value(i.clone()).map_err(|e| format!("资源管理器图标的格式不对：{e}"))?;
        // 拼进键路径、值名里用，只认正常的 CLSID
        let Some(clsid) = normalize_clsid(&raw.clsid) else {
            continue;
        };
        raw.clsid = clsid;
        out.push(raw);
    }
    Ok(out)
}

/// `{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}` 换成大写；不是这个样子的返回 `None`。
pub fn normalize_clsid(s: &str) -> Option<String> {
    let inner = s.strip_prefix('{')?.strip_suffix('}')?;
    let parts: Vec<&str> = inner.split('-').collect();
    let ok = parts.len() == 5
        && parts.iter().zip([8, 4, 4, 4, 12]).all(|(p, n)| p.len() == n && p.chars().all(|c| c.is_ascii_hexdigit()));
    ok.then(|| format!("{{{}}}", inner.to_ascii_uppercase()))
}

/// 一个图标（同一个 CLSID 在机器的和用户的注册表里都登记了，算一个）。
#[derive(Debug, Clone)]
pub struct Group {
    pub place: ShellPlace,
    /// 大写的 CLSID（带花括号）
    pub clsid: String,
    pub name: String,
    pub title: String,
    pub localized: String,
    pub user: ClassKey,
    pub machine: ClassKey,
    pub wow_user: ClassKey,
    pub wow_machine: ClassKey,
}

impl Group {
    pub fn id(&self) -> String {
        let place = match self.place {
            ShellPlace::Nav => "nav",
            ShellPlace::Pc => "pc",
        };
        format!("{place}:{}", self.clsid)
    }

    pub fn windows_own(&self) -> bool {
        WINDOWS_HIDEABLE.contains(&self.clsid.as_str())
    }
}

/// 按位置和 CLSID 合起来，只留软件加的（和 [`WINDOWS_HIDEABLE`] 里的）。不列：没有 CLSID 键的（资源管理器显示
/// 不出来）；导航栏里没有 [`PINNED_VALUE`] 的（本来就不在导航栏里显示）。
pub fn group(raw: Vec<RawPlace>) -> Vec<Group> {
    let mut groups: BTreeMap<(ShellPlace, String), Group> = BTreeMap::new();
    for r in raw {
        let hideable = WINDOWS_HIDEABLE.contains(&r.clsid.as_str());
        let windows_core = r.system_server && !r.folder_target;
        if PROTECTED.contains(&r.clsid.as_str()) || (windows_core && !hideable) {
            continue;
        }
        if !r.user.exists && !r.machine.exists {
            continue;
        }
        let effective = if r.user.exists { r.user } else { r.machine };
        if r.place == ShellPlace::Nav && effective.pinned < 0 {
            continue;
        }
        let g = groups.entry((r.place, r.clsid.clone())).or_insert_with(|| Group {
            place: r.place,
            clsid: r.clsid.clone(),
            name: String::new(),
            title: r.title.clone(),
            localized: r.localized.clone(),
            user: r.user,
            machine: r.machine,
            wow_user: r.wow_user,
            wow_machine: r.wow_machine,
        });
        // 名字取用户登记的那个（和资源管理器一样，用户的优先）
        if g.name.is_empty() || r.hive == Hive::User {
            g.name = r.name;
        }
    }
    groups.into_values().collect()
}

/// 一份 CLSID 键的完整路径（HKCU 由引擎换成登录用户的）；`wow` 是 32 位程序看的那一份。
pub fn class_key(hive: Hive, wow: bool, clsid: &str) -> String {
    let root = match hive {
        Hive::Machine => r"HKLM\SOFTWARE\Classes",
        Hive::User => r"HKCU\Software\Classes",
    };
    let wow = if wow { r"\WOW6432Node" } else { "" };
    format!(r"{root}{wow}\CLSID\{clsid}")
}

/// 一处（64 位的或者 32 位程序看的）[`PINNED_VALUE`] 现在的样子，引擎按注册表里现在的值读。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pinned {
    /// 用户的那份 CLSID 键在不在（在的话资源管理器只看它里面的值）
    pub user_exists: bool,
    pub user: Option<u64>,
    pub machine: Option<u64>,
}

impl Pinned {
    /// 资源管理器看到的值
    pub fn effective(&self) -> Option<u64> {
        if self.user_exists { self.user } else { self.machine }
    }

    pub fn shown(&self) -> bool {
        self.effective().is_some_and(|v| v != 0)
    }
}

/// 一处要怎么改。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// 在用户的或者机器的那份键里写这个值
    Write(Hive, u32),
    /// 删掉用户那份键里的值，键空了一起删掉（回到软件自己登记的样子）
    DropUser,
}

/// 要让这一处显示（`show`）或者不显示，怎么改；已经是那样了（或者这一处根本没有这个值）返回 `None`。
/// `only_pinned`：用户的那份键里只有这一个值（列表时读的）。
pub fn plan(p: Pinned, only_pinned: bool, show: bool) -> Option<Change> {
    match (show, p.effective()) {
        (false, Some(v)) if v != 0 => Some(Change::Write(Hive::User, 0)),
        (true, Some(0)) if p.user_exists => {
            if only_pinned && p.machine.is_some_and(|m| m != 0) {
                Some(Change::DropUser)
            } else {
                Some(Change::Write(Hive::User, 1))
            }
        }
        (true, Some(0)) => Some(Change::Write(Hive::Machine, 1)),
        _ => None,
    }
}

/// 修改日志里的一条记录是不是这里的开关（键是去掉根的部分）；是的话返回 CLSID（大写）。
pub fn target_clsid(key: &str, name: &str) -> Option<String> {
    if key.eq_ignore_ascii_case(HIDE_PC_KEY) {
        return normalize_clsid(name);
    }
    if !name.eq_ignore_ascii_case(PINNED_VALUE) {
        return None;
    }
    let lower = key.to_ascii_lowercase();
    let rest = lower.strip_prefix(r"software\classes\")?;
    let rest = rest.strip_prefix(r"wow6432node\").unwrap_or(rest);
    normalize_clsid(rest.strip_prefix(r"clsid\")?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const WPS: &str = "{5FCD4425-CA3A-48F4-A57C-B8A75C32ACB1}";

    fn raw(place: &str, clsid: &str, hive: &str, extra: Value) -> Value {
        let mut v = json!({ "place": place, "clsid": clsid, "hive": hive, "name": "", "title": "", "localized": "",
                            "system_server": false, "folder_target": false });
        v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        v
    }

    fn key(pinned: i64, only_pinned: bool) -> Value {
        json!({ "exists": true, "pinned": pinned, "only_pinned": only_pinned })
    }

    #[test]
    fn a_single_item_is_accepted_and_bad_clsids_are_dropped() {
        let v = json!({ "result": "ok", "items": raw("pc", &WPS.to_lowercase(), "user", json!({ "machine": key(-1, false) })) });
        let list = parse_list(&v).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].clsid, WPS, "CLSID 换成大写");
        assert!(list[0].machine.exists && !list[0].user.exists, "没给的键算不在");
        assert_eq!(list[0].user.pinned, -1);
        let bad = json!({ "result": "ok", "items": [raw("pc", r"{5FCD4425-CA3A-48F4-A57C-B8A75C32ACB1}\x", "user", json!({}))] });
        assert!(parse_list(&bad).unwrap().is_empty());
        assert!(parse_list(&json!({ "result": "nope" })).is_err());
        assert_eq!(
            normalize_clsid("{0db7e03f-fc29-4dc6-9020-ff41b59e513a}").as_deref(),
            Some("{0DB7E03F-FC29-4DC6-9020-FF41B59E513A}")
        );
        assert_eq!(normalize_clsid("{0db7e03f-fc29-4dc6-9020-ff41b59e513}"), None);
        assert_eq!(normalize_clsid("0db7e03f-fc29-4dc6-9020-ff41b59e513a"), None);
    }

    #[test]
    fn only_items_programs_added_are_listed() {
        let list = parse_list(&json!({ "result": "ok", "items": [
            // 网盘：程序是 shell32，指向一个文件夹
            raw("nav", "{11111111-2222-3333-4444-555555555555}", "user",
                json!({ "user": key(1, false), "system_server": true, "folder_target": true, "name": "某网盘" })),
            // 同一个也登记在所有用户的注册表里：算一个，名字用用户的
            raw("nav", "{11111111-2222-3333-4444-555555555555}", "machine", json!({ "user": key(1, false), "name": "旧名字" })),
            // 没有 System.IsPinnedToNameSpaceTree：本来就不在导航栏里
            raw("nav", "{22222222-2222-3333-4444-555555555555}", "machine", json!({ "machine": key(-1, false) })),
            // 用户的那份键在但是没有这个值：机器的那份里的值资源管理器看不到
            raw("nav", "{33333333-2222-3333-4444-555555555555}", "machine",
                json!({ "user": { "exists": true, "pinned": -1 }, "machine": key(1, false) })),
            // Windows 自己的：程序在 Windows 目录里，不指向文件夹
            raw("nav", "{44444444-2222-3333-4444-555555555555}", "machine", json!({ "machine": key(1, false), "system_server": true })),
            // 保护的
            raw("nav", "{F874310E-B6B7-47DC-BC84-B9E6B38F5903}", "machine", json!({ "machine": key(1, false) })),
            raw("pc", "{D3162B92-9365-467A-956B-92703ACA08AF}", "machine", json!({ "machine": key(-1, false) })),
            // Windows 自带、能隐藏的
            raw("nav", "{E88865EA-0E1C-4E20-9AA6-EDCD0212C87C}", "machine", json!({ "machine": key(1, false), "system_server": true })),
            raw("pc", "{0DB7E03F-FC29-4DC6-9020-FF41B59E513A}", "machine", json!({ "machine": key(-1, false), "system_server": true })),
            // 「此电脑」里的不看 System.IsPinnedToNameSpaceTree
            raw("pc", WPS, "user", json!({ "user": key(-1, false) })),
            // 没有 CLSID 键：显示不出来
            raw("pc", "{55555555-2222-3333-4444-555555555555}", "user", json!({})),
        ] }))
        .unwrap();
        let groups = group(list);
        let ids: Vec<String> = groups.iter().map(Group::id).collect();
        assert_eq!(
            ids,
            vec![
                "nav:{11111111-2222-3333-4444-555555555555}",
                "nav:{E88865EA-0E1C-4E20-9AA6-EDCD0212C87C}",
                "pc:{0DB7E03F-FC29-4DC6-9020-FF41B59E513A}",
                format!("pc:{WPS}").as_str(),
            ]
        );
        assert_eq!(groups[0].name, "某网盘");
        assert!(!groups[0].windows_own() && groups[1].windows_own() && groups[2].windows_own());
    }

    #[test]
    fn keys() {
        assert_eq!(class_key(Hive::User, false, WPS), format!(r"HKCU\Software\Classes\CLSID\{WPS}"));
        assert_eq!(class_key(Hive::Machine, true, WPS), format!(r"HKLM\SOFTWARE\Classes\WOW6432Node\CLSID\{WPS}"));
    }

    #[test]
    fn hiding_writes_the_users_copy_and_showing_goes_back_to_what_the_program_registered() {
        let p = |user_exists, user, machine| Pinned { user_exists, user, machine };
        // 只有机器的那份：隐藏时新建用户的那份
        assert_eq!(plan(p(false, None, Some(1)), false, false), Some(Change::Write(Hive::User, 0)));
        // 用户的那份（OneDrive）：改它
        assert_eq!(plan(p(true, Some(1), None), false, false), Some(Change::Write(Hive::User, 0)));
        // 隐藏时新建的：恢复时删掉
        assert_eq!(plan(p(true, Some(0), Some(1)), true, true), Some(Change::DropUser));
        // 用户的那份里还有别的东西，或者机器的那份也是 0：写 1
        assert_eq!(plan(p(true, Some(0), Some(1)), false, true), Some(Change::Write(Hive::User, 1)));
        assert_eq!(plan(p(true, Some(0), Some(0)), true, true), Some(Change::Write(Hive::User, 1)));
        // 只有机器的那份、是 0：写在机器的那份里
        assert_eq!(plan(p(false, None, Some(0)), false, true), Some(Change::Write(Hive::Machine, 1)));
        // 已经是那样了、这一处没有这个值：不改
        assert_eq!(plan(p(true, Some(0), None), false, false), None);
        assert_eq!(plan(p(false, None, Some(1)), false, true), None);
        assert_eq!(plan(p(true, None, Some(1)), false, false), None, "用户的那份在、没有这个值：本来就不显示");
        assert_eq!(plan(p(false, None, None), false, true), None);
        assert!(p(false, None, Some(2)).shown() && !p(true, None, Some(1)).shown());
    }

    #[test]
    fn journal_entries_are_recognized() {
        let clsid = Some(WPS.to_owned());
        assert_eq!(target_clsid(HIDE_PC_KEY, &WPS.to_lowercase()), clsid);
        assert_eq!(target_clsid(&format!(r"SOFTWARE\Classes\CLSID\{WPS}"), PINNED_VALUE), clsid);
        assert_eq!(
            target_clsid(&format!(r"Software\Classes\WOW6432Node\CLSID\{WPS}"), "system.ispinnedtonamespacetree"),
            clsid
        );
        assert_eq!(target_clsid(&format!(r"Software\Classes\CLSID\{WPS}"), "SortOrderIndex"), None);
        assert_eq!(target_clsid(&format!(r"Software\Classes\CLSID\{WPS}\ShellFolder"), PINNED_VALUE), None);
        assert_eq!(target_clsid(HIDE_PC_KEY, "NotAClsid"), None);
    }
}
