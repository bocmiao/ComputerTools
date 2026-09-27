//! 界面能调用的命令。每个命令都在后台线程里跑，界面不会卡住；出错时返回给用户看的中文说明。

use std::sync::Arc;

use medkit_core::Engine;
use medkit_core::views::{
    ApplyResult, CatalogSummary, CheckResult, ContextMenuItem, FeatureState, JournalSession, Preview, StartupItem,
    SymptomDetail, SystemInfo, ToolResult, UndoResult,
};
use tauri::State;

use crate::awake::AwakeStatus;
use crate::images;
use crate::rename::{self, RenamePreview, RenameRules};
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

/// 右键菜单里软件加的项目（Windows 自带的不列），显示不显示和改的时候读写同一个位置。
#[tauri::command]
pub async fn context_menu_list(state: State<'_, AppState>) -> CmdResult<Vec<ContextMenuItem>> {
    with_engine(state, |e| e.context_menu_list()).await
}

/// 从右键菜单里拿掉（visible 为 false）或者恢复一项，记进修改日志。只接受最近一次列表里的 ID。
#[tauri::command]
pub async fn context_menu_set(state: State<'_, AppState>, id: String, visible: bool) -> CmdResult<ApplyResult> {
    with_engine(state, move |e| e.context_menu_set(&id, visible)).await
}

/// 批量重命名：用系统的文件夹选择框选一个文件夹。界面拿不到、也传不了别的路径。
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
    rename.last = None;
    Ok(Some(display))
}

/// 按规则预览选中文件夹里的文件会改成什么名字（不改任何东西）。
#[tauri::command]
pub async fn rename_preview(state: State<'_, AppState>, rules: RenameRules) -> CmdResult<RenamePreview> {
    let selected = state.rename.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut rename = selected.lock().map_err(|_| "批量重命名状态异常。")?;
        rename::preview(&mut rename, &rules)
    })
    .await
    .map_err(|e| format!("内部错误：{e}"))?
}

/// 按刚才的预览改名；文件夹在预览以后变了就不改。返回改了几个文件。
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

/// 撤销上一次改名（改回原名）。返回改回了几个文件。
#[tauri::command]
pub async fn rename_undo(state: State<'_, AppState>) -> CmdResult<usize> {
    let selected = state.rename.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut rename = selected.lock().map_err(|_| "批量重命名状态异常。")?;
        rename::undo(&mut rename)
    })
    .await
    .map_err(|e| format!("内部错误：{e}"))?
}

/// 图片批量处理：用系统的文件夹选择框选保存到哪里。界面拿不到、也传不了别的路径。
#[tauri::command]
pub async fn image_select_folder(state: State<'_, AppState>) -> CmdResult<Option<String>> {
    #[cfg(windows)]
    let folder = tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new().set_title("选择处理好的图片保存到哪个文件夹").pick_folder()
    })
    .await
    .map_err(|e| format!("打开文件夹选择器失败：{e}"))?;
    #[cfg(not(windows))]
    let folder: Option<std::path::PathBuf> = None;
    let Some(folder) = folder else { return Ok(None) };
    let folder = folder.canonicalize().map_err(|e| format!("无法读取所选文件夹：{e}"))?;
    let display = folder.display().to_string();
    state.images.lock().map_err(|_| "图片处理状态异常。")?.folder = Some(folder);
    Ok(Some(display))
}

/// 图片批量处理：把界面处理好的一张图片存进选好的文件夹，只新建、不覆盖（重名就在后面加「 (2)」）。
/// 图片内容就是请求体（二进制）；文件名按 URL 编码放在请求头 `x-medkit-name` 里（请求头只能是 ASCII），
/// 原图的修改时间（毫秒）放在 `x-medkit-modified` 里。返回实际用的文件名。
#[tauri::command]
pub async fn image_save(state: State<'_, AppState>, request: tauri::ipc::Request<'_>) -> CmdResult<String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("图片内容的格式不对（要直接传二进制）。".into());
    };
    let header = |name: &str| request.headers().get(name).and_then(|v| v.to_str().ok());
    let name = header("x-medkit-name").and_then(images::decode_component).ok_or("没有给出文件名。")?;
    let modified = header("x-medkit-modified").and_then(images::modified_from_millis);
    let folder = state.images.lock().map_err(|_| "图片处理状态异常。")?.folder.clone();
    let folder = folder.ok_or("请先选择保存到哪个文件夹。")?;
    let bytes = bytes.clone();
    tauri::async_runtime::spawn_blocking(move || images::save(&folder, &name, &bytes, modified))
        .await
        .map_err(|e| format!("内部错误：{e}"))?
}

/// 图片批量处理：在资源管理器里打开选好的保存文件夹。
#[tauri::command]
pub async fn image_open_folder(state: State<'_, AppState>) -> CmdResult<()> {
    let folder = state.images.lock().map_err(|_| "图片处理状态异常。")?.folder.clone();
    let folder = folder.ok_or("还没有选择保存的文件夹。")?;
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || medkit_core::platform::windows::open_folder(&folder))
            .await
            .map_err(|e| format!("内部错误：{e}"))?
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = folder;
        Err("只有在 Windows 上才能打开文件夹。".into())
    }
}

/// 屏幕坏点测试：窗口进入、退出全屏（盖住任务栏，整块屏幕都是测试的颜色）。
#[tauri::command]
pub async fn screen_fullscreen<R: tauri::Runtime>(window: tauri::WebviewWindow<R>, on: bool) -> CmdResult<()> {
    window.set_fullscreen(on).map_err(|e| format!("窗口没能{}全屏：{e}", if on { "进入" } else { "退出" }))
}

/// 别让电脑自己睡着：现在开没开。
#[tauri::command]
pub async fn awake_get(state: State<'_, AppState>) -> CmdResult<AwakeStatus> {
    Ok(state.awake.lock().map_err(|_| "状态异常。")?.status())
}

/// 别让电脑自己睡着：打开（display 为 true 时屏幕也亮着）或者关掉。只在小药箱开着时有效，不改电源设置。
#[tauri::command]
pub async fn awake_set(state: State<'_, AppState>, on: bool, display: bool) -> CmdResult<AwakeStatus> {
    state.awake.lock().map_err(|_| "状态异常。")?.set(on, display)
}
