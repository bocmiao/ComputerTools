//! 界面能调用的命令。每个命令都在后台线程里跑，界面不会卡住；出错时返回给用户看的中文说明。

use std::sync::Arc;

use medkit_core::Engine;
use medkit_core::views::{
    ApplyResult, CatalogSummary, CheckResult, FeatureState, JournalSession, Preview, StartupItem, SymptomDetail,
    SystemInfo, ToolResult, UndoResult,
};
use tauri::State;

use crate::rename::{self, RenamePreview};
use crate::setup::AppState;

type CmdResult<T> = Result<T, String>;

async fn with_engine<T, F>(state: State<'_, AppState>, f: F) -> CmdResult<T>
where
    T: Send + 'static,
    F: FnOnce(&Engine) -> medkit_core::Result<T> + Send + 'static,
{
    let engine: Arc<Engine> = state.engine.clone()?;
    tauri::async_runtime::spawn_blocking(move || f(&engine).map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("内部错误：{e}"))?
}

#[tauri::command]
pub async fn system_info(state: State<'_, AppState>) -> CmdResult<SystemInfo> {
    with_engine(state, |e| Ok(e.system_info())).await
}

#[tauri::command]
pub async fn catalog_summary(state: State<'_, AppState>) -> CmdResult<CatalogSummary> {
    with_engine(state, |e| Ok(e.catalog_summary())).await
}

#[tauri::command]
pub async fn symptom_detail(state: State<'_, AppState>, id: String) -> CmdResult<SymptomDetail> {
    with_engine(state, move |e| e.symptom_detail(&id)).await
}

#[tauri::command]
pub async fn run_profile(state: State<'_, AppState>, id: String) -> CmdResult<Vec<CheckResult>> {
    with_engine(state, move |e| e.run_profile(&id)).await
}

#[tauri::command]
pub async fn run_check(state: State<'_, AppState>, id: String) -> CmdResult<CheckResult> {
    with_engine(state, move |e| e.run_check(&id)).await
}

#[tauri::command]
pub async fn feature_detect(state: State<'_, AppState>, id: String) -> CmdResult<FeatureState> {
    with_engine(state, move |e| e.feature_detect(&id)).await
}

#[tauri::command]
pub async fn feature_preview(state: State<'_, AppState>, id: String) -> CmdResult<Preview> {
    with_engine(state, move |e| e.feature_preview(&id)).await
}

#[tauri::command]
pub async fn feature_apply(state: State<'_, AppState>, id: String) -> CmdResult<ApplyResult> {
    with_engine(state, move |e| e.feature_apply(&id)).await
}

#[tauri::command]
pub async fn journal_list(state: State<'_, AppState>) -> CmdResult<Vec<JournalSession>> {
    with_engine(state, |e| e.journal_list()).await
}

#[tauri::command]
pub async fn journal_undo(state: State<'_, AppState>, entry_id: String, force: bool) -> CmdResult<UndoResult> {
    with_engine(state, move |e| e.journal_undo(&entry_id, force)).await
}

#[tauri::command]
pub async fn journal_undo_session(state: State<'_, AppState>, session_id: String) -> CmdResult<Vec<UndoResult>> {
    with_engine(state, move |e| e.journal_undo_session(&session_id)).await
}

/// `note`：用户自己写的「遇到了什么问题」（可以不传），和报告一起脱敏。
#[tauri::command]
pub async fn report_generate(state: State<'_, AppState>, note: Option<String>) -> CmdResult<String> {
    with_engine(state, move |e| e.report_generate(note.as_deref())).await
}

#[tauri::command]
pub async fn tool_run(state: State<'_, AppState>, id: String) -> CmdResult<ToolResult> {
    with_engine(state, move |e| e.tool_run(&id)).await
}

#[tauri::command]
pub async fn tool_open(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    with_engine(state, move |e| e.tool_open(&id)).await
}

#[tauri::command]
pub async fn startup_list(state: State<'_, AppState>) -> CmdResult<Vec<StartupItem>> {
    with_engine(state, |e| e.startup_list()).await
}

/// 停用或恢复一个开机启动项（和任务管理器同一个开关）。只认最近一次列出来的启动项 ID。
#[tauri::command]
pub async fn startup_set(state: State<'_, AppState>, id: String, enabled: bool) -> CmdResult<ApplyResult> {
    with_engine(state, move |e| e.startup_set(&id, enabled)).await
}

#[tauri::command]
pub async fn rename_select_folder(state: State<'_, AppState>) -> CmdResult<Option<String>> {
    #[cfg(windows)]
    let folder = tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new().set_title("选择要批量重命名的文件夹").pick_folder()
    })
    .await
    .map_err(|e| format!("打开文件夹选择器失败：{e}"))?;
    #[cfg(not(windows))]
    let folder: Option<std::path::PathBuf> = None;
    let Some(folder) = folder else { return Ok(None) };
    let folder = folder.canonicalize().map_err(|e| format!("无法读取所选文件夹：{e}"))?;
    let display = folder.display().to_string();
    let mut rename = state.rename.lock().map_err(|_| "批量重命名状态异常。")?;
    rename.folder = Some(folder);
    rename.preview = None;
    Ok(Some(display))
}

#[tauri::command]
pub async fn rename_preview(state: State<'_, AppState>, prefix: String) -> CmdResult<RenamePreview> {
    let selected = state.rename.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut rename = selected.lock().map_err(|_| "批量重命名状态异常。")?;
        rename::preview(&mut rename, &prefix)
    })
    .await
    .map_err(|e| format!("内部错误：{e}"))?
}

#[tauri::command]
pub async fn rename_apply(state: State<'_, AppState>) -> CmdResult<usize> {
    let selected = state.rename.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut rename = selected.lock().map_err(|_| "批量重命名状态异常。")?;
        rename::apply(&mut rename)
    })
    .await
    .map_err(|e| format!("内部错误：{e}"))?
}
