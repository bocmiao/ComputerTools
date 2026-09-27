//! 右键菜单管理在假系统上的行为：只列第三方的，拿掉、恢复写的值，修改日志，撤销，只认列表里的项目。

use std::sync::Arc;

use medkit_core::Engine;
use medkit_core::catalog::{Catalog, CatalogData};
use medkit_core::context_menu;
use medkit_core::journal::Journal;
use medkit_core::model::Reboot;
use medkit_core::platform::Platform;
use medkit_core::platform::mock::MockPlatform;
use medkit_core::registry::{RegRoot, RegValue};
use medkit_core::script::MockRunner;
use medkit_core::views::{ContextMenuItem, ContextMenuKind};
use serde_json::json;

const SID: &str = "S-1-5-21-1000-2000-3000-1001";
const BLOCKED: &str = r"Software\Microsoft\Windows\CurrentVersion\Shell Extensions\Blocked";
const WINRAR: &str = "{B41DB860-64E4-11D2-9906-E49FADC173CA}";
const GIT_SHELL: &str = r"SOFTWARE\Classes\Directory\shell\git_shell";
const CODE_USER: &str = r"Software\Classes\Directory\Background\shell\VSCode";

struct World {
    engine: Engine,
    platform: Arc<MockPlatform>,
    runner: Arc<MockRunner>,
    _dir: tempfile::TempDir,
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let platform = Arc::new(MockPlatform::new());
    platform.set_indirect("@C:\\Program Files\\Git\\git-bash.exe,-101", "Open Git &Bash here");
    let runner = Arc::new(MockRunner::new());
    runner.returns(
        context_menu::LIST_SCRIPT,
        json!({ "result": "ok", "items": [
            // Windows 自己的：不列
            { "kind": "verb", "hive": "machine", "scope": "Directory", "key": "cmd", "text": "@shell32.dll,-8506",
              "path": "C:\\Windows\\System32\\cmd.exe", "exists": true, "system": true },
            { "kind": "handler", "hive": "machine", "scope": "*", "key": "Open With", "clsid": "{09799AFB-AD67-11D1-ABCD-00C04FC30936}",
              "path": "C:\\Windows\\System32\\shell32.dll", "exists": true, "system": true },
            // 第三方的命令：所有用户的和当前用户的
            { "kind": "verb", "hive": "machine", "scope": "Directory", "key": "git_shell", "text": "@C:\\Program Files\\Git\\git-bash.exe,-101",
              "path": "C:\\Program Files\\Git\\git-bash.exe", "exists": true, "signature": "valid", "signer": "Johannes Schindelin" },
            { "kind": "verb", "hive": "user", "scope": "Directory\\Background", "key": "VSCode", "text": "通过 Code 打开",
              "path": "C:\\Users\\x\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe", "exists": true, "extended": true },
            // 同一个外壳扩展登记在三处，另外还有 Windows 11 新菜单里的项目：合成一项
            { "kind": "handler", "hive": "machine", "scope": "*", "key": "WinRAR", "clsid": WINRAR, "text": "WinRAR shell extension",
              "path": "C:\\Program Files\\WinRAR\\rarext.dll", "exists": true, "description": "WinRAR shell extension", "company": "Alexander Roshal" },
            { "kind": "handler", "hive": "machine", "scope": "Directory", "key": "WinRAR", "clsid": WINRAR,
              "path": "C:\\Program Files\\WinRAR\\rarext.dll", "exists": true },
            { "kind": "handler", "hive": "machine", "scope": "Drive", "key": "WinRAR", "clsid": WINRAR,
              "path": "C:\\Program Files\\WinRAR\\rarext.dll", "exists": true },
            // 卸载后留下的空壳
            { "kind": "handler", "hive": "user", "scope": "*", "key": "OldCloud", "clsid": "{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}",
              "class_found": false }
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

fn user() -> RegRoot {
    RegRoot::User(SID.into())
}

fn value(w: &World, root: &RegRoot, key: &str, name: &str) -> Option<RegValue> {
    w.platform.reg_get(root, key, name).unwrap()
}

fn find<'a>(items: &'a [ContextMenuItem], title: &str) -> &'a ContextMenuItem {
    items.iter().find(|i| i.title == title).unwrap_or_else(|| panic!("没有「{title}」：{items:#?}"))
}

#[test]
fn only_third_party_entries_are_listed_with_readable_names() {
    let w = world();
    let items = w.engine.context_menu_list().unwrap();
    assert_eq!(items.len(), 4, "{items:#?}");

    let git = find(&items, "Open Git Bash here");
    assert_eq!(git.kind, ContextMenuKind::Command);
    assert_eq!((git.program.as_str(), git.location.as_str()), ("git-bash.exe", "所有用户"));
    assert_eq!(git.publisher.as_deref(), Some("Johannes Schindelin"));
    assert_eq!(git.scopes, ["文件夹"]);
    assert!(git.visible);

    let code = find(&items, "通过 Code 打开");
    assert!(code.shift_only);
    assert_eq!(code.location, "当前用户");
    assert_eq!(code.scopes, ["文件夹空白处"]);

    let rar = find(&items, "WinRAR shell extension");
    assert_eq!(rar.kind, ContextMenuKind::Extension);
    assert_eq!(rar.scopes, ["文件", "文件夹", "磁盘"]);
    assert!(rar.note.contains("几处"), "{}", rar.note);

    let old = find(&items, "OldCloud");
    assert!(old.note.contains("卸载"), "{}", old.note);

    // 脚本拿到的是登录用户的注册表
    assert_eq!(w.runner.calls()[0].1["UserHive"], format!("Registry::HKEY_USERS\\{SID}"));
}

#[test]
fn hiding_a_command_writes_programmatic_access_only_where_it_lives() {
    let w = world();
    let items = w.engine.context_menu_list().unwrap();
    let r = w.engine.context_menu_set(&find(&items, "Open Git Bash here").id, false).unwrap();
    assert!(r.ok, "{r:?}");
    assert_eq!(r.reboot, Reboot::None, "命令下次右键就生效");
    assert!(r.message.contains("拿掉"), "{}", r.message);
    assert_eq!(
        value(&w, &RegRoot::LocalMachine, GIT_SHELL, "ProgrammaticAccessOnly"),
        Some(RegValue::String(String::new()))
    );
    // 当前用户的命令写到登录用户的注册表
    let r = w.engine.context_menu_set(&find(&items, "通过 Code 打开").id, false).unwrap();
    assert!(r.ok, "{r:?}");
    assert_eq!(value(&w, &user(), CODE_USER, "ProgrammaticAccessOnly"), Some(RegValue::String(String::new())));
    let again = w.engine.context_menu_list().unwrap();
    assert!(!find(&again, "Open Git Bash here").visible);

    // 修改日志说人话，能撤销
    let sessions = w.engine.journal_list().unwrap();
    let e = sessions[0].entries.iter().find(|e| e.feature_title == "右键菜单：Open Git Bash here").unwrap();
    assert_eq!((e.before.as_str(), e.after.as_str()), ("显示", "不显示（已拿掉）"));
    let u = w.engine.journal_undo(&e.id, false).unwrap();
    assert!(u.ok, "{u:?}");
    assert_eq!(value(&w, &RegRoot::LocalMachine, GIT_SHELL, "ProgrammaticAccessOnly"), None);
}

#[test]
fn hiding_an_extension_blocks_its_clsid_once_for_all_its_places() {
    let w = world();
    let items = w.engine.context_menu_list().unwrap();
    let rar = find(&items, "WinRAR shell extension");
    let r = w.engine.context_menu_set(&rar.id, false).unwrap();
    assert!(r.ok, "{r:?}");
    assert_eq!(r.entry_ids.len(), 1);
    assert_eq!(r.reboot, Reboot::Explorer, "外壳扩展要重启资源管理器才生效");
    assert_eq!(value(&w, &RegRoot::LocalMachine, BLOCKED, WINRAR), Some(RegValue::String(String::new())));
    assert!(!find(&w.engine.context_menu_list().unwrap(), "WinRAR shell extension").visible);

    // 已经拿掉了再拿：不改
    let r = w.engine.context_menu_set(&rar.id, false).unwrap();
    assert!(r.entry_ids.is_empty());

    // 修改日志：重启以后没有列表了，用它登记的类名
    w.platform.seed_value(
        &RegRoot::LocalMachine,
        &format!(r"SOFTWARE\Classes\CLSID\{WINRAR}"),
        "",
        RegValue::String("WinRAR".into()),
    );
    let sessions = w.engine.journal_list().unwrap();
    assert_eq!(sessions[0].entries[0].feature_title, "右键菜单：WinRAR shell extension");

    // 恢复：别的工具写在当前用户下的也一起删
    w.platform.seed_value(&user(), BLOCKED, WINRAR, RegValue::String(String::new()));
    let r = w.engine.context_menu_set(&rar.id, true).unwrap();
    assert!(r.ok, "{r:?}");
    assert_eq!(r.entry_ids.len(), 2);
    assert_eq!(value(&w, &RegRoot::LocalMachine, BLOCKED, WINRAR), None);
    assert_eq!(value(&w, &user(), BLOCKED, WINRAR), None);
    assert!(find(&w.engine.context_menu_list().unwrap(), "WinRAR shell extension").visible);
}

#[test]
fn a_command_hidden_by_another_tool_is_shown_again_and_unknown_ids_are_refused() {
    let w = world();
    w.platform.seed_value(&RegRoot::LocalMachine, GIT_SHELL, "LegacyDisable", RegValue::String(String::new()));
    let items = w.engine.context_menu_list().unwrap();
    let git = find(&items, "Open Git Bash here");
    assert!(!git.visible, "别的工具用 LegacyDisable 藏起来的也算不显示");
    let r = w.engine.context_menu_set(&git.id, true).unwrap();
    assert!(r.ok, "{r:?}");
    assert_eq!(value(&w, &RegRoot::LocalMachine, GIT_SHELL, "LegacyDisable"), None);

    let e = w.engine.context_menu_set("not-in-the-list", false).unwrap_err();
    assert!(e.to_string().contains("刷新"), "{e}");
}

#[test]
fn a_failed_write_is_reported_and_nothing_is_left_behind() {
    let w = world();
    let items = w.engine.context_menu_list().unwrap();
    w.platform.fail_write(GIT_SHELL, "ProgrammaticAccessOnly");
    let r = w.engine.context_menu_set(&find(&items, "Open Git Bash here").id, false).unwrap();
    assert!(!r.ok);
    assert!(r.message.contains("退回原样"), "{}", r.message);
    assert_eq!(value(&w, &RegRoot::LocalMachine, GIT_SHELL, "ProgrammaticAccessOnly"), None);
}
