//! 「新建」菜单管理在假系统上的行为：列哪些、名字怎么来，关掉和恢复时怎么改名，修改日志，撤销，缓存，只认列表里的项目。

use std::sync::Arc;

use medkit_core::Engine;
use medkit_core::catalog::{Catalog, CatalogData};
use medkit_core::journal::Journal;
use medkit_core::new_menu;
use medkit_core::platform::Platform;
use medkit_core::platform::mock::MockPlatform;
use medkit_core::registry::{RegRoot, RegValue};
use medkit_core::script::MockRunner;
use medkit_core::views::{FeatureStateKind, NewMenuItem};
use serde_json::json;

const SID: &str = "S-1-5-21-1000-2000-3000-1001";
const WPS: &str = r"SOFTWARE\Classes\.docx\KWPS.Document.12\ShellNew";
const WORD: &str = r"SOFTWARE\Classes\.docx\Word.Document.12\ShellNew";
const TXT: &str = r"SOFTWARE\Classes\.txt\ShellNew";
const XMIND: &str = r"Software\Classes\.xmind\ShellNew";

struct World {
    engine: Engine,
    platform: Arc<MockPlatform>,
    runner: Arc<MockRunner>,
    _dir: tempfile::TempDir,
}

fn user() -> RegRoot {
    RegRoot::User(SID.into())
}

fn empty() -> RegValue {
    RegValue::String(String::new())
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let platform = Arc::new(MockPlatform::new());
    platform.set_indirect("@C:\\Program Files\\WPS\\wps.exe,-100", "WPS 文字 文档");
    platform.set_indirect(r"@%SystemRoot%\system32\notepad.exe,-470", "文本文档");
    let m = RegRoot::LocalMachine;
    platform.reg_set(&m, WPS, "NullFile", &empty()).unwrap();
    // Word 的那个现在不用（.docx 被 WPS 接管了），但也一起关，免得换回 Word 以后又冒出来
    platform.reg_set(&m, WORD, "FileName", &RegValue::String("Word.docx".into())).unwrap();
    platform.reg_set(&m, TXT, "NullFile", &empty()).unwrap();
    platform
        .reg_set(&m, TXT, "ItemName", &RegValue::ExpandString(r"@%SystemRoot%\system32\notepad.exe,-470".into()))
        .unwrap();
    platform.reg_set(&user(), XMIND, "Data", &RegValue::Binary(vec![0x50, 0x4b, 0x05, 0x06])).unwrap();
    let runner = Arc::new(MockRunner::new());
    let item =
        |hive: &str, ext: &str, progid: &str, current: &str, values: serde_json::Value, extra: serde_json::Value| {
            let mut v = json!({ "hive": hive, "ext": ext, "progid": progid, "current_progid": current, "values": values,
                            "item_name": "", "menu_text": "", "type_name": "" });
            v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            v
        };
    runner.returns(
        new_menu::LIST_SCRIPT,
        json!({ "result": "ok", "items": [
            item("machine", ".docx", "KWPS.Document.12", "KWPS.Document.12", json!(["NullFile"]),
                 json!({ "type_name": "@C:\\Program Files\\WPS\\wps.exe,-100" })),
            item("machine", ".docx", "Word.Document.12", "KWPS.Document.12", json!(["FileName"]), json!({})),
            item("machine", ".txt", "", "txtfile", json!(["ItemName", "NullFile"]),
                 json!({ "item_name": "@%SystemRoot%\\system32\\notepad.exe,-470" })),
            item("user", ".xmind", "", "", json!("Data"), json!({ "menu_text": "XMind 思维导图(&X)" })),
            // 快捷方式：不列
            item("machine", ".lnk", "", "lnkfile", json!(["NullFile", "Handler"]), json!({})),
        ]}),
    );
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

fn find<'a>(items: &'a [NewMenuItem], ext: &str) -> &'a NewMenuItem {
    items.iter().find(|i| i.ext == ext).unwrap_or_else(|| panic!("没有 {ext}：{items:#?}"))
}

#[test]
fn entries_are_listed_with_their_menu_text() {
    let w = world();
    let items = w.engine.new_menu_list().unwrap();
    let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(titles, ["WPS 文字 文档", "XMind 思维导图", "文本文档"], "按名字排，快捷方式不列");
    let txt = find(&items, ".txt");
    assert!(txt.windows_own && txt.visible);
    assert_eq!(txt.location, "所有用户");
    assert_eq!(find(&items, ".xmind").location, "当前用户");
    assert!(!find(&items, ".docx").windows_own);
    // 脚本拿到的是登录用户的注册表
    assert_eq!(w.runner.calls()[0].1["UserHive"], format!("Registry::HKEY_USERS\\{SID}"));
}

#[test]
fn hiding_renames_the_values_everywhere_and_restoring_renames_them_back() {
    let w = world();
    let cache = new_menu::CACHE_KEY;
    w.platform.reg_set(&user(), cache, "Classes", &RegValue::MultiString(vec![".docx".into()])).unwrap();
    w.engine.new_menu_list().unwrap();

    let r = w.engine.new_menu_set(".docx", false).unwrap();
    assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    assert_eq!(r.entry_ids.len(), 4, "两个值，各写一个新名字、删一个旧名字");
    let m = RegRoot::LocalMachine;
    assert_eq!(value(&w, &m, WPS, "NullFile"), None);
    assert_eq!(value(&w, &m, WPS, "MedkitHidden.NullFile"), Some(empty()));
    assert_eq!(value(&w, &m, WORD, "FileName"), None);
    assert_eq!(value(&w, &m, WORD, "MedkitHidden.FileName"), Some(RegValue::String("Word.docx".into())));
    assert_eq!(value(&w, &user(), cache, "Classes"), None, "资源管理器的缓存删掉了，它会重建");
    assert!(!find(&w.engine.new_menu_list().unwrap(), ".docx").visible);

    let r = w.engine.new_menu_set(".docx", true).unwrap();
    assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    assert_eq!(value(&w, &m, WPS, "NullFile"), Some(empty()));
    assert_eq!(value(&w, &m, WPS, "MedkitHidden.NullFile"), None);
    assert_eq!(value(&w, &m, WORD, "FileName"), Some(RegValue::String("Word.docx".into())));
    assert!(find(&w.engine.new_menu_list().unwrap(), ".docx").visible);

    let again = w.engine.new_menu_set(".docx", true).unwrap();
    assert!(again.entry_ids.is_empty(), "本来就显示，什么都不改");
}

#[test]
fn binary_data_in_the_user_hive_is_kept_exactly() {
    let w = world();
    w.engine.new_menu_list().unwrap();
    let data = RegValue::Binary(vec![0x50, 0x4b, 0x05, 0x06]);
    w.engine.new_menu_set(".xmind", false).unwrap();
    assert_eq!(value(&w, &user(), XMIND, "Data"), None);
    assert_eq!(value(&w, &user(), XMIND, "MedkitHidden.Data"), Some(data.clone()));
    w.engine.new_menu_set(".xmind", true).unwrap();
    assert_eq!(value(&w, &user(), XMIND, "Data"), Some(data));
}

#[test]
fn undoing_in_the_journal_brings_it_back() {
    let w = world();
    w.engine.new_menu_list().unwrap();
    let r = w.engine.new_menu_set(".txt", false).unwrap();
    let m = RegRoot::LocalMachine;
    assert_eq!(value(&w, &m, TXT, "NullFile"), None);
    assert!(value(&w, &m, TXT, "ItemName").is_some(), "只改让它出现的值，别的不动");
    for id in r.entry_ids.iter().rev() {
        assert!(w.engine.journal_undo(id, false).unwrap().ok);
    }
    assert_eq!(value(&w, &m, TXT, "NullFile"), Some(empty()));
    assert_eq!(value(&w, &m, TXT, "MedkitHidden.NullFile"), None);

    let sessions = w.engine.journal_list().unwrap();
    let entries: Vec<_> = sessions.iter().flat_map(|s| &s.entries).collect();
    assert!(entries.iter().all(|e| e.feature_title == "「新建」菜单：文本文档"), "{entries:#?}");
    let states: Vec<(&str, &str)> = entries.iter().map(|e| (e.before.as_str(), e.after.as_str())).collect();
    assert!(states.iter().all(|s| *s == ("显示", "不显示（已关掉）")), "{states:?}");
}

#[test]
fn restoring_keeps_what_the_program_wrote_again() {
    let w = world();
    w.engine.new_menu_list().unwrap();
    w.engine.new_menu_set(".txt", false).unwrap();
    // 软件（或者 Windows 更新）又把它写回来了，但写得不一样
    let m = RegRoot::LocalMachine;
    w.platform.reg_set(&m, TXT, "NullFile", &RegValue::String("new".into())).unwrap();
    w.engine.new_menu_list().unwrap();
    w.engine.new_menu_set(".txt", false).unwrap();
    w.engine.new_menu_set(".txt", true).unwrap();
    assert_eq!(value(&w, &m, TXT, "NullFile"), Some(RegValue::String("new".into())));
    assert_eq!(value(&w, &m, TXT, "MedkitHidden.NullFile"), None);
}

#[test]
fn only_entries_from_the_last_list_can_be_switched() {
    let w = world();
    let e = w.engine.new_menu_set(".txt", false).unwrap_err();
    assert!(e.to_string().contains("不在刚才的列表里"), "{e}");
    w.engine.new_menu_list().unwrap();
    assert!(w.engine.new_menu_set(".lnk", false).is_err(), "快捷方式不在列表里，关不了");
}
