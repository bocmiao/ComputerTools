//! 引擎在假系统上的行为：执行、撤销、漂移、回滚、登录用户解析、脚本类功能、报告。

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex};

use medkit_core::Engine;
use medkit_core::catalog::{self, Catalog, CatalogData, Severity};
use medkit_core::journal::{ApplyRecord, Journal, RECORD_VERSION, Record, State, TargetRef, new_id, now_rfc3339};
use medkit_core::model::{Check, Feature, Profile, StartType, Status, Symptom};
use medkit_core::platform::mock::MockPlatform;
use medkit_core::platform::{Platform, UserIdentity};
use medkit_core::registry::{RegRoot, RegValue};
use medkit_core::script::{MockRunner, ScriptError};
use medkit_core::views::FeatureStateKind;
use serde_json::{Value, json};

const SID: &str = "S-1-5-21-1000-2000-3000-1001";
const ADVANCED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";

const CHECKS: &[&str] = &[
    r#"
id: disk.free-space
schema_version: 1
title: { zh-CN: 系统盘剩余空间 }
category: disk
probe: { script: checks/disk/free-space.ps1 }
results:
  ok:
    status: ok
    message: { zh-CN: "还剩 {free_gb} GB。" }
  low:
    status: advice
    message: { zh-CN: "只剩 {free_gb} GB 了。" }
    fixer: medkit
    next: { zh-CN: "先清理下载文件夹。" }
references: [ "https://example.com/disk" ]
"#,
    r#"
id: explorer.hive-probe
schema_version: 1
title: { zh-CN: 读登录用户的设置 }
category: system
user_hive: true
probe: { script: checks/system/hive-probe.ps1 }
results:
  ok: { status: ok, message: { zh-CN: 正常 } }
references: [ "https://example.com/hive" ]
"#,
    r#"
id: disk.hib-state
schema_version: 1
title: { zh-CN: 休眠文件 }
category: disk
probe: { script: checks/disk/hib-state.ps1 }
results:
  off: { status: na, message: { zh-CN: 这台电脑没有休眠文件。 } }
  full:
    status: advice
    message: { zh-CN: 休眠文件是完整版。 }
    fixer: medkit
    links: [ "feature:test.verified" ]
  reduced: { status: ok, message: { zh-CN: 已经是精简版。 } }
references: [ "https://example.com/hib-state" ]
"#,
    r#"
id: system.keys
schema_version: 1
title: { zh-CN: 键盘的辅助功能 }
category: system
probe: { builtin: keyboard-aids }
results:
  ok: { status: ok, message: { zh-CN: 都没开。 } }
  filter-keys: { status: advice, message: { zh-CN: 筛选键开着。 }, fixer: user }
  sticky-keys: { status: advice, message: { zh-CN: 粘滞键开着。 }, fixer: user }
  mouse-keys: { status: advice, message: { zh-CN: 鼠标键开着。 }, fixer: user }
references: [ "https://example.com/keys" ]
"#,
    r#"
id: system.admin-only
schema_version: 1
title: { zh-CN: 需要管理员的检测 }
category: system
requires_admin: true
probe: { script: checks/system/admin-only.ps1 }
results:
  ok: { status: ok, message: { zh-CN: 正常 } }
references: [ "https://example.com/admin" ]
"#,
];

const FEATURES: &[&str] = &[
    r#"
id: explorer.show-extensions
schema_version: 1
title: { zh-CN: 显示文件扩展名 }
description: { zh-CN: 在资源管理器里显示扩展名。 }
category: settings
risk: safe
level: light
recommend: recommended
reboot: explorer
target: current-user
actions:
  - registry: { key: 'HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced', name: HideFileExt, type: dword, value: 0 }
windows_default:
  - registry: { key: 'HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced', name: HideFileExt, type: dword, value: 1 }
undo: auto
references: [ "https://example.com/ext" ]
"#,
    // 和上一个改同一个值：用来验证整批撤销的顺序
    r#"
id: explorer.ext-two
schema_version: 1
title: { zh-CN: 同一个值改成 2 }
description: { zh-CN: 测试用。 }
category: settings
risk: safe
level: light
recommend: optional
target: current-user
actions:
  - registry: { key: 'HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced', name: HideFileExt, type: dword, value: 2 }
windows_default:
  - registry: { key: 'HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced', name: HideFileExt, type: dword, value: 1 }
undo: auto
references: [ "https://example.com/ext" ]
"#,
    r#"
id: test.deep-key
schema_version: 1
title: { zh-CN: 写一个还不存在的键 }
description: { zh-CN: 测试用。 }
category: settings
risk: safe
level: light
recommend: optional
target: current-user
actions:
  - registry: { key: 'HKCU\Software\MedkitTest\Deep\Key', name: Flag, type: string, value: "on" }
windows_default:
  - registry: { key: 'HKCU\Software\MedkitTest\Deep\Key', name: Flag, delete: true }
undo: auto
references: [ "https://example.com/deep" ]
"#,
    r#"
id: test.two-steps
schema_version: 1
title: { zh-CN: 两步修改 }
description: { zh-CN: 第二步会失败。 }
category: settings
risk: safe
level: light
recommend: optional
target: machine
actions:
  - registry: { key: 'HKLM\SOFTWARE\MedkitTest', name: First, type: dword, value: 1 }
  - registry: { key: 'HKLM\SOFTWARE\MedkitTest', name: Second, type: dword, value: 1 }
windows_default:
  - registry: { key: 'HKLM\SOFTWARE\MedkitTest', name: First, delete: true }
  - registry: { key: 'HKLM\SOFTWARE\MedkitTest', name: Second, delete: true }
undo: auto
references: [ "https://example.com/two" ]
"#,
    r#"
id: test.delete-value
schema_version: 1
title: { zh-CN: 删掉一个值 }
description: { zh-CN: 测试用。 }
category: network
risk: caution
level: medium
recommend: optional
target: current-user
actions:
  - registry: { key: 'HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings', name: ProxyServer, delete: true }
break_actions:
  - registry: { key: 'HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings', name: ProxyServer, type: string, value: "127.0.0.1:1" }
undo: auto
references: [ "https://example.com/proxy" ]
"#,
    r#"
id: test.search-manual
schema_version: 1
title: { zh-CN: 搜索服务改成手动 }
description: { zh-CN: 测试用。 }
category: performance
risk: safe
level: medium
recommend: optional
target: machine
actions:
  - service: { name: WSearch, start_type: manual }
windows_default:
  - service: { name: WSearch, start_type: delayed-auto }
undo: auto
references: [ "https://example.com/wsearch" ]
"#,
    r#"
id: test.future-only
schema_version: 1
title: { zh-CN: 只适用于以后的系统 }
description: { zh-CN: 测试用。 }
category: settings
risk: safe
level: light
recommend: optional
target: machine
applies_to: { min_build: 99999 }
actions:
  - registry: { key: 'HKLM\SOFTWARE\MedkitTest', name: Future, type: dword, value: 1 }
windows_default:
  - registry: { key: 'HKLM\SOFTWARE\MedkitTest', name: Future, delete: true }
undo: auto
references: [ "https://example.com/future" ]
"#,
    r#"
id: disk.hibernation-reduce
schema_version: 1
title: { zh-CN: 缩小休眠文件 }
description: { zh-CN: 把休眠文件改成精简模式。 }
category: disk
risk: safe
level: medium
recommend: optional
target: current-user
detect: { script: features/disk/hib-detect.ps1 }
prepare: { script: features/disk/hib-prepare.ps1 }
run: { script: features/disk/hib-reduce.ps1 }
undo: { script: features/disk/hib-restore.ps1 }
break: { script: features/disk/hib-break.ps1 }
references: [ "https://example.com/hib" ]
"#,
    r#"
id: test.verified
schema_version: 1
title: { zh-CN: 看检测结论的修改 }
description: { zh-CN: 测试用。 }
category: disk
risk: safe
level: light
recommend: optional
target: machine
detect: { script: features/test/verified-detect.ps1 }
prepare: { script: features/test/verified-run.ps1 }
run: { script: features/test/verified-run.ps1 }
undo: { script: features/test/verified-undo.ps1 }
verify: disk.hib-state
references: [ "https://example.com/verified" ]
"#,
    r#"
id: test.one-way
schema_version: 1
title: { zh-CN: 不能撤销的操作 }
description: { zh-CN: 测试用。 }
category: disk
risk: safe
level: heavy
recommend: optional
target: machine
detect: { script: features/test/one-way-detect.ps1 }
run: { script: features/test/one-way-run.ps1 }
undo: none
irreversible_reason: { zh-CN: 删掉的文件找不回来。 }
break: { script: features/test/one-way-break.ps1 }
references: [ "https://example.com/oneway" ]
"#,
];

const SYMPTOMS: &[&str] = &[r#"
id: disk-full
schema_version: 1
title: { zh-CN: C 盘满了 }
keywords: [ C盘红了 ]
maturity: one-click
steps:
  - check: disk.free-space
    fixes: [ disk.hibernation-reduce ]
"#];

const PROFILES: &[&str] = &[r#"
id: healthcheck
schema_version: 1
title: { zh-CN: 一键体检 }
checks: [ disk.free-space, explorer.hive-probe ]
"#];

fn parse_all<T: serde::de::DeserializeOwned>(docs: &[&str]) -> Vec<T> {
    docs.iter().map(|d| catalog::parse_typed(d).unwrap_or_else(|e| panic!("{e}\n{d}"))).collect()
}

fn fixture_data() -> CatalogData {
    CatalogData {
        checks: parse_all::<Check>(CHECKS),
        features: parse_all::<Feature>(FEATURES),
        symptoms: parse_all::<Symptom>(SYMPTOMS),
        profiles: parse_all::<Profile>(PROFILES),
        tools: Vec::new(),
        sources: Default::default(),
    }
}

fn fixture_scripts(data: &CatalogData) -> BTreeSet<String> {
    let mut s = BTreeSet::new();
    for c in &data.checks {
        s.extend(c.probe.script.clone());
    }
    for f in &data.features {
        for r in [&f.detect, &f.prepare, &f.run, &f.break_script].into_iter().flatten() {
            s.insert(r.script.clone());
        }
        if let medkit_core::model::Undo::Script(r) = &f.undo {
            s.insert(r.script.clone());
        }
    }
    s
}

struct World {
    engine: Engine,
    platform: Arc<MockPlatform>,
    runner: Arc<MockRunner>,
    journal_path: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

fn world_with(platform: MockPlatform) -> World {
    let dir = tempfile::tempdir().unwrap();
    let journal_path = dir.path().join("journal").join("journal.jsonl");
    let platform = Arc::new(platform);
    let runner = Arc::new(MockRunner::new());
    runner.returns("features/disk/hib-prepare.ps1", json!({ "before": { "type": "full" } }));
    let engine = engine_on(&journal_path, platform.clone(), runner.clone());
    World { engine, platform, runner, journal_path, _dir: dir }
}

fn world() -> World {
    world_with(MockPlatform::new())
}

fn engine_on(journal: &Path, platform: Arc<MockPlatform>, runner: Arc<MockRunner>) -> Engine {
    Engine::new(Catalog::new(fixture_data()), "test", "0.0.0-test", platform, runner, Journal::open(journal).unwrap())
}

fn user() -> RegRoot {
    RegRoot::User(SID.into())
}

fn hide_ext(w: &World) -> Option<RegValue> {
    w.platform.reg_get(&user(), ADVANCED, "HideFileExt").unwrap()
}

#[test]
fn fixture_catalog_passes_validation() {
    let data = fixture_data();
    let problems = catalog::validate(&data, &fixture_scripts(&data));
    let errors: Vec<_> = problems.iter().filter(|p| p.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn apply_writes_to_the_logged_in_user_and_undo_restores_it() {
    let w = world();
    w.platform.seed_value(&user(), ADVANCED, "HideFileExt", RegValue::Dword(1));

    assert_eq!(w.engine.feature_detect("explorer.show-extensions").unwrap().state, FeatureStateKind::NotApplied);
    let r = w.engine.feature_apply("explorer.show-extensions").unwrap();
    assert!(r.ok, "{r:?}");
    assert_eq!(r.verified, FeatureStateKind::Applied);
    assert_eq!(r.entry_ids.len(), 1);
    assert_eq!(hide_ext(&w), Some(RegValue::Dword(0)));
    // 写的是登录用户的 HKU\<SID>，而不是运行程序的账户
    assert_eq!(w.platform.reg_get(&RegRoot::CurrentUser, ADVANCED, "HideFileExt").unwrap(), None);

    let sessions = w.engine.journal_list().unwrap();
    assert_eq!(sessions.len(), 1);
    let e = &sessions[0].entries[0];
    assert!(e.ok && e.can_undo && !e.undone && !e.pending, "{e:?}");
    assert_eq!(e.before, "1（dword）");
    assert_eq!(e.after, "0（dword）");

    let u = w.engine.journal_undo(&r.entry_ids[0], false).unwrap();
    assert!(u.ok && !u.drift, "{u:?}");
    assert_eq!(hide_ext(&w), Some(RegValue::Dword(1)));

    let e = &w.engine.journal_list().unwrap()[0].entries[0];
    assert!(e.undone && !e.can_undo && e.undone_at.is_some());
    assert!(w.engine.journal_undo(&r.entry_ids[0], false).is_err(), "不能恢复两次");
}

#[test]
fn falls_back_to_process_hkcu_without_a_logged_in_user() {
    let mut p = MockPlatform::new();
    p.interactive = None;
    let w = world_with(p);
    let preview = w.engine.feature_preview("explorer.show-extensions").unwrap();
    assert!(preview.notes.iter().any(|n| n.contains("没找到登录用户")), "{:?}", preview.notes);
    assert!(w.engine.feature_apply("explorer.show-extensions").unwrap().ok);
    assert_eq!(w.platform.reg_get(&RegRoot::CurrentUser, ADVANCED, "HideFileExt").unwrap(), Some(RegValue::Dword(0)));
}

#[test]
fn elevated_as_another_admin_still_targets_the_logged_in_user() {
    let mut p = MockPlatform::new();
    p.process = Some(UserIdentity { sid: "S-1-5-21-1000-2000-3000-500".into(), name: "MOCK-PC\\admin".into() });
    let w = world_with(p);
    assert!(w.engine.system_info().elevated_user_mismatch);
    let preview = w.engine.feature_preview("explorer.show-extensions").unwrap();
    assert!(preview.notes.iter().any(|n| n.contains("别的管理员账户")), "{:?}", preview.notes);
    w.engine.feature_apply("explorer.show-extensions").unwrap();
    assert_eq!(hide_ext(&w), Some(RegValue::Dword(0)));
}

#[test]
fn already_applied_feature_is_left_alone() {
    let w = world();
    w.platform.seed_value(&user(), ADVANCED, "HideFileExt", RegValue::Dword(0));
    let r = w.engine.feature_apply("explorer.show-extensions").unwrap();
    assert!(r.ok && r.entry_ids.is_empty(), "{r:?}");
    assert!(w.engine.journal_list().unwrap().is_empty());
}

#[test]
fn created_keys_are_removed_on_undo() {
    let w = world();
    w.platform.seed_value(&user(), r"Software\Other", "Keep", RegValue::Dword(1));
    let r = w.engine.feature_apply("test.deep-key").unwrap();
    assert!(r.ok, "{r:?}");
    assert!(w.platform.key_exists(&user(), r"Software\MedkitTest\Deep\Key"));

    assert!(w.engine.journal_undo(&r.entry_ids[0], false).unwrap().ok);
    assert!(!w.platform.key_exists(&user(), r"Software\MedkitTest"), "新建的键应该一并删掉");
    assert!(w.platform.key_exists(&user(), "Software"), "原来就有的键不能删");
    assert!(w.platform.key_exists(&user(), r"Software\Other"));
}

#[test]
fn created_keys_that_gained_content_are_kept() {
    let w = world();
    let r = w.engine.feature_apply("test.deep-key").unwrap();
    // 别的程序后来在新建的键下面也写了东西
    w.platform.seed_value(&user(), r"Software\MedkitTest\Deep", "Other", RegValue::Dword(7));
    assert!(w.engine.journal_undo(&r.entry_ids[0], false).unwrap().ok);
    assert_eq!(w.platform.reg_get(&user(), r"Software\MedkitTest\Deep\Key", "Flag").unwrap(), None);
    assert!(!w.platform.key_exists(&user(), r"Software\MedkitTest\Deep\Key"));
    assert!(w.platform.key_exists(&user(), r"Software\MedkitTest\Deep"));
}

#[test]
fn undo_reports_drift_and_force_overrides_it() {
    let w = world();
    w.platform.seed_value(&user(), ADVANCED, "HideFileExt", RegValue::Dword(1));
    let r = w.engine.feature_apply("explorer.show-extensions").unwrap();
    w.platform.seed_value(&user(), ADVANCED, "HideFileExt", RegValue::Dword(5));

    let u = w.engine.journal_undo(&r.entry_ids[0], false).unwrap();
    assert!(!u.ok && u.drift, "{u:?}");
    assert_eq!(hide_ext(&w), Some(RegValue::Dword(5)), "漂移时不能自动覆盖");
    assert!(w.engine.journal_list().unwrap()[0].entries[0].can_undo, "漂移后仍可强制恢复");

    let u = w.engine.journal_undo(&r.entry_ids[0], true).unwrap();
    assert!(u.ok, "{u:?}");
    assert_eq!(hide_ext(&w), Some(RegValue::Dword(1)));
}

#[test]
fn failure_midway_rolls_back_earlier_steps() {
    let w = world();
    w.platform.fail_write(r"SOFTWARE\MedkitTest", "Second");
    let r = w.engine.feature_apply("test.two-steps").unwrap();
    assert!(!r.ok, "{r:?}");
    assert!(r.message.contains("退回原样"), "{}", r.message);
    assert!(r.error.is_some());
    assert_eq!(w.platform.reg_get(&RegRoot::LocalMachine, r"SOFTWARE\MedkitTest", "First").unwrap(), None);
    assert!(!w.platform.key_exists(&RegRoot::LocalMachine, r"SOFTWARE\MedkitTest"), "新建的键也要退掉");

    let entries = &w.engine.journal_list().unwrap()[0].entries;
    assert_eq!(entries.len(), 2);
    assert!(entries[0].ok && entries[0].undone && !entries[0].can_undo);
    assert!(entries[0].error.as_deref().unwrap().contains("后面的步骤失败"), "{:?}", entries[0].error);
    assert!(!entries[1].ok && !entries[1].can_undo);
    assert!(entries[1].error.as_deref().unwrap().contains("模拟失败"), "{:?}", entries[1].error);
}

#[test]
fn delete_action_is_undone_by_writing_the_value_back() {
    let w = world();
    let key = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    w.engine.break_feature("test.delete-value").unwrap();
    assert_eq!(w.platform.reg_get(&user(), key, "ProxyServer").unwrap(), Some(RegValue::String("127.0.0.1:1".into())));

    w.runner.returns("host/restore-point.ps1", json!({ "result": "created" }));
    let r = w.engine.feature_apply("test.delete-value").unwrap();
    assert!(r.ok, "{r:?}");
    assert!(r.notes.iter().any(|n| n.contains("已经创建系统还原点")), "{:?}", r.notes);
    let calls = w.runner.calls();
    assert_eq!(calls[0].0, "host/restore-point.ps1");
    assert_eq!(calls[0].1["Description"], "medkit: test.delete-value");
    assert_eq!(w.platform.reg_get(&user(), key, "ProxyServer").unwrap(), None);

    assert!(w.engine.journal_undo(&r.entry_ids[0], false).unwrap().ok);
    assert_eq!(w.platform.reg_get(&user(), key, "ProxyServer").unwrap(), Some(RegValue::String("127.0.0.1:1".into())));
}

#[test]
fn restore_point_failure_does_not_block_the_fix() {
    let w = world();
    w.engine.break_feature("test.delete-value").unwrap();
    w.runner.on("host/restore-point.ps1", |_| Err(ScriptError::Failed("服务被禁用".into())));
    let r = w.engine.feature_apply("test.delete-value").unwrap();
    assert!(r.ok);
    assert!(r.notes.iter().any(|n| n.contains("没能创建还原点")), "{:?}", r.notes);
}

#[test]
fn service_start_type_round_trip() {
    let w = world();
    w.platform.seed_service("WSearch", StartType::DelayedAuto);
    let r = w.engine.feature_apply("test.search-manual").unwrap();
    assert!(r.ok, "{r:?}");
    assert_eq!(w.platform.service_get("WSearch").unwrap(), Some(StartType::Manual));
    assert!(w.engine.journal_undo(&r.entry_ids[0], false).unwrap().ok);
    assert_eq!(w.platform.service_get("WSearch").unwrap(), Some(StartType::DelayedAuto));
}

#[test]
fn missing_service_is_reported_not_crashed() {
    let w = world();
    let r = w.engine.feature_apply("test.search-manual").unwrap();
    assert!(!r.ok);
    assert!(r.error.as_deref().unwrap().contains("WSearch"), "{:?}", r.error);
    assert!(w.engine.journal_list().unwrap().is_empty(), "没改的东西不写日志");
}

#[test]
fn session_undo_goes_newest_first() {
    let w = world();
    w.platform.seed_value(&user(), ADVANCED, "HideFileExt", RegValue::Dword(1));
    assert!(w.engine.feature_apply("explorer.show-extensions").unwrap().ok);
    assert!(w.engine.feature_apply("explorer.ext-two").unwrap().ok);
    assert_eq!(hide_ext(&w), Some(RegValue::Dword(2)));

    let results = w.engine.journal_undo_session(w.engine.session_id()).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.ok && !r.drift), "{results:?}");
    assert_eq!(hide_ext(&w), Some(RegValue::Dword(1)));
}

#[test]
fn journal_survives_restart_and_old_sessions_can_be_undone() {
    let w = world();
    w.platform.seed_value(&user(), ADVANCED, "HideFileExt", RegValue::Dword(1));
    let r = w.engine.feature_apply("explorer.show-extensions").unwrap();
    let old_session = w.engine.session_id().to_owned();

    let restarted = engine_on(&w.journal_path, w.platform.clone(), w.runner.clone());
    assert_ne!(restarted.session_id(), old_session);
    let sessions = restarted.journal_list().unwrap();
    assert_eq!(sessions[0].id, old_session);
    assert!(restarted.journal_undo(&r.entry_ids[0], false).unwrap().ok);
    assert_eq!(hide_ext(&w), Some(RegValue::Dword(1)));
}

#[test]
fn crash_between_apply_and_commit_can_still_be_undone() {
    let w = world();
    let root = user();
    w.platform.seed_value(&root, ADVANCED, "HideFileExt", RegValue::Dword(1));
    // 模拟：写了 apply 记录、改了系统，然后程序崩了，没写 commit
    let rec = ApplyRecord {
        v: RECORD_VERSION,
        id: new_id(),
        session: "crashed-session".into(),
        time: now_rfc3339(),
        feature: "explorer.show-extensions".into(),
        action: 0,
        target: TargetRef::Registry { root: root.clone(), key: ADVANCED.into(), name: "HideFileExt".into() },
        before: State::Registry { value: Some(RegValue::Dword(1)), created_keys: Vec::new() },
    };
    Journal::open(&w.journal_path).unwrap().append(&Record::Apply(rec.clone())).unwrap();
    w.platform.seed_value(&root, ADVANCED, "HideFileExt", RegValue::Dword(0));

    let e = &w.engine.journal_list().unwrap()[0].entries[0];
    assert!(e.pending && !e.ok && e.can_undo, "{e:?}");
    assert!(e.error.as_deref().unwrap().contains("状态不确定"));
    assert!(w.engine.journal_undo(&rec.id, false).unwrap().ok);
    assert_eq!(hide_ext(&w), Some(RegValue::Dword(1)));
}

#[test]
fn damaged_journal_lines_are_skipped() {
    let w = world();
    w.platform.seed_value(&user(), ADVANCED, "HideFileExt", RegValue::Dword(1));
    w.engine.feature_apply("explorer.show-extensions").unwrap();
    // 断电造成的半行
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().append(true).open(&w.journal_path).unwrap();
    f.write_all(b"{\"kind\":\"apply\",\"v\":1,\"id\":\"trunc").unwrap();
    drop(f);
    let sessions = w.engine.journal_list().unwrap();
    assert_eq!(sessions[0].entries.len(), 1);
}

#[test]
fn script_feature_passes_before_to_the_undo_script() {
    let w = world();
    let applied = Arc::new(Mutex::new(false));
    let a = applied.clone();
    w.runner.on("features/disk/hib-detect.ps1", move |_| {
        let state = if *a.lock().unwrap() { "applied" } else { "not-applied" };
        Ok(json!({ "state": state, "facts": { "hiberfil_gb": 6.4 } }))
    });
    let a = applied.clone();
    w.runner.on("features/disk/hib-reduce.ps1", move |_| {
        *a.lock().unwrap() = true;
        Ok(json!({ "before": { "type": "full" }, "after": { "type": "reduced" } }))
    });
    let a = applied.clone();
    w.runner.on("features/disk/hib-restore.ps1", move |args| {
        let before: Value = serde_json::from_str(args["Before"].as_str().unwrap()).unwrap();
        assert_eq!(before, json!({ "type": "full" }));
        *a.lock().unwrap() = false;
        Ok(json!({}))
    });

    let d = w.engine.feature_detect("disk.hibernation-reduce").unwrap();
    assert_eq!(d.state, FeatureStateKind::NotApplied);
    assert_eq!(d.details, vec!["hiberfil_gb：6.4".to_owned()]);

    let r = w.engine.feature_apply("disk.hibernation-reduce").unwrap();
    assert!(r.ok && r.verified == FeatureStateKind::Applied, "{r:?}");
    let e = &w.engine.journal_list().unwrap()[0].entries[0];
    // 脚本自己的数据不给用户看，只说清楚记没记下
    assert!(e.before.contains("已经记下"), "{}", e.before);
    assert_eq!(e.after, "已按这一项修改");

    assert!(w.engine.journal_undo(&r.entry_ids[0], false).unwrap().ok);
    assert!(!*applied.lock().unwrap());
    // current-user 的脚本类功能收到的是登录用户的注册表根
    let hive = format!("Registry::HKEY_USERS\\{SID}");
    assert!(w.runner.calls().iter().all(|(_, args)| args["UserHive"] == hive.as_str()));
}

#[test]
fn reversible_script_without_before_is_not_reported_as_success() {
    let w = world();
    w.runner.returns("features/disk/hib-detect.ps1", json!({ "state": "not-applied" }));
    w.runner.returns("features/disk/hib-prepare.ps1", json!({ "after": { "type": "reduced" } }));

    assert!(w.engine.feature_apply("disk.hibernation-reduce").is_err());
    assert!(w.engine.journal_list().unwrap().is_empty());
    assert!(!w.runner.calls().iter().any(|(s, _)| s.ends_with("hib-reduce.ps1")));
}

#[test]
fn irreversible_feature_cannot_be_undone() {
    let w = world();
    w.runner.returns("features/test/one-way-detect.ps1", json!({ "state": "not-applied" }));
    w.runner.returns("features/test/one-way-run.ps1", json!({ "before": null, "after": null }));
    let preview = w.engine.feature_preview("test.one-way").unwrap();
    assert!(!preview.feature.reversible);
    assert!(preview.notes.iter().any(|n| n.contains("删掉的文件找不回来")), "{:?}", preview.notes);

    let r = w.engine.feature_apply("test.one-way").unwrap();
    assert!(r.ok);
    let e = &w.engine.journal_list().unwrap()[0].entries[0];
    assert!(!e.can_undo);
    assert!(w.engine.journal_undo(&r.entry_ids[0], false).is_err());
    assert!(w.engine.journal_undo_session(w.engine.session_id()).unwrap().is_empty());
}

#[test]
fn features_for_other_builds_are_refused() {
    let w = world();
    let preview = w.engine.feature_preview("test.future-only").unwrap();
    assert!(preview.notes.iter().any(|n| n.starts_with("不能执行")), "{:?}", preview.notes);
    assert!(matches!(w.engine.feature_apply("test.future-only"), Err(medkit_core::Error::NotApplicable(_))));
}

/// 复查用的检测说这台电脑不适用（na，例如没有休眠文件、是笔记本）：预览里说明原因、不给执行，
/// 执行也被拒绝；只跑了检测，功能自己的脚本一个都没跑，也没写修改日志。
#[test]
fn verify_check_saying_na_makes_the_feature_not_applicable() {
    let w = world();
    w.runner.returns("checks/disk/hib-state.ps1", json!({ "result": "off" }));
    let why = "这台电脑没有休眠文件。";

    let preview = w.engine.feature_preview("test.verified").unwrap();
    assert!(!preview.feature.applicable);
    assert_eq!(preview.feature.not_applicable_reason.as_deref(), Some(why));
    assert_eq!(preview.notes, vec![format!("不能执行：{why}")]);
    assert_eq!(preview.changes[0].current, "不适用");

    assert!(matches!(w.engine.feature_apply("test.verified"), Err(medkit_core::Error::NotApplicable(r)) if r == why));
    let d = w.engine.feature_detect("test.verified").unwrap();
    assert_eq!(d.state, FeatureStateKind::Unknown);
    assert!(d.error.as_deref().is_some_and(|e| e.contains(why)), "{d:?}");

    assert!(w.runner.calls().iter().all(|(s, _)| s == "checks/disk/hib-state.ps1"), "{:?}", w.runner.calls());
    assert!(w.engine.journal_list().unwrap().is_empty());
}

/// 检测给 advice 时照常执行，改完用同一个检测复查。
#[test]
fn verify_check_judges_a_script_feature_before_and_after() {
    let w = world();
    let reduced = Arc::new(Mutex::new(false));
    let r = reduced.clone();
    w.runner.on("checks/disk/hib-state.ps1", move |_| {
        Ok(json!({ "result": if *r.lock().unwrap() { "reduced" } else { "full" } }))
    });
    let r = reduced.clone();
    w.runner.on("features/test/verified-run.ps1", move |args| {
        if args.get("Prepare") == Some(&Value::Bool(true)) {
            return Ok(json!({ "before": { "kind": "full" } }));
        }
        *r.lock().unwrap() = true;
        Ok(json!({ "after": { "kind": "reduced" } }))
    });

    let preview = w.engine.feature_preview("test.verified").unwrap();
    assert!(preview.feature.applicable && preview.notes.is_empty(), "{preview:?}");
    assert_eq!(preview.changes[0].current, "还没改");
    let res = w.engine.feature_apply("test.verified").unwrap();
    assert!(res.ok && res.verified == FeatureStateKind::Applied, "{res:?}");
    assert_eq!(w.engine.feature_detect("test.verified").unwrap().state, FeatureStateKind::Applied);
}

#[test]
fn check_results_are_rendered_from_facts() {
    let w = world();
    w.runner.returns("checks/disk/free-space.ps1", json!({ "result": "low", "facts": { "free_gb": 12.34 } }));
    let r = w.engine.run_check("disk.free-space").unwrap();
    assert_eq!(r.status, Status::Advice);
    assert_eq!(r.message, "只剩 12.3 GB 了。");
    assert_eq!(r.next.as_deref(), Some("先清理下载文件夹。"));
    assert_eq!(r.result_code.as_deref(), Some("low"));
    assert!(r.error.is_none());
}

#[test]
fn check_failures_become_unknown_with_a_reason() {
    let w = world();
    w.runner.returns("checks/disk/free-space.ps1", json!({ "result": "weird" }));
    let r = w.engine.run_check("disk.free-space").unwrap();
    assert_eq!(r.status, Status::Unknown);
    assert!(r.error.as_deref().unwrap().contains("weird"));

    w.runner.on("checks/disk/free-space.ps1", |_| Err(ScriptError::Timeout(15)));
    let r = w.engine.run_check("disk.free-space").unwrap();
    assert_eq!(r.status, Status::Unknown);
    assert!(r.error.is_some());
}

#[test]
fn admin_only_checks_are_skipped_without_admin() {
    let mut p = MockPlatform::new();
    p.admin = false;
    let w = world_with(p);
    let r = w.engine.run_check("system.admin-only").unwrap();
    assert_eq!(r.status, Status::Unknown);
    assert!(r.error.as_deref().unwrap().contains("管理员"));
    assert!(w.runner.calls().is_empty(), "不应该去跑脚本");
}

#[test]
fn user_hive_argument_points_at_the_logged_in_user() {
    let w = world();
    w.runner.returns("checks/system/hive-probe.ps1", json!({ "result": "ok" }));
    w.engine.run_check("explorer.hive-probe").unwrap();
    let calls = w.runner.calls();
    assert_eq!(calls[0].1["UserHive"], format!("Registry::HKEY_USERS\\{SID}").as_str());

    let mut p = MockPlatform::new();
    p.interactive = None;
    let w = world_with(p);
    w.runner.returns("checks/system/hive-probe.ps1", json!({ "result": "ok" }));
    w.engine.run_check("explorer.hive-probe").unwrap();
    assert_eq!(w.runner.calls()[0].1["UserHive"], "HKCU:");
}

#[test]
fn symptom_detail_resolves_titles() {
    let w = world();
    let d = w.engine.symptom_detail("disk-full").unwrap();
    assert_eq!(d.steps[0].check_title, "系统盘剩余空间");
    assert_eq!(d.steps[0].fixes[0].id, "disk.hibernation-reduce");
    assert!(w.engine.symptom_detail("nope").is_err());
}

#[test]
fn report_is_redacted() {
    let w = world();
    w.runner.returns("checks/disk/free-space.ps1", json!({ "result": "ok", "facts": { "free_gb": 80 } }));
    w.runner.returns("checks/system/hive-probe.ps1", json!({ "result": "ok" }));
    w.platform.seed_value(&user(), ADVANCED, "HideFileExt", RegValue::Dword(1));
    let results = w.engine.run_profile("healthcheck").unwrap();
    assert_eq!(results.len(), 2);
    w.engine.feature_apply("explorer.show-extensions").unwrap();

    let report = w.engine.report_generate(None).unwrap();
    assert!(report.contains("还剩 80 GB"), "{report}");
    assert!(report.contains("显示文件扩展名"), "{report}");
    for leaked in ["xiaoming", "MOCK-PC", "1000-2000-3000"] {
        assert!(!report.contains(leaked), "报告里出现了 {leaked}：\n{report}");
    }
}

// ───────────── 审查发现的问题的回归测试 ─────────────

/// 回滚那一步也失败时，不能告诉用户「已经退回原样」。
#[test]
fn rollback_failure_is_reported_honestly() {
    let w = world();
    let key = r"SOFTWARE\MedkitTest";
    w.platform.seed_value(&RegRoot::LocalMachine, key, "First", RegValue::Dword(0));
    // First 第一次写（执行）成功，第二次写（回滚）失败；Second 一写就失败
    w.platform.fail_write_after(key, "First", 1);
    w.platform.fail_write(key, "Second");

    let r = w.engine.feature_apply("test.two-steps").unwrap();
    assert!(!r.ok);
    assert!(r.message.contains("没能自动退回"), "回滚失败了却说退回了：{}", r.message);
    assert_eq!(w.platform.reg_get(&RegRoot::LocalMachine, key, "First").unwrap(), Some(RegValue::Dword(1)));
    // 这一步还留在系统上，用户要能从修改日志里恢复
    let first = &w.engine.journal_list().unwrap()[0].entries[0];
    assert!(first.can_undo && !first.undone, "{first:?}");
}

/// 断电截断在一个中文字符中间，那一行不是合法 UTF-8：只跳过它，不能整份日志都读不出来。
#[test]
fn invalid_utf8_in_the_journal_only_loses_that_line() {
    let w = world();
    w.platform.seed_value(&user(), ADVANCED, "HideFileExt", RegValue::Dword(1));
    let r = w.engine.feature_apply("explorer.show-extensions").unwrap();
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().append(true).open(&w.journal_path).unwrap();
    // 「错」的 UTF-8 是 E9 94 99，只写了前两个字节
    f.write_all(b"{\"kind\":\"commit\",\"error\":\"\xE9\x94").unwrap();
    drop(f);

    assert_eq!(w.engine.journal_list().unwrap()[0].entries.len(), 1);
    assert!(w.engine.journal_undo(&r.entry_ids[0], false).unwrap().ok);
}

/// 上次写到一半的半行没有换行结尾，下一条记录不能和它粘在一起被一起丢掉。
#[test]
fn a_torn_line_does_not_swallow_the_next_record() {
    let w = world();
    w.platform.seed_value(&user(), ADVANCED, "HideFileExt", RegValue::Dword(1));
    w.engine.feature_apply("explorer.show-extensions").unwrap();
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().append(true).open(&w.journal_path).unwrap();
    f.write_all(b"{\"kind\":\"apply\",\"v\":1,\"id\":\"torn").unwrap();
    drop(f);

    // 断电以后的下一次修改
    let r = w.engine.feature_apply("test.deep-key").unwrap();
    assert!(r.ok, "{r:?}");
    let entries: Vec<_> = w.engine.journal_list().unwrap().into_iter().flat_map(|s| s.entries).collect();
    assert_eq!(entries.len(), 2, "新记录被半行吞掉了：{entries:?}");
    assert!(w.engine.journal_undo(&r.entry_ids[0], false).unwrap().ok);
}

/// 脚本类功能执行成功、但结果写不进修改日志：必须马上用撤销脚本退回。
#[test]
fn script_change_that_cannot_be_recorded_is_undone_immediately() {
    let w = world();
    let applied = Arc::new(Mutex::new(false));
    let journal = w.journal_path.clone();
    let a = applied.clone();
    w.runner.on("features/disk/hib-reduce.ps1", move |_| {
        *a.lock().unwrap() = true;
        // 模拟磁盘出问题：之后的写日志都会失败
        std::fs::remove_file(&journal).unwrap();
        std::fs::create_dir(&journal).unwrap();
        Ok(json!({ "before": { "type": "full" }, "after": { "type": "reduced" } }))
    });
    let a = applied.clone();
    w.runner.on("features/disk/hib-restore.ps1", move |args| {
        let before: Value = serde_json::from_str(args["Before"].as_str().unwrap()).unwrap();
        assert_eq!(before, json!({ "type": "full" }));
        *a.lock().unwrap() = false;
        Ok(json!({}))
    });
    w.runner.returns("features/disk/hib-detect.ps1", json!({ "state": "not-applied" }));

    assert!(w.engine.feature_apply("disk.hibernation-reduce").is_err());
    assert!(!*applied.lock().unwrap(), "改动记不下来，却没有退回");
}

/// 脚本类功能撤销时，要改回当初执行时那个用户的设置，而不是现在登录的用户。
#[test]
fn script_undo_targets_the_user_it_was_applied_to() {
    let w = world();
    w.runner.returns("features/disk/hib-detect.ps1", json!({ "state": "not-applied" }));
    w.runner.returns("features/disk/hib-reduce.ps1", json!({ "before": { "type": "full" }, "after": {} }));
    let r = w.engine.feature_apply("disk.hibernation-reduce").unwrap();
    assert!(r.ok, "{r:?}");

    // 第二天换了一个人登录，用管理员账户打开小药箱撤销
    let mut p = MockPlatform::new();
    p.interactive = Some(UserIdentity { sid: "S-1-5-21-1000-2000-3000-1002".into(), name: "MOCK-PC\\other".into() });
    let runner = Arc::new(MockRunner::new());
    runner.returns("features/disk/hib-detect.ps1", json!({ "state": "applied" }));
    runner.returns("features/disk/hib-restore.ps1", json!({}));
    let restarted = engine_on(&w.journal_path, Arc::new(p), runner.clone());
    assert!(restarted.journal_undo(&r.entry_ids[0], false).unwrap().ok);

    // 撤销前的核对和撤销本身，都用执行时那个用户的注册表
    let hive = format!("Registry::HKEY_USERS\\{SID}");
    for script in ["hib-detect.ps1", "hib-restore.ps1"] {
        let (_, args) = runner.calls().into_iter().find(|(s, _)| s.ends_with(script)).unwrap();
        assert_eq!(args["UserHive"], hive.as_str(), "{script}");
    }
}

/// 脚本类修复后来被改掉了（比如用户又开了一个新代理），撤销前要先问，不能直接覆盖。
#[test]
fn script_undo_asks_first_when_the_fix_is_no_longer_in_place() {
    let w = world();
    let state = Arc::new(Mutex::new("not-applied"));
    let st = state.clone();
    w.runner.on("features/disk/hib-detect.ps1", move |_| Ok(json!({ "state": *st.lock().unwrap() })));
    let st = state.clone();
    w.runner.on("features/disk/hib-reduce.ps1", move |_| {
        *st.lock().unwrap() = "applied";
        Ok(json!({ "before": { "type": "full" }, "after": {} }))
    });
    w.runner.returns("features/disk/hib-restore.ps1", json!({}));
    let r = w.engine.feature_apply("disk.hibernation-reduce").unwrap();

    // 别的程序把它改回去了
    *state.lock().unwrap() = "not-applied";
    let u = w.engine.journal_undo(&r.entry_ids[0], false).unwrap();
    assert!(u.drift && !u.ok, "{u:?}");
    assert!(!w.runner.calls().iter().any(|(s, _)| s.ends_with("hib-restore.ps1")), "没问就撤销了");

    // 用户确认以后才撤销
    assert!(w.engine.journal_undo(&r.entry_ids[0], true).unwrap().ok);
}

/// 那个用户已经注销时，撤销要明确拒绝，不能「成功」了却什么都没改。
#[test]
fn undo_is_refused_while_that_users_settings_are_not_loaded() {
    let w = world();
    w.platform.seed_value(&user(), ADVANCED, "HideFileExt", RegValue::Dword(1));
    let r = w.engine.feature_apply("explorer.show-extensions").unwrap();
    w.platform.unload_hive(SID);

    let u = w.engine.journal_undo(&r.entry_ids[0], true).unwrap();
    assert!(!u.ok && !u.drift, "{u:?}");
    assert!(u.message.contains("没有登录"), "{}", u.message);
    let e = &w.engine.journal_list().unwrap()[0].entries[0];
    assert!(!e.undone && e.can_undo, "没改成却被标成已恢复：{e:?}");
}

/// 清理新建的空键失败只是收尾没做完，值已经恢复了，整条撤销不能算失败。
#[test]
fn failing_to_remove_created_keys_does_not_fail_the_undo() {
    let w = world();
    let r = w.engine.feature_apply("test.deep-key").unwrap();
    w.platform.fail_key_delete(r"Software\MedkitTest\Deep\Key");

    let u = w.engine.journal_undo(&r.entry_ids[0], false).unwrap();
    assert!(u.ok, "{u:?}");
    assert_eq!(w.platform.reg_get(&user(), r"Software\MedkitTest\Deep\Key", "Flag").unwrap(), None);
    assert!(w.engine.journal_list().unwrap()[0].entries[0].undone);
}

/// 执行中途程序退出、脚本类修改没记下原状态的，不给「恢复原状」按钮，而不是点了才报错。
#[test]
fn script_entries_without_a_recorded_state_cannot_be_undone() {
    let w = world();
    let rec = ApplyRecord {
        v: RECORD_VERSION,
        id: new_id(),
        session: "crashed".into(),
        time: now_rfc3339(),
        feature: "disk.hibernation-reduce".into(),
        action: 0,
        target: TargetRef::Script { feature: "disk.hibernation-reduce".into(), hive: None },
        before: State::Script { data: Value::Null },
    };
    Journal::open(&w.journal_path).unwrap().append(&Record::Apply(rec.clone())).unwrap();
    let e = &w.engine.journal_list().unwrap()[0].entries[0];
    assert!(e.pending && !e.can_undo, "{e:?}");
    // 位置一栏说人话，不显示功能 ID（功能名界面上单独显示）
    assert_eq!(e.target, "小药箱脚本改的设置");
    assert!(w.engine.journal_undo(&rec.id, false).is_err());
    assert!(w.runner.calls().is_empty(), "不应该去跑撤销脚本");
}

#[test]
fn interrupted_script_can_restore_its_prepared_state() {
    let w = world();
    let rec = ApplyRecord {
        v: RECORD_VERSION,
        id: new_id(),
        session: "interrupted".into(),
        time: now_rfc3339(),
        feature: "disk.hibernation-reduce".into(),
        action: 0,
        target: TargetRef::Script { feature: "disk.hibernation-reduce".into(), hive: None },
        before: State::Script { data: json!({ "type": "full" }) },
    };
    Journal::open(&w.journal_path).unwrap().append(&Record::Apply(rec.clone())).unwrap();
    let entry = &w.engine.journal_list().unwrap()[0].entries[0];
    assert!(entry.pending && entry.can_undo && entry.before.contains("已经记下"), "{entry:?}");
    w.runner.on("features/disk/hib-restore.ps1", |args| {
        let before: Value = serde_json::from_str(args["Before"].as_str().unwrap()).unwrap();
        assert_eq!(before, json!({ "type": "full" }));
        Ok(json!({}))
    });
    assert!(w.engine.journal_undo(&rec.id, false).unwrap().ok);
}

#[test]
fn script_skips_when_state_changes_after_snapshot() {
    let w = world();
    w.runner.returns("features/disk/hib-detect.ps1", json!({ "state": "not-applied" }));
    w.runner.on("features/disk/hib-reduce.ps1", |args| {
        let before: Value = serde_json::from_str(args["Before"].as_str().unwrap()).unwrap();
        assert_eq!(before, json!({ "type": "full" }));
        Ok(json!({ "skipped": true, "reason": "状态已变化" }))
    });
    let result = w.engine.feature_apply("disk.hibernation-reduce").unwrap();
    assert!(!result.ok && result.message.contains("没有修改"), "{result:?}");
    let entry = &w.engine.journal_list().unwrap()[0].entries[0];
    assert!(!entry.pending && !entry.can_undo, "{entry:?}");
    assert!(!w.runner.calls().iter().any(|(script, _)| script.ends_with("hib-restore.ps1")));
}

/// 目录里直接标出这台电脑不能用的功能，界面据此禁用，不用等点了执行才报错。
#[test]
fn catalog_marks_features_that_do_not_apply_here() {
    let w = world();
    let summary = w.engine.catalog_summary();
    let find = |id: &str| summary.features.iter().find(|f| f.id == id).unwrap().clone();
    let future = find("test.future-only");
    assert!(!future.applicable);
    assert!(future.not_applicable_reason.as_deref().unwrap().contains("99999"), "{future:?}");
    let ext = find("explorer.show-extensions");
    assert!(ext.applicable && ext.not_applicable_reason.is_none());
}

/// 没改成、已经退回的，不能再叫用户去重启。
#[test]
fn a_failed_apply_does_not_ask_for_a_restart() {
    let w = world();
    w.platform.seed_value(&user(), ADVANCED, "HideFileExt", RegValue::Dword(1));
    w.platform.fail_write(ADVANCED, "HideFileExt");
    let r = w.engine.feature_apply("explorer.show-extensions").unwrap();
    assert!(!r.ok);
    assert_eq!(r.reboot, medkit_core::model::Reboot::None);
}

/// 没改成功、自动退回以后也没退干净的那一步（commit 里 left_changes 为 true）：
/// 修改日志里要能手动恢复，而不是说「当时就没有改成功，不需要恢复」。
#[test]
fn a_failed_step_that_left_changes_can_still_be_undone() {
    use medkit_core::journal::CommitRecord;
    let w = world();
    let key = r"SOFTWARE\MedkitTest";
    // 系统上现在是改了一半的样子：原来没有这个值
    w.platform.seed_value(&RegRoot::LocalMachine, key, "Half", RegValue::Dword(1));
    let rec = ApplyRecord {
        v: RECORD_VERSION,
        id: new_id(),
        session: "s1".into(),
        time: now_rfc3339(),
        feature: "test.two-steps".into(),
        action: 1,
        target: TargetRef::Registry { root: RegRoot::LocalMachine, key: key.into(), name: "Half".into() },
        before: State::Registry { value: None, created_keys: Vec::new() },
    };
    let journal = Journal::open(&w.journal_path).unwrap();
    journal.append(&Record::Apply(rec.clone())).unwrap();
    for left_changes in [false, true] {
        journal
            .append(&Record::Commit(CommitRecord {
                v: RECORD_VERSION,
                reference: rec.id.clone(),
                time: now_rfc3339(),
                ok: false,
                after: None,
                error: Some("写入后读回的值不对".into()),
                left_changes,
            }))
            .unwrap();
    }
    let e = &w.engine.journal_list().unwrap()[0].entries[0];
    assert!(e.can_undo && !e.ok, "{e:?}");
    assert!(w.engine.journal_undo(&rec.id, false).unwrap().ok);
    assert_eq!(w.platform.reg_get(&RegRoot::LocalMachine, key, "Half").unwrap(), None);
}

/// 恢复原状以后，和当初修改一样要重启资源管理器才看得到效果，结果里要带上。
#[test]
fn undo_tells_what_is_needed_to_see_the_change() {
    use medkit_core::model::Reboot;
    let w = world();
    w.platform.seed_value(&user(), ADVANCED, "HideFileExt", RegValue::Dword(1));
    let r = w.engine.feature_apply("explorer.show-extensions").unwrap();
    let u = w.engine.journal_undo(&r.entry_ids[0], false).unwrap();
    assert!(u.ok);
    assert_eq!(u.reboot, Reboot::Explorer);
    assert_eq!(serde_json::to_value(&u).unwrap()["reboot"], "explorer");
}

/// 内置检测读的是平台报告的实际状态（Windows 上是 SystemParametersInfo），不经过脚本。
#[test]
fn keyboard_aids_are_read_from_the_platform() {
    let w = world();
    assert_eq!(w.engine.run_check("system.keys").unwrap().status, Status::Ok);

    let mut platform = MockPlatform::new();
    platform.keyboard.sticky_keys = true;
    platform.keyboard.mouse_keys = true;
    let w = world_with(platform);
    let r = w.engine.run_check("system.keys").unwrap();
    assert_eq!((r.status, r.message.as_str()), (Status::Advice, "粘滞键开着。"));
    assert_eq!(Value::Object(r.facts), json!({ "filter_keys": false, "sticky_keys": true, "mouse_keys": true }));
    assert!(w.runner.calls().is_empty(), "不该跑脚本：{:?}", w.runner.calls());
}

/// 用户自己写的问题描述放在报告最前面，和其余部分一样脱敏；太长的截断，空的不写这一节。
#[test]
fn the_users_own_note_is_redacted_and_kept_short() {
    let w = world();
    let note = "  我是xiaoming，MOCK-PC 昨天开始上不了网，路由器是192.168.1.1\u{7}，电话13812345678  ";
    let report = w.engine.report_generate(Some(note)).unwrap();
    let section = report.split("== 我遇到的问题 ==\n").nth(1).expect("有这一节");
    let first = section.lines().next().unwrap();
    assert_eq!(first, "我是<已隐藏>，<已隐藏> 昨天开始上不了网，路由器是<IP>，电话<手机号>");
    assert!(report.find("== 我遇到的问题 ==").unwrap() < report.find("== 最近一次体检 ==").unwrap());

    let long = "网".repeat(medkit_core::engine::NOTE_MAX_CHARS + 50);
    let report = w.engine.report_generate(Some(&long)).unwrap();
    assert!(report.contains(&format!("{}……（后面的省略了）", "网".repeat(medkit_core::engine::NOTE_MAX_CHARS))));
    assert!(!report.contains(&"网".repeat(medkit_core::engine::NOTE_MAX_CHARS + 1)));

    for empty in [None, Some(""), Some("  \n ")] {
        assert!(!w.engine.report_generate(empty).unwrap().contains("我遇到的问题"));
    }
}

/// 直接去「按症状修」查过、没做过体检的人，报告里也要有那几项的结论（断网求助时正是这样）。
#[test]
fn checks_run_outside_the_health_check_appear_in_the_report() {
    let w = world();
    w.runner.returns("checks/system/admin-only.ps1", json!({ "result": "ok", "facts": {} }));
    w.engine.run_check("system.admin-only").unwrap();
    let report = w.engine.report_generate(None).unwrap();
    assert!(report.contains("单独检查过的项目"), "{report}");
    assert!(report.contains("需要管理员的检测"), "{report}");
    // 时间按「年-月-日 时:分」显示（换成本机时间），不是 RFC 3339 原文
    let generated = report.lines().find(|l| l.starts_with("生成时间：")).unwrap();
    assert!(!generated.contains('T') && generated.contains(':'), "{generated}");
}
