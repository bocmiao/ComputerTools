//! 小工具（docs/architecture.md 第 11 节）：数据校验、运行 info / action、打开系统工具。
//! 小工具不改设置，所以也要确认它们不会写修改日志。

use std::collections::BTreeSet;
use std::sync::Arc;

use medkit_core::Engine;
use medkit_core::catalog::{self, Catalog, CatalogData, Severity};
use medkit_core::journal::Journal;
use medkit_core::model::{Check, Status, Tool, ToolGroup};
use medkit_core::platform::OpenRequest;
use medkit_core::platform::mock::MockPlatform;
use medkit_core::script::{MockRunner, ScriptError};
use medkit_core::views::ToolOpens;
use serde_json::{Value, json};

const SID: &str = "S-1-5-21-1000-2000-3000-1001";

const CHECK: &str = r#"
id: disk.free-space
schema_version: 1
title: { zh-CN: 系统盘剩余空间 }
category: disk
probe: { script: checks/disk/free-space.ps1 }
results:
  low:
    status: advice
    message: { zh-CN: 快满了 }
    links: [ "tool:test.cleanup", "tool:test.hardware" ]
references: [ "https://example.com" ]
"#;

const TOOLS: &[&str] = &[
    r#"
id: test.hardware
schema_version: 1
group: info
title: { zh-CN: 电脑配置 }
description: { zh-CN: 看看这台电脑的配置。 }
category: hardware
requires_admin: true
user_hive: true
timeout_sec: 60
run: { script: tools/test/hardware.ps1 }
results:
  ok: { status: ok, message: { zh-CN: "这台电脑有 {disk_count} 块硬盘。" } }
labels:
  sections: { disk: { zh-CN: 硬盘 }, wifi: { zh-CN: WiFi } }
  rows: { size: { zh-CN: 容量 }, type: { zh-CN: 类型 }, password: { zh-CN: 密码 } }
  values: { ssd: { zh-CN: 固态硬盘 } }
references: [ "https://example.com" ]
"#,
    r#"
id: test.flush
schema_version: 1
group: action
title: { zh-CN: 刷新 DNS 缓存 }
description: { zh-CN: 清掉记住的网址解析结果。 }
category: network
run: { script: tools/test/flush.ps1 }
confirm: { zh-CN: 确定要刷新吗？ }
results:
  done:
    status: ok
    message: { zh-CN: "清掉了 {entries} 条记录。" }
    next: { zh-CN: 还是打不开的话，看看「上不了网」。 }
    links: [ "tool:test.cleanup" ]
references: [ "https://example.com" ]
"#,
    r#"
id: test.cleanup
schema_version: 1
group: open
title: { zh-CN: 磁盘清理 }
description: { zh-CN: 系统自带的清理工具。 }
category: disk
open: { program: disk-cleanup }
references: [ "https://example.com" ]
"#,
    r#"
id: test.device-manager
schema_version: 1
group: open
title: { zh-CN: 设备管理器 }
description: { zh-CN: 看看哪个硬件没装好驱动。 }
category: hardware
audience: helper
open: { program: device-manager }
references: [ "https://example.com" ]
"#,
    r#"
id: test.windows-update
schema_version: 1
group: open
title: { zh-CN: Windows 更新 }
description: { zh-CN: 检查和安装系统更新。 }
category: settings
open: { settings: windowsupdate }
references: [ "https://example.com" ]
"#,
];

fn tools() -> Vec<Tool> {
    TOOLS.iter().map(|d| catalog::parse_typed(d).unwrap_or_else(|e| panic!("{e}\n{d}"))).collect()
}

fn data() -> CatalogData {
    CatalogData { checks: vec![catalog::parse_typed::<Check>(CHECK).unwrap()], tools: tools(), ..Default::default() }
}

fn scripts() -> BTreeSet<String> {
    ["checks/disk/free-space.ps1", "tools/test/hardware.ps1", "tools/test/flush.ps1"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn errors(data: &CatalogData) -> Vec<String> {
    catalog::validate(data, &scripts())
        .into_iter()
        .filter(|p| p.severity == Severity::Error)
        .map(|p| p.message)
        .collect()
}

/// 把一个小工具的 YAML 改一处再校验，返回错误信息。
fn errors_after(id: &str, edit: impl FnOnce(&mut Tool)) -> Vec<String> {
    let mut d = data();
    let t = d.tools.iter_mut().find(|t| t.id == id).unwrap();
    edit(t);
    errors(&d)
}

struct World {
    engine: Engine,
    platform: Arc<MockPlatform>,
    runner: Arc<MockRunner>,
    _dir: tempfile::TempDir,
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let journal = Journal::open(dir.path().join("journal.jsonl")).unwrap();
    let platform = Arc::new(MockPlatform::new());
    let runner = Arc::new(MockRunner::new());
    let engine = Engine::new(Catalog::new(data()), "test", "0.0.0-test", platform.clone(), runner.clone(), journal);
    World { engine, platform, runner, _dir: dir }
}

// ───────────── 数据校验 ─────────────

#[test]
fn fixture_tools_are_valid() {
    assert_eq!(errors(&data()), Vec::<String>::new());
}

#[test]
fn open_tools_can_only_name_allowlisted_programs_and_pages() {
    let e = errors_after("test.cleanup", |t| t.open.as_mut().unwrap().program = Some("cmd".into()));
    assert!(e.iter().any(|m| m.contains("不在名单里：cmd")), "{e:?}");
    let e = errors_after("test.windows-update", |t| t.open.as_mut().unwrap().settings = Some("../x".into()));
    assert!(e.iter().any(|m| m.contains("不在名单里：../x")), "{e:?}");
    let e = errors_after("test.cleanup", |t| t.open.as_mut().unwrap().settings = Some("windowsupdate".into()));
    assert!(e.iter().any(|m| m.contains("只能写 program 或 settings 之一")), "{e:?}");
    let e = errors_after("test.cleanup", |t| t.open = None);
    assert!(e.iter().any(|m| m.contains("只能写 program 或 settings 之一")), "{e:?}");
}

#[test]
fn each_group_only_takes_its_own_fields() {
    // open 不跑脚本
    let e = errors_after("test.cleanup", |t| t.run = tools()[1].run.clone());
    assert!(e.iter().any(|m| m.contains("open 小工具只写 open")), "{e:?}");
    // info 只读，不用确认；action 不显示表格
    let e = errors_after("test.hardware", |t| t.confirm = tools()[1].confirm.clone());
    assert!(e.iter().any(|m| m.contains("只有 action 小工具能写 confirm")), "{e:?}");
    let e = errors_after("test.flush", |t| t.labels = tools()[0].labels.clone());
    assert!(e.iter().any(|m| m.contains("只有 info 小工具能写 labels")), "{e:?}");
    // info / action 必须有脚本和结果
    let e = errors_after("test.flush", |t| t.run = None);
    assert!(e.iter().any(|m| m.contains("必须写 run")), "{e:?}");
    let e = errors_after("test.flush", |t| t.results.clear());
    assert!(e.iter().any(|m| m.contains("results 不能为空")), "{e:?}");
    let e = errors_after("test.flush", |t| t.open = tools()[2].open.clone());
    assert!(e.iter().any(|m| m.contains("只有 open 小工具能写 open")), "{e:?}");
}

#[test]
fn tool_scripts_and_links_must_exist() {
    let e = errors_after("test.flush", |t| t.run.as_mut().unwrap().script = "tools/test/nope.ps1".into());
    assert!(e.iter().any(|m| m.contains("引用的脚本不存在")), "{e:?}");

    let mut d = data();
    d.checks[0].results.get_mut("low").unwrap().links.push("tool:test.nope".into());
    let e = errors(&d);
    assert!(e.iter().any(|m| m.contains("链接无效：tool:test.nope")), "{e:?}");

    let e = errors_after("test.flush", |t| t.results.get_mut("done").unwrap().links.push("tool:test.gone".into()));
    assert!(e.iter().any(|m| m.contains("链接无效：tool:test.gone")), "{e:?}");
}

#[test]
fn tool_ids_are_unique() {
    let mut d = data();
    let dup = d.tools[2].clone();
    d.tools.push(dup);
    let e = errors(&d);
    assert!(e.iter().any(|m| m.contains("ID 重复：test.cleanup")), "{e:?}");
}

// ───────────── 目录摘要 ─────────────

#[test]
fn catalog_summary_lists_tools() {
    let w = world();
    let tools = w.engine.catalog_summary().tools;
    assert_eq!(tools.len(), TOOLS.len());
    let by_id = |id: &str| tools.iter().find(|t| t.id == id).unwrap().clone();
    let flush = by_id("test.flush");
    assert_eq!((flush.group, flush.opens, flush.confirm.as_deref()), (ToolGroup::Action, None, Some("确定要刷新吗？")));
    assert_eq!(by_id("test.cleanup").opens, Some(ToolOpens::Program));
    assert_eq!(by_id("test.windows-update").opens, Some(ToolOpens::Settings));
    let dm = serde_json::to_value(by_id("test.device-manager")).unwrap();
    assert_eq!(dm["audience"], "helper");
    assert_eq!(dm["group"], "open");
}

// ───────────── 运行 info / action ─────────────

#[test]
fn info_tools_render_sections_with_labels() {
    let w = world();
    w.runner.returns(
        "tools/test/hardware.ps1",
        json!({
            "result": "ok",
            "facts": { "disk_count": 1 },
            "sections": [
                { "id": "disk", "name": "Samsung SSD 870", "rows": [
                    { "id": "size", "value": "466 GB" }, { "id": "type", "code": "ssd" }
                ] },
                { "id": "wifi", "name": "我家", "rows": [ { "id": "password", "value": "12345678", "secret": true } ] }
            ]
        }),
    );
    let r = w.engine.tool_run("test.hardware").unwrap();
    assert_eq!((r.status, r.result_code.as_deref(), r.error.as_deref()), (Status::Ok, Some("ok"), None));
    assert_eq!(r.message, "这台电脑有 1 块硬盘。");
    assert_eq!(r.sections[0].title, "硬盘：Samsung SSD 870");
    assert_eq!(r.sections[0].rows[1].value, "固态硬盘");
    assert!(r.sections[1].rows[0].secret);

    // user_hive 的小工具拿到的是登录用户的注册表
    let calls = w.runner.calls();
    assert_eq!(
        calls[0].1.get("UserHive").and_then(Value::as_str),
        Some(format!("Registry::HKEY_USERS\\{SID}").as_str())
    );
    let v = serde_json::to_value(&r).unwrap();
    assert!(v["sections"][1]["rows"][0]["secret"].as_bool().unwrap(), "camelCase 序列化：{v}");
}

#[test]
fn missing_labels_are_reported_in_error() {
    let w = world();
    w.runner.returns(
        "tools/test/hardware.ps1",
        json!({ "result": "ok", "facts": { "disk_count": 1 },
                "sections": [ { "id": "disk", "rows": [ { "id": "speed", "value": "7000 MB/s" } ] } ] }),
    );
    let r = w.engine.tool_run("test.hardware").unwrap();
    assert_eq!(r.status, Status::Ok, "表格照样显示");
    assert_eq!(r.sections[0].rows[0].label, "speed");
    assert!(r.error.as_deref().is_some_and(|e| e.contains("labels.rows.speed")), "{:?}", r.error);
}

#[test]
fn action_tools_render_their_message() {
    let w = world();
    w.runner.returns(
        "tools/test/flush.ps1",
        json!({ "result": "done", "facts": { "entries": 42 }, "sections": [ { "id": "x", "rows": [] } ] }),
    );
    let r = w.engine.tool_run("test.flush").unwrap();
    assert_eq!(r.message, "清掉了 42 条记录。");
    assert_eq!(r.next.as_deref(), Some("还是打不开的话，看看「上不了网」。"));
    assert_eq!(r.links, vec!["tool:test.cleanup".to_owned()]);
    assert!(r.sections.is_empty(), "action 不显示表格");
    assert!(w.runner.calls()[0].1.is_empty(), "没有 user_hive 就不传 -UserHive");
}

#[test]
fn failing_or_unknown_results_are_reported_honestly() {
    let w = world();
    w.runner.on("tools/test/flush.ps1", |_| Err(ScriptError::Failed("DNS Client 服务没有运行".into())));
    let r = w.engine.tool_run("test.flush").unwrap();
    assert_eq!((r.status, r.message.as_str()), (Status::Unknown, "没能完成。"));
    assert!(r.error.as_deref().is_some_and(|e| e.contains("DNS Client")), "{:?}", r.error);

    w.runner.returns("tools/test/hardware.ps1", json!({ "result": "surprise" }));
    let r = w.engine.tool_run("test.hardware").unwrap();
    assert_eq!((r.status, r.message.as_str()), (Status::Unknown, "没能读出来。"));
    assert!(r.error.as_deref().is_some_and(|e| e.contains("surprise")), "{:?}", r.error);
}

#[test]
fn admin_only_tools_refuse_without_admin() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = MockPlatform::new();
    p.admin = false;
    let platform = Arc::new(p);
    let runner = Arc::new(MockRunner::new());
    let engine = Engine::new(
        Catalog::new(data()),
        "test",
        "0.0.0-test",
        platform,
        runner.clone(),
        Journal::open(dir.path().join("journal.jsonl")).unwrap(),
    );
    let r = engine.tool_run("test.hardware").unwrap();
    assert!(r.error.as_deref().is_some_and(|e| e.contains("管理员")), "{:?}", r.error);
    assert!(runner.calls().is_empty(), "不该去跑脚本");
}

// ───────────── 打开系统工具 ─────────────

#[test]
fn open_tools_ask_the_platform_to_open_allowlisted_targets() {
    let w = world();
    w.engine.tool_open("test.device-manager").unwrap();
    w.engine.tool_open("test.windows-update").unwrap();
    assert_eq!(
        w.platform.opened(),
        vec![OpenRequest::Program { exe: "mmc.exe", args: &["devmgmt.msc"] }, OpenRequest::Settings("windowsupdate"),]
    );
}

#[test]
fn missing_programs_are_explained() {
    let w = world();
    w.platform.remove_program("cleanmgr.exe");
    let e = w.engine.tool_open("test.cleanup").unwrap_err().to_string();
    assert!(e.contains("这台电脑上没有「磁盘清理」") && e.contains("cleanmgr.exe"), "{e}");
}

#[test]
fn groups_cannot_be_mixed_up() {
    let w = world();
    assert!(w.engine.tool_run("test.cleanup").is_err(), "open 小工具不能运行");
    assert!(w.engine.tool_open("test.flush").is_err(), "action 小工具不能打开");
    assert!(w.engine.tool_run("test.nope").is_err());
    assert!(w.engine.tool_open("test.nope").is_err());
    assert!(w.runner.calls().is_empty() && w.platform.opened().is_empty());
}

#[test]
fn tools_never_write_the_journal() {
    let w = world();
    w.runner.returns("tools/test/flush.ps1", json!({ "result": "done", "facts": { "entries": 1 } }));
    w.engine.tool_run("test.flush").unwrap();
    w.engine.tool_open("test.cleanup").unwrap();
    assert!(w.engine.journal_list().unwrap().is_empty());
}
