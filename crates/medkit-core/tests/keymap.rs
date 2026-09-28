//! 改键（键位重映射）在假系统上的行为：写进 HKLM 的 `Scancode Map`，读回来说成人话，全部恢复是删掉这个值，
//! 修改日志和撤销，别的改键软件设的值只能整个清掉。

use std::sync::Arc;

use medkit_core::Engine;
use medkit_core::catalog::{Catalog, CatalogData};
use medkit_core::journal::Journal;
use medkit_core::keymap::{self, MappingInput};
use medkit_core::model::Reboot;
use medkit_core::platform::Platform;
use medkit_core::platform::mock::MockPlatform;
use medkit_core::registry::{RegRoot, RegValue};
use medkit_core::script::MockRunner;
use medkit_core::views::FeatureStateKind;

struct World {
    engine: Engine,
    platform: Arc<MockPlatform>,
    _dir: tempfile::TempDir,
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let platform = Arc::new(MockPlatform::new());
    let journal = Journal::open(dir.path().join("journal.jsonl")).unwrap();
    let engine = Engine::new(
        Catalog::new(CatalogData::default()),
        "test",
        "0.0.0-test",
        platform.clone(),
        Arc::new(MockRunner::new()),
        journal,
    );
    World { engine, platform, _dir: dir }
}

fn subkey() -> &'static str {
    keymap::KEY.strip_prefix(r"HKLM\").unwrap()
}

fn scancode_map(w: &World) -> Option<RegValue> {
    w.platform.reg_get(&RegRoot::LocalMachine, subkey(), keymap::VALUE).unwrap()
}

fn seed(w: &World, value: RegValue) {
    w.platform.seed_value(&RegRoot::LocalMachine, subkey(), keymap::VALUE, value);
}

fn map(from: &str, to: Option<&str>) -> MappingInput {
    MappingInput { from: from.into(), to: to.map(Into::into) }
}

fn bytes(hex: &str) -> Vec<u8> {
    hex::decode(hex.replace(' ', "")).unwrap()
}

#[test]
fn nothing_is_remapped_by_default() {
    let w = world();
    let view = w.engine.key_remap_get().unwrap();
    assert!(view.mappings.is_empty() && !view.foreign && view.foreign_text.is_none(), "{view:?}");
    let caps = view.keys.iter().find(|k| k.id == "CapsLock").unwrap();
    assert_eq!(caps.label, "Caps Lock（大写锁定）");
    assert!(!caps.target_only);
    assert!(view.keys.iter().find(|k| k.id == "AudioVolumeMute").unwrap().target_only);
}

#[test]
fn remapping_writes_the_scancode_map_and_undo_takes_it_away() {
    let w = world();
    let r = w.engine.key_remap_set(&[map("CapsLock", Some("ControlLeft")), map("MetaLeft", None)]).unwrap();
    assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    assert_eq!(r.entry_ids.len(), 1);
    assert_eq!(r.reboot, Reboot::Reboot, "重启电脑以后才生效");
    assert!(r.message.contains("Caps Lock（大写锁定） → 左 Ctrl；左 Win → 不起作用"), "{}", r.message);
    // 3 条（两条映射加结束标记）；Caps Lock（3A）变成左 Ctrl（1D），左 Win（E05B）变成 0
    assert_eq!(
        scancode_map(&w),
        Some(RegValue::Binary(bytes("00000000 00000000 03000000 1D003A00 00005BE0 00000000")))
    );

    let view = w.engine.key_remap_get().unwrap();
    assert!(!view.foreign, "{view:?}");
    let rows: Vec<(&str, Option<&str>, &str)> =
        view.mappings.iter().map(|m| (m.from.as_str(), m.to.as_deref(), m.text.as_str())).collect();
    assert_eq!(
        rows,
        [("CapsLock", Some("ControlLeft"), "Caps Lock（大写锁定） → 左 Ctrl"), ("MetaLeft", None, "左 Win → 不起作用"),]
    );

    let sessions = w.engine.journal_list().unwrap();
    let e = &sessions[0].entries[0];
    assert_eq!(e.feature_title, "键位重映射（改键）");
    assert_eq!(e.before, "没有改键（Windows 默认）");
    assert_eq!(e.after, "Caps Lock（大写锁定） → 左 Ctrl；左 Win → 不起作用");
    assert!(e.ok && e.can_undo, "{e:?}");

    let u = w.engine.journal_undo(&r.entry_ids[0], false).unwrap();
    assert!(u.ok && !u.drift, "{u:?}");
    assert_eq!(u.reboot, Reboot::Reboot, "撤销也是重启以后生效");
    assert_eq!(scancode_map(&w), None, "原来没有这个值，撤销就是删掉它");
    assert!(w.engine.key_remap_get().unwrap().mappings.is_empty());
}

#[test]
fn changing_replaces_the_whole_list_and_clearing_deletes_the_value() {
    let w = world();
    w.engine.key_remap_set(&[map("Insert", None)]).unwrap();
    let r = w.engine.key_remap_set(&[map("Insert", None), map("ContextMenu", Some("AudioVolumeMute"))]).unwrap();
    assert!(r.ok && r.entry_ids.len() == 1, "{r:?}");
    assert_eq!(
        scancode_map(&w),
        Some(RegValue::Binary(bytes("00000000 00000000 03000000 000052E0 20E05DE0 00000000")))
    );

    let r = w.engine.key_remap_set(&[]).unwrap();
    assert!(r.ok && r.entry_ids.len() == 1 && r.reboot == Reboot::Reboot, "{r:?}");
    assert!(r.message.contains("全部去掉"), "{}", r.message);
    assert_eq!(scancode_map(&w), None);
    let e = &w.engine.journal_list().unwrap()[0].entries[2];
    assert_eq!(e.before, "Insert（插入） → 不起作用；菜单键（右 Ctrl 左边） → 静音");
    assert_eq!(e.after, "没有改键（Windows 默认）");

    // 撤销「全部恢复」：原来的值写回去
    let u = w.engine.journal_undo(&r.entry_ids[0], false).unwrap();
    assert!(u.ok, "{u:?}");
    assert_eq!(w.engine.key_remap_get().unwrap().mappings.len(), 2);
}

#[test]
fn setting_what_is_already_there_writes_nothing() {
    let w = world();
    let r = w.engine.key_remap_set(&[]).unwrap();
    assert!(r.ok && r.entry_ids.is_empty() && r.reboot == Reboot::None, "{r:?}");
    assert_eq!(r.message, "本来就是这样，不用改。");

    w.engine.key_remap_set(&[map("CapsLock", None)]).unwrap();
    let r = w.engine.key_remap_set(&[map("CapsLock", None)]).unwrap();
    assert!(r.ok && r.entry_ids.is_empty() && r.reboot == Reboot::None, "{r:?}");
    assert_eq!(w.engine.journal_list().unwrap()[0].entries.len(), 1);
}

#[test]
fn bad_requests_are_refused_before_anything_is_written() {
    let w = world();
    for bad in [
        vec![map("KeyA", Some("KeyA"))],
        vec![map("KeyA", None), map("KeyA", Some("KeyB"))],
        vec![map("AudioVolumeUp", Some("KeyA"))],
        vec![map("NoSuchKey", None)],
    ] {
        assert!(w.engine.key_remap_set(&bad).is_err(), "{bad:?}");
    }
    assert_eq!(scancode_map(&w), None);
    assert!(w.engine.journal_list().unwrap().is_empty());
}

#[test]
fn values_set_by_other_tools_can_only_be_cleared() {
    // 日文键盘的「かな」键（70）变成左 Ctrl：小药箱的名单里没有 70
    let foreign = bytes("00000000 00000000 03000000 1D007000 00003A00 00000000");
    let w = world();
    seed(&w, RegValue::Binary(foreign.clone()));
    let view = w.engine.key_remap_get().unwrap();
    assert!(view.foreign, "{view:?}");
    assert_eq!(view.foreign_text.as_deref(), Some("扫描码 0x0070 → 左 Ctrl"));
    assert_eq!(view.mappings.len(), 1, "认得的那条照样列出来：{view:?}");

    let err = w.engine.key_remap_set(&[map("CapsLock", None)]).unwrap_err().to_string();
    assert!(err.contains("认不出来"), "{err}");
    assert_eq!(scancode_map(&w), Some(RegValue::Binary(foreign.clone())), "没有动");

    let r = w.engine.key_remap_set(&[]).unwrap();
    assert!(r.ok, "{r:?}");
    assert_eq!(scancode_map(&w), None);
    // 撤销：一个字节不差地写回去
    w.engine.journal_undo(&r.entry_ids[0], false).unwrap();
    assert_eq!(scancode_map(&w), Some(RegValue::Binary(foreign)));
}

#[test]
fn broken_values_are_shown_as_unrecognised() {
    for value in [RegValue::Binary(vec![1, 2, 3]), RegValue::Dword(1)] {
        let w = world();
        seed(&w, value.clone());
        let view = w.engine.key_remap_get().unwrap();
        assert!(view.foreign && view.mappings.is_empty(), "{view:?}");
        assert!(w.engine.key_remap_set(&[map("CapsLock", None)]).is_err());
        assert!(w.engine.key_remap_set(&[]).unwrap().ok);
        assert_eq!(scancode_map(&w), None);
    }
}
