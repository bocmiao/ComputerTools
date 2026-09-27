//! 界面和后端的接线测试：用 Tauri 的 mock 运行时，把界面命令逐个真调一遍，
//! 确认它们能过权限校验、能序列化成界面期望的 camelCase 结构。
//!
//! 引擎用的是真实的内嵌数据（catalog/）+ 假系统（MockPlatform / MockRunner），所以不碰真实系统。
//! 脚本类的检测在假系统上会返回错误结果——这里测的是**结构**对不对，不是检测结论。

use serde_json::{Value, json};
use tauri::WebviewWindow;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;

fn app() -> WebviewWindow<tauri::test::MockRuntime> {
    let app = mock_builder()
        .manage(medkit_lib::setup::test_state())
        .invoke_handler(medkit_lib::command_handler!())
        .build(tauri::generate_context!())
        .expect("构建 mock 应用");
    tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default()).build().expect("构建窗口")
}

/// 调一个命令，返回 Ok(结果 JSON) 或 Err(错误信息)。
fn invoke(win: &WebviewWindow<tauri::test::MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
    get_ipc_response(
        win,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(args),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|b| b.deserialize::<Value>().expect("结果能转成 JSON"))
}

fn ok(win: &WebviewWindow<tauri::test::MockRuntime>, cmd: &str, args: Value) -> Value {
    invoke(win, cmd, args).unwrap_or_else(|e| panic!("命令 {cmd} 出错：{e}"))
}

/// 断言对象里有这些 key（camelCase），一个都不能少。
fn has_keys(v: &Value, keys: &[&str]) {
    let obj = v.as_object().unwrap_or_else(|| panic!("期望是对象，得到：{v}"));
    for k in keys {
        assert!(obj.contains_key(*k), "缺少字段 {k}；实际字段：{:?}", obj.keys().collect::<Vec<_>>());
    }
}

#[test]
fn every_command_is_reachable_and_well_shaped() {
    let win = app();

    // system_info
    has_keys(
        &ok(&win, "system_info", json!({})),
        &[
            "osCaption",
            "build",
            "edition",
            "isAdmin",
            "interactiveUser",
            "elevatedUserMismatch",
            "appVersion",
            "catalogVersion",
        ],
    );

    // catalog_summary：拿到真实的检测清单、症状、功能
    let summary = ok(&win, "catalog_summary", json!({}));
    has_keys(&summary, &["profiles", "symptoms", "features", "tools"]);
    let profiles = summary["profiles"].as_array().unwrap();
    assert!(profiles.iter().any(|p| p["id"] == "healthcheck"), "应该有 healthcheck 清单");
    has_keys(&profiles[0], &["id", "title", "checkCount"]);
    let features = summary["features"].as_array().unwrap();
    assert!(!features.is_empty(), "应该有功能");
    has_keys(
        &features[0],
        &[
            "id",
            "title",
            "description",
            "category",
            "risk",
            "level",
            "recommend",
            "subjective",
            "reboot",
            "reversible",
            "irreversibleReason",
            "applicable",
            "notApplicableReason",
        ],
    );
    let symptoms = summary["symptoms"].as_array().unwrap();
    has_keys(&symptoms[0], &["id", "title", "summary", "keywords", "maturity"]);

    // symptom_detail：用真实症状 ID
    let sid = symptoms[0]["id"].as_str().unwrap().to_owned();
    let detail = ok(&win, "symptom_detail", json!({ "id": sid }));
    has_keys(&detail, &["id", "title", "causes", "guide", "steps"]);
    if let Some(step) = detail["steps"].as_array().and_then(|s| s.first()) {
        has_keys(step, &["check", "checkTitle", "stopOn", "fixes"]);
    }

    // run_check / run_profile：脚本在假系统上会失败，但结构必须完整
    let check_id = summary["symptoms"]
        .as_array()
        .and_then(|s| s.iter().find_map(|s| s["id"].as_str()))
        .map(|_| detail["steps"][0]["check"].as_str().unwrap().to_owned())
        .unwrap();
    has_keys(
        &ok(&win, "run_check", json!({ "id": check_id })),
        &[
            "id",
            "title",
            "category",
            "status",
            "resultCode",
            "message",
            "fixer",
            "next",
            "links",
            "facts",
            "error",
            "durationMs",
        ],
    );
    let profile_results = ok(&win, "run_profile", json!({ "id": "healthcheck" }));
    assert!(profile_results.is_array(), "run_profile 返回数组");

    // 功能：detect / preview
    let fid = features[0]["id"].as_str().unwrap().to_owned();
    has_keys(&ok(&win, "feature_detect", json!({ "id": &fid })), &["id", "state", "details", "error"]);
    has_keys(
        &ok(&win, "feature_preview", json!({ "id": &fid })),
        &["feature", "changes", "willCreateRestorePoint", "notes"],
    );

    // 修改日志、报告
    assert!(ok(&win, "journal_list", json!({})).is_array());
    assert!(ok(&win, "report_generate", json!({})).is_string());

    // 小工具：结构和数据文件对得上；每一组都拿一个真调一次
    let tools = summary["tools"].as_array().unwrap();
    for t in tools {
        has_keys(t, &["id", "title", "description", "category", "group", "opens", "audience", "confirm"]);
    }
    let of_group = |g: &str| {
        tools
            .iter()
            .find(|t| t["group"] == g)
            .map(|t| t["id"].as_str().unwrap().to_owned())
            .unwrap_or_else(|| panic!("应该有 {g} 小工具"))
    };
    // info / action：假系统上脚本会失败，但返回的是完整的结果结构，不是命令错误
    for id in ["info", "action"].map(of_group) {
        let r = ok(&win, "tool_run", json!({ "id": id }));
        has_keys(
            &r,
            &["id", "title", "status", "resultCode", "message", "next", "links", "sections", "error", "durationMs"],
        );
    }
    // open：假系统上直接成功，返回 null
    assert!(ok(&win, "tool_open", json!({ "id": of_group("open") })).is_null());
    assert!(ok(&win, "startup_list", json!({})).is_array());
}

#[test]
fn undo_commands_are_reachable() {
    let win = app();
    // 没有这条记录，应该返回「找不到」而不是崩溃或权限错
    let e = invoke(&win, "journal_undo", json!({ "entryId": "nope", "force": false })).unwrap_err();
    assert!(e.is_string(), "journal_undo 出错时返回字符串，实际：{e}");
    assert!(ok(&win, "journal_undo_session", json!({ "sessionId": "nope" })).is_array());
}

#[test]
fn unknown_ids_come_back_as_string_errors() {
    let win = app();
    for (cmd, args) in [
        ("symptom_detail", json!({ "id": "nope" })),
        ("run_check", json!({ "id": "nope" })),
        ("run_profile", json!({ "id": "nope" })),
        ("feature_detect", json!({ "id": "nope" })),
        ("feature_preview", json!({ "id": "nope" })),
        ("feature_apply", json!({ "id": "nope" })),
        ("tool_run", json!({ "id": "nope" })),
        ("tool_open", json!({ "id": "nope" })),
        ("startup_disable", json!({ "id": "nope" })),
    ] {
        let e = invoke(&win, cmd, args).unwrap_err();
        assert!(e.is_string(), "{cmd} 对未知 ID 应返回字符串错误，实际：{e}");
    }
}
