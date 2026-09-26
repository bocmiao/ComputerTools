//! 电脑小药箱的桌面外壳：准备运行环境、组装引擎，把引擎的功能以命令的形式交给界面。
//!
//! 界面只能传 ID（见 docs/architecture.md 第 9 节），不能传命令字符串或路径。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod setup;

fn main() {
    if let Err(message) = setup::preflight() {
        setup::fatal(&message);
        return;
    }
    let state = setup::init();
    let result = tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::system_info,
            commands::catalog_summary,
            commands::symptom_detail,
            commands::run_profile,
            commands::run_check,
            commands::feature_detect,
            commands::feature_preview,
            commands::feature_apply,
            commands::journal_list,
            commands::journal_undo,
            commands::journal_undo_session,
            commands::report_generate,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        let message = setup::webview2_hint().unwrap_or_else(|| format!("界面启动失败：{e}"));
        setup::fatal(&message);
    }
}
