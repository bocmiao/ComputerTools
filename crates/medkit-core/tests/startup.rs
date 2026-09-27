//! 开机启动项在假系统上的行为：列表、禁用、恢复、撤销，只认列表里的启动项。

use std::sync::Arc;

use medkit_core::Engine;
use medkit_core::catalog::{Catalog, CatalogData};
use medkit_core::journal::Journal;
use medkit_core::platform::Platform;
use medkit_core::platform::mock::MockPlatform;
use medkit_core::registry::{RegRoot, RegValue};
use medkit_core::script::MockRunner;
use medkit_core::startup::{self, Source};
use medkit_core::views::{StartupAdvice, StartupSignature};
use serde_json::json;

const SID: &str = "S-1-5-21-1000-2000-3000-1001";
const USER_RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const MACHINE_RUN32: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32";
/// 任务管理器禁用时写的值：03，再加禁用的时间
const DISABLED_BY_TASK_MANAGER: [u8; 12] = [3, 0, 0, 0, 0x10, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0x01];

struct World {
    engine: Engine,
    platform: Arc<MockPlatform>,
    runner: Arc<MockRunner>,
    _dir: tempfile::TempDir,
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let platform = Arc::new(MockPlatform::new());
    let runner = Arc::new(MockRunner::new());
    runner.returns(
        startup::LIST_SCRIPT,
        json!({ "result": "ok", "items": [
            { "source": "user-run", "name": "WeChat", "path": "C:\\Program Files\\Tencent\\WeChat\\WeChat.exe",
              "exists": true, "description": "微信", "company": "Tencent", "signature": "valid",
              "signer": "Tencent Technology(Shenzhen) Company Limited" },
            { "source": "machine-run32", "name": "RtkAudUService", "path": "C:\\Windows\\RtkAudUService64.exe",
              "exists": true, "signature": "valid", "signer": "Realtek Semiconductor Corp.", "system": true },
            { "source": "user-folder", "name": "Old tool.lnk", "path": "C:\\Old\\tool.exe", "exists": false }
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

#[test]
fn the_list_shows_names_advice_and_the_switch() {
    let w = world();
    w.platform.seed_value(
        &RegRoot::LocalMachine,
        MACHINE_RUN32,
        "RtkAudUService",
        RegValue::Binary(DISABLED_BY_TASK_MANAGER.to_vec()),
    );
    let items = w.engine.startup_list().unwrap();
    assert_eq!(items.len(), 3);

    let wechat = &items[0];
    assert_eq!((wechat.title.as_str(), wechat.program.as_str()), ("微信", "WeChat.exe"));
    assert_eq!(wechat.publisher.as_deref(), Some("Tencent Technology(Shenzhen) Company Limited"));
    assert_eq!(wechat.signature, StartupSignature::Valid);
    assert!(wechat.enabled);
    assert_eq!(wechat.advice, StartupAdvice::CanDisable);
    assert_eq!(wechat.location, "当前用户（注册表）");

    let realtek = &items[1];
    assert!(!realtek.enabled, "任务管理器里禁用过的要显示成禁用");
    assert_eq!(realtek.advice, StartupAdvice::Keep);
    assert_eq!(realtek.title, "RtkAudUService", "没有说明的用名字");

    let gone = &items[2];
    assert_eq!(gone.advice, StartupAdvice::CanDisable);
    assert!(gone.reason.contains("卸载"), "{}", gone.reason);

    // 脚本拿到的是登录用户的注册表
    let calls = w.runner.calls();
    assert_eq!(calls[0].1["UserHive"], format!("Registry::HKEY_USERS\\{SID}"));
}

#[test]
fn disabling_writes_what_task_manager_writes_and_undo_removes_it() {
    let w = world();
    w.engine.startup_list().unwrap();
    let r = w.engine.startup_set(&startup::item_id(Source::UserRun, "WeChat"), false).unwrap();
    assert!(r.ok, "{r:?}");
    assert_eq!(r.entry_ids.len(), 1);
    assert!(r.message.contains("不会再自动启动"), "{}", r.message);
    assert_eq!(r.feature, startup::FEATURE_ID);
    match value(&w, &user(), USER_RUN, "WeChat") {
        Some(RegValue::Binary(b)) => {
            assert_eq!(b.len(), 12);
            assert_eq!(&b[..4], &[3, 0, 0, 0]);
            assert_ne!(&b[4..], &[0; 8], "要写上禁用的时间");
        }
        other => panic!("开关没写对：{other:?}"),
    }
    assert!(!w.engine.startup_list().unwrap()[0].enabled);

    // 修改日志说人话
    let sessions = w.engine.journal_list().unwrap();
    let e = &sessions[0].entries[0];
    assert_eq!(e.feature_title, "开机启动项：WeChat");
    assert_eq!((e.before.as_str(), e.after.as_str()), ("开机自动启动（默认）", "不自动启动（已停用）"));
    assert!(e.can_undo);

    let u = w.engine.journal_undo(&r.entry_ids[0], false).unwrap();
    assert!(u.ok, "{u:?}");
    assert_eq!(value(&w, &user(), USER_RUN, "WeChat"), None, "原来没有这个值，撤销后也不该有");
    assert!(w.engine.startup_list().unwrap()[0].enabled);
}

#[test]
fn turning_back_on_an_item_disabled_in_task_manager_can_be_undone_exactly() {
    let w = world();
    let root = RegRoot::LocalMachine;
    w.platform.seed_value(&root, MACHINE_RUN32, "RtkAudUService", RegValue::Binary(DISABLED_BY_TASK_MANAGER.to_vec()));
    w.engine.startup_list().unwrap();
    let r = w.engine.startup_set(&startup::item_id(Source::MachineRun32, "RtkAudUService"), true).unwrap();
    assert!(r.ok, "{r:?}");
    assert_eq!(
        value(&w, &root, MACHINE_RUN32, "RtkAudUService"),
        Some(RegValue::Binary(vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]))
    );
    let u = w.engine.journal_undo(&r.entry_ids[0], false).unwrap();
    assert!(u.ok, "{u:?}");
    assert_eq!(
        value(&w, &root, MACHINE_RUN32, "RtkAudUService"),
        Some(RegValue::Binary(DISABLED_BY_TASK_MANAGER.to_vec())),
        "撤销要一字不差地写回原来的值（包括任务管理器记的时间）"
    );
}

#[test]
fn only_listed_items_can_be_switched_and_no_op_changes_are_not_logged() {
    let w = world();
    let e = w.engine.startup_set(&startup::item_id(Source::UserRun, "WeChat"), false).unwrap_err();
    assert!(e.to_string().contains("不在刚才的列表里"), "{e}");

    w.engine.startup_list().unwrap();
    let e = w.engine.startup_set(&startup::item_id(Source::MachineRun, "WeChat"), false).unwrap_err();
    assert!(e.to_string().contains("不在刚才的列表里"), "来源也要对得上：{e}");
    let e = w.engine.startup_set(&startup::item_id(Source::UserRun, "Evil"), false).unwrap_err();
    assert!(e.to_string().contains("不在刚才的列表里"), "{e}");

    let r = w.engine.startup_set(&startup::item_id(Source::UserRun, "WeChat"), true).unwrap();
    assert!(r.ok && r.entry_ids.is_empty(), "{r:?}");
    assert!(r.message.contains("本来就是这样"));
    assert!(w.engine.journal_list().unwrap().is_empty(), "没改的不写修改日志");
}

#[test]
fn a_failing_list_script_is_reported() {
    let w = world();
    w.runner.returns(startup::LIST_SCRIPT, json!({ "result": "boom" }));
    let e = w.engine.startup_list().unwrap_err();
    assert!(e.to_string().contains("没有定义的结果"), "{e}");
}

#[test]
fn a_write_that_fails_is_rolled_back_and_reported() {
    let w = world();
    w.engine.startup_list().unwrap();
    w.platform.fail_write(USER_RUN, "WeChat");
    let r = w.engine.startup_set(&startup::item_id(Source::UserRun, "WeChat"), false).unwrap();
    assert!(!r.ok, "{r:?}");
    assert!(r.error.is_some());
    assert_eq!(value(&w, &user(), USER_RUN, "WeChat"), None);
}

#[test]
fn undo_does_not_overwrite_a_change_made_in_task_manager_afterwards() {
    let w = world();
    let items = w.engine.startup_list().unwrap();
    let r = w.engine.startup_set(&items[0].id, false).unwrap();
    assert!(r.ok, "{r:?}");
    // 用户后来又在任务管理器里把它打开了
    w.platform.seed_value(&user(), USER_RUN, "WeChat", RegValue::Binary(vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
    let u = w.engine.journal_undo(&r.entry_ids[0], false).unwrap();
    assert!(u.drift && !u.ok, "{u:?}");
    assert_eq!(
        value(&w, &user(), USER_RUN, "WeChat"),
        Some(RegValue::Binary(vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])),
        "被别人改过的，不经用户同意不能覆盖"
    );
}

#[test]
fn ids_come_from_the_list_and_differ_by_source() {
    let w = world();
    let items = w.engine.startup_list().unwrap();
    assert_eq!(items[0].id, startup::item_id(Source::UserRun, "WeChat"));
    assert_ne!(startup::item_id(Source::UserRun, "WeChat"), startup::item_id(Source::MachineRun, "WeChat"));
}
