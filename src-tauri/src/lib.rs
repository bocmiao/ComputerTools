//! 电脑小药箱的桌面外壳。拆成 lib + bin 两部分，是为了让 tests/ 里的集成测试能调用命令。
//!
//! 诊断和修复命令只接受 ID；批量重命名、图片批量处理的目录，「文件删不掉」要查的文件由原生选择器取得
//! （见 docs/architecture.md 第 9 节）。

pub mod awake;
pub mod commands;
pub mod images;
pub mod rename;
pub mod setup;

/// 注册命令处理器。`run()` 和集成测试都用它，保证测的和真跑的是同一套。
#[macro_export]
macro_rules! command_handler {
    () => {
        tauri::generate_handler![
            $crate::commands::system_info,
            $crate::commands::catalog_summary,
            $crate::commands::symptom_detail,
            $crate::commands::run_profile,
            $crate::commands::run_check,
            $crate::commands::feature_detect,
            $crate::commands::feature_preview,
            $crate::commands::feature_apply,
            $crate::commands::journal_list,
            $crate::commands::journal_undo,
            $crate::commands::journal_undo_session,
            $crate::commands::report_generate,
            $crate::commands::tool_run,
            $crate::commands::tool_open,
            $crate::commands::startup_list,
            $crate::commands::startup_set,
            $crate::commands::context_menu_list,
            $crate::commands::context_menu_set,
            $crate::commands::rename_select_folder,
            $crate::commands::rename_preview,
            $crate::commands::rename_apply,
            $crate::commands::rename_undo,
            $crate::commands::image_select_folder,
            $crate::commands::image_save,
            $crate::commands::image_open_folder,
            $crate::commands::screen_fullscreen,
            $crate::commands::awake_get,
            $crate::commands::awake_set,
            $crate::commands::lockers_pick_files,
            $crate::commands::lockers_pick_folder,
            $crate::commands::lockers_refresh,
        ]
    };
}

pub fn run() {
    // 启动前先清掉可能被注入的 WebView2 环境变量（管理员进程会继承启动它的用户级进程的环境）。
    setup::harden_environment();

    if let Err(message) = setup::preflight() {
        setup::fatal(&message);
        return;
    }
    if !setup::claim_single_instance() {
        setup::fatal("小药箱已经在运行了，请在任务栏里找到它的窗口。\n\n如果是在另一个账户里打开的，请先在那边关掉。");
        return;
    }
    let state = setup::init();
    let result =
        tauri::Builder::default().manage(state).invoke_handler(command_handler!()).run(tauri::generate_context!());
    if let Err(e) = result {
        let message = setup::webview2_hint().unwrap_or_else(|| format!("界面启动失败：{e}"));
        setup::fatal(&message);
    }
}
