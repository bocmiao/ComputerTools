//! 资源管理器里多出来的图标在假系统上的行为：列哪些、名字怎么来，隐藏和恢复时改哪一份键，32 位程序看的那一份，
//! 「此电脑」里的，修改日志，撤销，只认列表里的项目。

use std::sync::Arc;

use medkit_core::Engine;
use medkit_core::catalog::{Catalog, CatalogData};
use medkit_core::journal::Journal;
use medkit_core::platform::Platform;
use medkit_core::platform::mock::MockPlatform;
use medkit_core::registry::{RegRoot, RegValue};
use medkit_core::script::MockRunner;
use medkit_core::shell_places::{self, HIDE_PC_KEY, PINNED_VALUE};
use medkit_core::views::{FeatureStateKind, ShellPlace, ShellPlaceItem};
use serde_json::{Value, json};

const SID: &str = "S-1-5-21-1000-2000-3000-1001";
/// 网盘：登记在当前用户的注册表里（OneDrive 也是这样），32 位程序看的那一份也有
const CLOUD: &str = "{11111111-2222-3333-4444-555555555555}";
/// 图库：Windows 自带，只在机器的注册表里
const GALLERY: &str = "{E88865EA-0E1C-4E20-9AA6-EDCD0212C87C}";
/// 「此电脑」里的 WPS 云文档
const WPS: &str = "{5FCD4425-CA3A-48F4-A57C-B8A75C32ACB1}";

struct World {
    engine: Engine,
    platform: Arc<MockPlatform>,
    runner: Arc<MockRunner>,
    _dir: tempfile::TempDir,
}

fn user() -> RegRoot {
    RegRoot::User(SID.into())
}

fn class(clsid: &str) -> String {
    format!(r"Software\Classes\CLSID\{clsid}")
}

fn wow(clsid: &str) -> String {
    format!(r"Software\Classes\WOW6432Node\CLSID\{clsid}")
}

fn key(exists: bool, pinned: i64, only_pinned: bool) -> Value {
    json!({ "exists": exists, "pinned": pinned, "only_pinned": only_pinned })
}

/// 脚本的一项；`extra` 覆盖默认值
fn item(place: &str, clsid: &str, hive: &str, extra: Value) -> Value {
    let mut v = json!({ "place": place, "clsid": clsid, "hive": hive, "name": "", "title": "", "localized": "",
                        "user": key(false, -1, false), "machine": key(false, -1, false),
                        "wow_user": key(false, -1, false), "wow_machine": key(false, -1, false),
                        "system_server": false, "folder_target": false });
    v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    v
}

/// 脚本的输出；`gallery_user` 是图库在用户注册表里的那份 CLSID 键（隐藏以后脚本会读到新建的那份）
fn listing(gallery_user: Value) -> Value {
    json!({ "result": "ok", "items": [
        item("nav", CLOUD, "user", json!({ "name": "某某网盘", "title": "某某网盘",
             "user": key(true, 1, false), "wow_user": key(true, 1, true), "system_server": true, "folder_target": true })),
        item("nav", GALLERY, "machine", json!({ "localized": r"@%SystemRoot%\system32\windows.storage.dll,-50310",
             "user": gallery_user, "machine": key(true, 1, false), "system_server": true })),
        item("pc", WPS, "machine", json!({ "name": "WPS云文档", "title": "WPS云文档", "machine": key(true, -1, false) })),
        // 主文件夹：Windows 自己的，不列
        item("nav", "{F874310E-B6B7-47DC-BC84-B9E6B38F5903}", "machine", json!({ "machine": key(true, 1, false) })),
    ]})
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let platform = Arc::new(MockPlatform::new());
    platform.set_indirect(r"@%SystemRoot%\system32\windows.storage.dll,-50310", "图库");
    let dword = RegValue::Dword;
    platform.reg_set(&user(), &class(CLOUD), "", &RegValue::String("某某网盘".into())).unwrap();
    platform.reg_set(&user(), &class(CLOUD), PINNED_VALUE, &dword(1)).unwrap();
    platform.reg_set(&user(), &wow(CLOUD), PINNED_VALUE, &dword(1)).unwrap();
    let m = RegRoot::LocalMachine;
    platform.reg_set(&m, &format!(r"SOFTWARE\Classes\CLSID\{GALLERY}"), PINNED_VALUE, &dword(1)).unwrap();
    platform.reg_set(&m, &format!(r"SOFTWARE\Classes\CLSID\{WPS}"), "", &RegValue::String("WPS云文档".into())).unwrap();
    let runner = Arc::new(MockRunner::new());
    runner.returns(shell_places::LIST_SCRIPT, listing(key(false, -1, false)));
    let journal = Journal::open(dir.path().join("journal.jsonl")).unwrap();
    let engine = Engine::new(
        Catalog::new(CatalogData::default()),
        "test",
        "0.0.0-test",
        platform.clone(),
        runner.clone(),
        journal,
    );
    World { engine, platform, runner, _dir: dir }
}

fn value(w: &World, root: &RegRoot, key: &str, name: &str) -> Option<RegValue> {
    w.platform.reg_get(root, key, name).unwrap()
}

fn find(items: &[ShellPlaceItem], id: &str) -> ShellPlaceItem {
    items.iter().find(|i| i.id == id).cloned().unwrap_or_else(|| panic!("没有 {id}：{items:#?}"))
}

fn nav(clsid: &str) -> String {
    format!("nav:{clsid}")
}

#[test]
fn icons_programs_added_are_listed_with_their_names() {
    let w = world();
    let items = w.engine.shell_places_list().unwrap();
    let titles: Vec<(ShellPlace, &str)> = items.iter().map(|i| (i.place, i.title.as_str())).collect();
    assert_eq!(
        titles,
        [(ShellPlace::Nav, "图库"), (ShellPlace::Nav, "某某网盘"), (ShellPlace::Pc, "WPS云文档")],
        "导航栏的在前，按名字排；主文件夹不列"
    );
    assert!(items.iter().all(|i| i.visible && i.note.is_empty()), "{items:#?}");
    assert!(find(&items, &nav(GALLERY)).windows_own && !find(&items, &nav(CLOUD)).windows_own);
    // 脚本拿到的是登录用户的注册表
    assert_eq!(w.runner.calls()[0].1["UserHive"], format!("Registry::HKEY_USERS\\{SID}"));
}

#[test]
fn hiding_a_cloud_drive_changes_the_users_keys_for_both_views() {
    let w = world();
    w.engine.shell_places_list().unwrap();
    let r = w.engine.shell_places_set(&nav(CLOUD), false).unwrap();
    assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    assert_eq!(r.entry_ids.len(), 2, "64 位的和 32 位程序看的各一条");
    assert_eq!(r.reboot, medkit_core::model::Reboot::Explorer);
    assert_eq!(value(&w, &user(), &class(CLOUD), PINNED_VALUE), Some(RegValue::Dword(0)));
    assert_eq!(value(&w, &user(), &wow(CLOUD), PINNED_VALUE), Some(RegValue::Dword(0)));
    assert!(!find(&w.engine.shell_places_list().unwrap(), &nav(CLOUD)).visible);

    // 用户的那份键里还有别的值（软件自己登记的）：恢复时写回 1，不删
    let r = w.engine.shell_places_set(&nav(CLOUD), true).unwrap();
    assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    assert_eq!(value(&w, &user(), &class(CLOUD), PINNED_VALUE), Some(RegValue::Dword(1)));
    assert_eq!(value(&w, &user(), &class(CLOUD), ""), Some(RegValue::String("某某网盘".into())));
    // 32 位程序看的那一份在列表里是「只有这一个值」，但机器的那份没有这个值：删掉就不显示了，所以也写 1
    assert_eq!(value(&w, &user(), &wow(CLOUD), PINNED_VALUE), Some(RegValue::Dword(1)));

    let again = w.engine.shell_places_set(&nav(CLOUD), true).unwrap();
    assert!(again.entry_ids.is_empty(), "本来就显示，什么都不改");
}

#[test]
fn a_windows_item_is_hidden_for_the_current_user_and_restored_to_what_windows_registered() {
    let w = world();
    w.engine.shell_places_list().unwrap();
    let machine_key = format!(r"SOFTWARE\Classes\CLSID\{GALLERY}");
    let r = w.engine.shell_places_set(&nav(GALLERY), false).unwrap();
    assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    assert_eq!(value(&w, &user(), &class(GALLERY), PINNED_VALUE), Some(RegValue::Dword(0)), "只改当前用户的");
    assert_eq!(value(&w, &RegRoot::LocalMachine, &machine_key, PINNED_VALUE), Some(RegValue::Dword(1)));
    assert!(!w.platform.reg_key_exists(&user(), &wow(GALLERY)).unwrap(), "32 位程序看的那一份本来就没有，不建");

    w.runner.returns(shell_places::LIST_SCRIPT, listing(key(true, 0, true)));
    let items = w.engine.shell_places_list().unwrap();
    assert!(!find(&items, &nav(GALLERY)).visible);
    let r = w.engine.shell_places_set(&nav(GALLERY), true).unwrap();
    assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    assert!(
        !w.platform.reg_key_exists(&user(), &class(GALLERY)).unwrap(),
        "隐藏时新建的键删掉了，回到 Windows 登记的样子"
    );
    assert!(find(&w.engine.shell_places_list().unwrap(), &nav(GALLERY)).visible);
}

#[test]
fn an_item_hidden_in_the_machine_key_is_shown_again_for_everyone() {
    let w = world();
    let machine_key = format!(r"SOFTWARE\Classes\CLSID\{GALLERY}");
    w.platform.reg_set(&RegRoot::LocalMachine, &machine_key, PINNED_VALUE, &RegValue::Dword(0)).unwrap();
    let items = w.engine.shell_places_list().unwrap();
    let gallery = find(&items, &nav(GALLERY));
    assert!(!gallery.visible && gallery.note.contains("所有用户"), "{gallery:?}");
    let r = w.engine.shell_places_set(&nav(GALLERY), true).unwrap();
    assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    assert_eq!(value(&w, &RegRoot::LocalMachine, &machine_key, PINNED_VALUE), Some(RegValue::Dword(1)));
    assert!(!w.platform.reg_key_exists(&user(), &class(GALLERY)).unwrap());
}

#[test]
fn this_pc_items_are_hidden_with_hide_my_computer_icons() {
    let w = world();
    w.engine.shell_places_list().unwrap();
    let id = format!("pc:{WPS}");
    let r = w.engine.shell_places_set(&id, false).unwrap();
    assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    assert_eq!(value(&w, &user(), HIDE_PC_KEY, WPS), Some(RegValue::Dword(1)));
    assert!(!find(&w.engine.shell_places_list().unwrap(), &id).visible);
    let r = w.engine.shell_places_set(&id, true).unwrap();
    assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    assert_eq!(value(&w, &user(), HIDE_PC_KEY, WPS), None);
}

#[test]
fn undoing_in_the_journal_brings_it_back() {
    let w = world();
    w.engine.shell_places_list().unwrap();
    let hide = w.engine.shell_places_set(&nav(GALLERY), false).unwrap();
    let pc = w.engine.shell_places_set(&format!("pc:{WPS}"), false).unwrap();
    for id in hide.entry_ids.iter().chain(&pc.entry_ids).rev() {
        let u = w.engine.journal_undo(id, false).unwrap();
        assert!(u.ok, "{u:?}");
    }
    assert!(!w.platform.reg_key_exists(&user(), &class(GALLERY)).unwrap(), "新建的键跟着删掉");
    assert_eq!(value(&w, &user(), HIDE_PC_KEY, WPS), None);

    let sessions = w.engine.journal_list().unwrap();
    let entries: Vec<_> = sessions.iter().flat_map(|s| &s.entries).collect();
    let rows: Vec<(&str, &str, &str)> =
        entries.iter().map(|e| (e.feature_title.as_str(), e.before.as_str(), e.after.as_str())).collect();
    assert_eq!(
        rows,
        [
            ("资源管理器里的图标：图库", "没有单独设置", "不显示（已隐藏）"),
            ("资源管理器里的图标：WPS云文档", "显示", "不显示（已隐藏）"),
        ],
        "{entries:#?}"
    );
}

#[test]
fn journal_titles_come_from_the_registry_after_a_restart() {
    let w = world();
    w.engine.shell_places_list().unwrap();
    w.engine.shell_places_set(&format!("pc:{WPS}"), false).unwrap();
    // 程序重启以后没有列表：用 CLSID 键里登记的名字
    let journal = Journal::open(w._dir.path().join("journal.jsonl")).unwrap();
    let fresh = Engine::new(
        Catalog::new(CatalogData::default()),
        "test",
        "0.0.0-test",
        w.platform.clone(),
        w.runner.clone(),
        journal,
    );
    let sessions = fresh.journal_list().unwrap();
    let titles: Vec<&str> = sessions.iter().flat_map(|s| &s.entries).map(|e| e.feature_title.as_str()).collect();
    assert_eq!(titles, ["资源管理器里的图标：WPS云文档"]);
}

#[test]
fn only_items_from_the_last_list_can_be_switched() {
    let w = world();
    let e = w.engine.shell_places_set(&nav(CLOUD), false).unwrap_err();
    assert!(e.to_string().contains("不在刚才的列表里"), "{e}");
    w.engine.shell_places_list().unwrap();
    assert!(
        w.engine.shell_places_set("nav:{F874310E-B6B7-47DC-BC84-B9E6B38F5903}", false).is_err(),
        "主文件夹不给隐藏"
    );
}

#[test]
fn a_switch_that_changed_after_the_list_is_not_guessed_at() {
    let w = world();
    w.engine.shell_places_list().unwrap();
    // 软件自己把开关删掉了：用户的那份键还在，资源管理器就不显示它了，也没有能改回来的值
    w.platform.reg_delete_value(&user(), &class(CLOUD), PINNED_VALUE).unwrap();
    let r = w.engine.shell_places_set(&nav(CLOUD), true).unwrap();
    assert!(!r.ok && r.entry_ids.is_empty() && r.message.contains("刷新"), "{r:?}");
}
