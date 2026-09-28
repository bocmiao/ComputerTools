//! 界面能调用的命令。每个命令都在后台线程里跑，界面不会卡住；出错时返回给用户看的中文说明。

use std::sync::Arc;

use medkit_core::Engine;
use medkit_core::keymap::MappingInput;
use medkit_core::lockers::LockTarget;
use medkit_core::views::{
    ApplyResult, CatalogSummary, CheckResult, ContextMenuItem, FeatureState, FileLockReport, JournalSession,
    KeyRemapView, NewMenuItem, OcrView, Preview, ShellPlaceItem, StartupItem, SymptomDetail, SystemInfo, ToolResult,
    UndoResult, WindowOwnerReport,
};
use tauri::State;

use crate::awake::AwakeStatus;
use crate::disk_speed::DriveView;
use crate::exe_check::ExeCheckView;
use crate::hidden::{self, HiddenReport, HiddenRestore, HiddenUndo};
use crate::images;
use crate::long_image;
use crate::pdf;
use crate::recycle_bin::{RecycleDriveView, RecycleRepairView};
use crate::rename::{self, RenamePreview, RenameRules};
use crate::setup::AppState;
use crate::shutdown::{ShutdownCancel, ShutdownStatus};
use crate::space::{self, SpaceReport};

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
pub async fn tool_open(state: State<'_, AppState>, id: String) -> CmdResult<Option<String>> {
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

/// 右键「新建」菜单里的项（软件加的和 Windows 自带的）。
#[tauri::command]
pub async fn new_menu_list(state: State<'_, AppState>) -> CmdResult<Vec<NewMenuItem>> {
    with_engine(state, |e| e.new_menu_list()).await
}

/// 从「新建」菜单里关掉（visible 为 false）或者恢复一项，记进修改日志。只接受最近一次列表里的 ID（扩展名）。
#[tauri::command]
pub async fn new_menu_set(state: State<'_, AppState>, id: String, visible: bool) -> CmdResult<ApplyResult> {
    with_engine(state, move |e| e.new_menu_set(&id, visible)).await
}

/// 软件加在资源管理器导航栏和「此电脑」里的图标（网盘、WPS 云文档这些）。
#[tauri::command]
pub async fn shell_places_list(state: State<'_, AppState>) -> CmdResult<Vec<ShellPlaceItem>> {
    with_engine(state, |e| e.shell_places_list()).await
}

/// 隐藏（visible 为 false）或者恢复一个图标，记进修改日志。只接受最近一次列表里的 ID。
#[tauri::command]
pub async fn shell_places_set(state: State<'_, AppState>, id: String, visible: bool) -> CmdResult<ApplyResult> {
    with_engine(state, move |e| e.shell_places_set(&id, visible)).await
}

/// 现在的改键（键位重映射），和能选的键。
#[tauri::command]
pub async fn key_remap_get(state: State<'_, AppState>) -> CmdResult<KeyRemapView> {
    with_engine(state, |e| e.key_remap_get()).await
}

/// 把改键整个换成这些（空的：全部恢复），记进修改日志，重启电脑以后生效。键只能是名单里的。
#[tauri::command]
pub async fn key_remap_set(state: State<'_, AppState>, mappings: Vec<MappingInput>) -> CmdResult<ApplyResult> {
    with_engine(state, move |e| e.key_remap_set(&mappings)).await
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

/// 图片转文字最多收这么大的图片（界面先转成 PNG；很长的截图也就几十 MB）。
const OCR_MAX_BYTES: usize = 64 * 1024 * 1024;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
/// 同一时间只认一张（临时文件夹里剩下的文件要清掉，不能清掉正在认的那张）
static OCR_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 图片转文字：图片（界面转好的 PNG）就是请求体。存成 `%ProgramData%\Medkit\ocr` 里的临时文件交给 Windows 自带的
/// 文字识别，认完马上删掉（上次没来得及删的也一起删）。只读：不改设置，不记修改日志。
#[tauri::command]
pub async fn ocr_recognize(state: State<'_, AppState>, request: tauri::ipc::Request<'_>) -> CmdResult<OcrView> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("图片内容的格式不对（要直接传二进制）。".into());
    };
    if bytes.len() > OCR_MAX_BYTES {
        return Err("图片太大了（超过 64 MB），先裁小一点再试。".into());
    }
    if !bytes.starts_with(PNG_SIGNATURE) {
        return Err("图片要先转成 PNG 才能认字。".into());
    }
    let bytes = bytes.clone();
    with_engine(state, move |e| {
        let _guard = OCR_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = crate::setup::scratch_dir("ocr").map_err(medkit_core::Error::Invalid)?;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|t| t.is_file()) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        let path = dir.join(format!("{}.png", medkit_core::journal::new_id()));
        std::fs::write(&path, &bytes)
            .map_err(|err| medkit_core::Error::Invalid(format!("没能把图片交给 Windows：{err}")))?;
        let result = e.ocr_recognize(&path);
        let _ = std::fs::remove_file(&path);
        result
    })
    .await
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

/// 图片合成 PDF：用系统的「另存为」对话框选存到哪里、叫什么，把界面拼好的 PDF 存进去。
/// PDF 内容就是请求体（二进制）；建议的文件名按 URL 编码放在请求头 `x-medkit-name` 里。
/// 用户点了「取消」返回 null；存好了返回完整路径（只在界面上显示）。
#[tauri::command]
pub async fn pdf_save(state: State<'_, AppState>, request: tauri::ipc::Request<'_>) -> CmdResult<Option<String>> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("PDF 内容的格式不对（要直接传二进制）。".into());
    };
    pdf::check(bytes)?;
    let suggested = request
        .headers()
        .get("x-medkit-name")
        .and_then(|v| v.to_str().ok())
        .and_then(images::decode_component)
        .filter(|name| rename::check_name(name).is_ok())
        .unwrap_or_else(|| pdf::DEFAULT_NAME.to_owned());
    let bytes = bytes.clone();
    #[cfg(windows)]
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        rfd::FileDialog::new()
            .set_title("把 PDF 存到哪里")
            .set_file_name(suggested)
            .add_filter("PDF 文件", &["pdf"])
            .save_file()
    })
    .await
    .map_err(|e| format!("打开「另存为」对话框失败：{e}"))?;
    #[cfg(not(windows))]
    let chosen: Option<std::path::PathBuf> = {
        let _ = suggested;
        None
    };
    let Some(chosen) = chosen else { return Ok(None) };
    let (path, may_replace) = pdf::target(chosen);
    let saved = path.clone();
    tauri::async_runtime::spawn_blocking(move || pdf::write(&path, &bytes, may_replace))
        .await
        .map_err(|e| format!("内部错误：{e}"))??;
    let display = saved.display().to_string();
    state.pdf.lock().map_err(|_| "PDF 状态异常。")?.saved = Some(saved);
    Ok(Some(display))
}

/// 图片合成 PDF：在资源管理器里显示刚存好的 PDF（打开它所在的文件夹并选中它）。
#[tauri::command]
pub async fn pdf_reveal(state: State<'_, AppState>) -> CmdResult<()> {
    let path = state.pdf.lock().map_err(|_| "PDF 状态异常。")?.saved.clone();
    let path = path.ok_or("还没有存过 PDF。")?;
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || medkit_core::platform::windows::reveal_file(&path))
            .await
            .map_err(|e| format!("内部错误：{e}"))?
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("只有在 Windows 上才能打开资源管理器。".into())
    }
}

/// 长图拼接：用系统的「另存为」对话框选存到哪里、叫什么，把界面拼好的长图（JPG 或 PNG）存进去。
/// 图片内容就是请求体（二进制）；建议的文件名按 URL 编码放在请求头 `x-medkit-name` 里。
/// 用户点了「取消」返回 null；存好了返回完整路径（只在界面上显示）。
#[tauri::command]
pub async fn long_image_save(
    state: State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> CmdResult<Option<String>> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("图片内容的格式不对（要直接传二进制）。".into());
    };
    let kind = long_image::check(bytes)?;
    let name = request
        .headers()
        .get("x-medkit-name")
        .and_then(|v| v.to_str().ok())
        .and_then(images::decode_component)
        .filter(|name| rename::check_name(name).is_ok());
    let suggested = long_image::suggested_name(name.as_deref(), kind);
    let bytes = bytes.clone();
    #[cfg(windows)]
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        rfd::FileDialog::new()
            .set_title("把长图存到哪里")
            .set_file_name(suggested)
            .add_filter(kind.filter_name(), kind.extensions())
            .save_file()
    })
    .await
    .map_err(|e| format!("打开「另存为」对话框失败：{e}"))?;
    #[cfg(not(windows))]
    let chosen: Option<std::path::PathBuf> = {
        let _ = suggested;
        None
    };
    let Some(chosen) = chosen else { return Ok(None) };
    let (path, may_replace) = pdf::target_with(chosen, kind.extensions());
    let saved = path.clone();
    tauri::async_runtime::spawn_blocking(move || pdf::write(&path, &bytes, may_replace))
        .await
        .map_err(|e| format!("内部错误：{e}"))??;
    let display = saved.display().to_string();
    state.long_image.lock().map_err(|_| "长图状态异常。")?.saved = Some(saved);
    Ok(Some(display))
}

/// 长图拼接：在资源管理器里显示刚存好的长图（打开它所在的文件夹并选中它）。
#[tauri::command]
pub async fn long_image_reveal(state: State<'_, AppState>) -> CmdResult<()> {
    let path = state.long_image.lock().map_err(|_| "长图状态异常。")?.saved.clone();
    let path = path.ok_or("还没有存过长图。")?;
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || medkit_core::platform::windows::reveal_file(&path))
            .await
            .map_err(|e| format!("内部错误：{e}"))?
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("只有在 Windows 上才能打开资源管理器。".into())
    }
}

/// U 盘里的文件不见了：用系统的文件夹选择框选 U 盘（或者 U 盘上的文件夹），列出被藏起来的东西。点了「取消」返回 null。
/// Windows 所在的盘不给用：那里的隐藏文件大多是系统自己的。
#[tauri::command]
pub async fn hidden_pick_folder(state: State<'_, AppState>) -> CmdResult<Option<HiddenReport>> {
    #[cfg(windows)]
    let folder = tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new().set_title("选择 U 盘（或者 U 盘上的文件夹）").pick_folder()
    })
    .await
    .map_err(|e| format!("打开文件夹选择器失败：{e}"))?;
    #[cfg(not(windows))]
    let folder: Option<std::path::PathBuf> = None;
    let Some(folder) = folder else { return Ok(None) };
    if hidden::on_system_drive(&folder) {
        return Err(
            "这是 Windows 所在的盘（系统盘），这里的隐藏文件大多是系统自己的，不能在这里用。请选 U 盘、移动硬盘或者别的盘。".into(),
        );
    }
    hidden_scan(state, folder).await.map(Some)
}

async fn hidden_scan(state: State<'_, AppState>, root: std::path::PathBuf) -> CmdResult<HiddenReport> {
    let shared = state.hidden.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = shared.lock().map_err(|_| "状态异常。")?;
        let (report, shown) = hidden::scan(&root, &hidden::SystemAttrs, s.changed.len())?;
        s.root = Some(root);
        s.shown = shown;
        Ok(report)
    })
    .await
    .map_err(|e| format!("内部错误：{e}"))?
}

/// U 盘里的文件不见了：把上次选的文件夹再查一遍。还没选过返回 null。
#[tauri::command]
pub async fn hidden_rescan(state: State<'_, AppState>) -> CmdResult<Option<HiddenReport>> {
    let root = state.hidden.lock().map_err(|_| "状态异常。")?.root.clone();
    match root {
        Some(root) => hidden_scan(state, root).await.map(Some),
        None => Ok(None),
    }
}

/// U 盘里的文件不见了：把勾选的（最近一次结果里的编号）显示出来，文件夹里的一起；程序和脚本文件照样藏着。
/// 原来的属性都记下来，能撤销。返回做了什么和重新查的结果。
#[tauri::command]
pub async fn hidden_restore(state: State<'_, AppState>, ids: Vec<usize>) -> CmdResult<HiddenRestore> {
    let shared = state.hidden.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = shared.lock().map_err(|_| "状态异常。")?;
        let root = s.root.clone().ok_or("请先选择 U 盘。")?;
        let targets = ids
            .iter()
            .map(|&id| s.shown.get(id).cloned().ok_or("有的项目不在刚才的结果里，请重新查一遍。"))
            .collect::<Result<Vec<_>, _>>()?;
        if targets.is_empty() {
            return Err("没有勾选要显示出来的文件。".to_owned());
        }
        let mut changed = std::mem::take(&mut s.changed);
        let result = hidden::restore(&targets, &hidden::SystemAttrs, &mut changed);
        s.changed = changed;
        let (report, shown) = hidden::scan(&root, &hidden::SystemAttrs, s.changed.len())?;
        s.shown = shown;
        Ok(HiddenRestore { result, report })
    })
    .await
    .map_err(|e| format!("内部错误：{e}"))?
}

/// U 盘里的文件不见了：把上一次「显示出来」改过的属性都改回去（重新藏起来）。
#[tauri::command]
pub async fn hidden_undo(state: State<'_, AppState>) -> CmdResult<HiddenUndo> {
    let shared = state.hidden.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut s = shared.lock().map_err(|_| "状态异常。")?;
        if s.changed.is_empty() {
            return Err("没有可以撤销的。".to_owned());
        }
        let mut changed = std::mem::take(&mut s.changed);
        let result = hidden::undo(&mut changed, &hidden::SystemAttrs);
        let report = match s.root.clone() {
            Some(root) => {
                let (report, shown) = hidden::scan(&root, &hidden::SystemAttrs, 0)?;
                s.shown = shown;
                Some(report)
            }
            None => None,
        };
        Ok(HiddenUndo { result, report })
    })
    .await
    .map_err(|e| format!("内部错误：{e}"))?
}

/// 硬盘测速：本机的硬盘分区和 U 盘（光驱、网络驱动器不列），剩余空间不到 2 GB 的不能测。
#[tauri::command]
pub async fn disk_speed_drives() -> CmdResult<Vec<DriveView>> {
    tauri::async_runtime::spawn_blocking(crate::disk_speed::drives).await.map_err(|e| format!("内部错误：{e}"))
}

/// 回收站坏了：各个盘的回收站里有多少东西（只有个数和大小，没有文件名）。
#[tauri::command]
pub async fn recycle_drives() -> CmdResult<Vec<RecycleDriveView>> {
    tauri::async_runtime::spawn_blocking(crate::recycle_bin::drives).await.map_err(|e| format!("内部错误：{e}"))
}

/// 回收站坏了：清空并重建一个盘的回收站（删掉 `<盘>:\$Recycle.Bin`，照微软的办法），里面所有账户的东西都会删掉。
/// 只收现在还在的盘符。
#[tauri::command]
pub async fn recycle_repair(letter: String) -> CmdResult<RecycleRepairView> {
    tauri::async_runtime::spawn_blocking(move || crate::recycle_bin::repair(&letter))
        .await
        .map_err(|e| format!("内部错误：{e}"))?
}

/// 「此应用无法在你的电脑上运行」：用系统的选择框选一个程序文件，看它本身能不能在这台电脑上运行（只读文件开头，
/// 不运行它；结果里只有文件名）。没选返回 null。
#[tauri::command]
pub async fn exe_check_pick() -> CmdResult<Option<ExeCheckView>> {
    #[cfg(windows)]
    let file = tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title("选择打不开的程序（双击时提示「此应用无法在你的电脑上运行」的那个）")
            .add_filter("程序", &["exe", "com", "msi"])
            .add_filter("所有文件", &["*"])
            .pick_file()
    })
    .await
    .map_err(|e| format!("打开文件选择器失败：{e}"))?;
    #[cfg(not(windows))]
    let file: Option<std::path::PathBuf> = None;
    let Some(file) = file else { return Ok(None) };
    tauri::async_runtime::spawn_blocking(move || {
        let (native, build) = crate::exe_check::this_pc();
        crate::exe_check::check(&file, native, build)
    })
    .await
    .map_err(|e| format!("内部错误：{e}"))?
    .map(Some)
}

/// 硬盘测速：在这个盘的根目录写一个关掉就删的临时文件，测顺序写、顺序读、4 KB 随机读，一共大约 20 秒。
/// 只收现在列出来、能测的盘符；同一时间只测一个。
#[tauri::command]
pub async fn disk_speed_run(
    state: State<'_, AppState>,
    letter: String,
) -> CmdResult<medkit_core::disk_speed::SpeedResult> {
    let lock = state.disk_speed.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _busy = lock.try_lock().map_err(|_| "正在测另一个盘，等它测完再测。")?;
        let letter = crate::disk_speed::check_letter(&letter, &crate::disk_speed::drives())?;
        let root = std::path::PathBuf::from(format!("{letter}:\\"));
        medkit_core::disk_speed::run(&root, medkit_core::disk_speed::TEST_BYTES, Default::default())
    })
    .await
    .map_err(|e| format!("内部错误：{e}"))?
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

/// 定时关机：小药箱安排的那一次（没安排是 null）。
#[tauri::command]
pub async fn shutdown_get(state: State<'_, AppState>) -> CmdResult<ShutdownStatus> {
    Ok(state.shutdown.lock().map_err(|_| "状态异常。")?.status())
}

/// 定时关机：`seconds` 秒以后关机（`restart` 时重启），到时间强制关掉所有程序；小药箱安排过的换成新的时间。
#[tauri::command]
pub async fn shutdown_schedule(state: State<'_, AppState>, seconds: u32, restart: bool) -> CmdResult<ShutdownStatus> {
    state.shutdown.lock().map_err(|_| "状态异常。")?.schedule(seconds, restart)
}

/// 定时关机：取消已经安排的关机或重启（不管是谁安排的）。
#[tauri::command]
pub async fn shutdown_cancel(state: State<'_, AppState>) -> CmdResult<ShutdownCancel> {
    state.shutdown.lock().map_err(|_| "状态异常。")?.cancel()
}

/// 「文件删不掉：是谁占着」：记下要查的文件或文件夹，查一次。
async fn lockers_check(state: State<'_, AppState>, target: LockTarget) -> CmdResult<Option<FileLockReport>> {
    *state.lockers.lock().map_err(|_| "状态异常。")? = Some(target.clone());
    with_engine(state, move |e| e.file_lockers(&target)).await.map(Some)
}

/// 文件删不掉：用系统的选择框选文件（可以多选），查哪些程序在用它们。界面拿不到、也传不了别的路径。
/// 没选（点了取消）返回 null。
#[tauri::command]
pub async fn lockers_pick_files(state: State<'_, AppState>) -> CmdResult<Option<FileLockReport>> {
    #[cfg(windows)]
    let files = tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new().set_title("选择删不掉的文件（可以选好几个）").pick_files()
    })
    .await
    .map_err(|e| format!("打开文件选择器失败：{e}"))?;
    #[cfg(not(windows))]
    let files: Option<Vec<std::path::PathBuf>> = None;
    match files {
        Some(files) if !files.is_empty() => lockers_check(state, LockTarget::Files(files)).await,
        _ => Ok(None),
    }
}

/// 文件夹删不掉：用系统的选择框选文件夹，查哪些程序在用里面的文件。没选返回 null。
#[tauri::command]
pub async fn lockers_pick_folder(state: State<'_, AppState>) -> CmdResult<Option<FileLockReport>> {
    #[cfg(windows)]
    let folder =
        tauri::async_runtime::spawn_blocking(|| rfd::FileDialog::new().set_title("选择删不掉的文件夹").pick_folder())
            .await
            .map_err(|e| format!("打开文件夹选择器失败：{e}"))?;
    #[cfg(not(windows))]
    let folder: Option<std::path::PathBuf> = None;
    match folder {
        Some(folder) => lockers_check(state, LockTarget::Folder(folder)).await,
        None => Ok(None),
    }
}

/// 关掉程序以后再查一次上次选的文件或文件夹。还没选过返回 null。
#[tauri::command]
pub async fn lockers_refresh(state: State<'_, AppState>) -> CmdResult<Option<FileLockReport>> {
    let target = state.lockers.lock().map_err(|_| "状态异常。")?.clone();
    match target {
        Some(target) => lockers_check(state, target).await,
        None => Ok(None),
    }
}

/// 「弹窗是哪个软件的」最多等几秒
const POPUP_MAX_WAIT: u32 = 10;

/// 「弹窗是哪个软件的」：等 `seconds` 秒（用户在这段时间里把鼠标移到弹窗上），看鼠标指着的窗口是哪个程序的。
/// 只读：不关窗口、不结束程序。记下程序文件，给「打开所在的文件夹」用；界面拿不到完整路径。
#[tauri::command]
pub async fn popup_find(state: State<'_, AppState>, seconds: u32) -> CmdResult<WindowOwnerReport> {
    let wait = std::time::Duration::from_secs(u64::from(seconds.min(POPUP_MAX_WAIT)));
    let slot = state.popup.clone();
    let (report, path) = with_engine(state, move |e| {
        std::thread::sleep(wait);
        e.window_owner()
    })
    .await?;
    *slot.lock().map_err(|_| "状态异常。")? = path;
    Ok(report)
}

/// 「弹窗是哪个软件的」：在资源管理器里打开刚才找到的程序所在的文件夹，并选中它。
#[tauri::command]
pub async fn popup_reveal(state: State<'_, AppState>) -> CmdResult<()> {
    let path = state.popup.lock().map_err(|_| "状态异常。")?.clone();
    let path = path.ok_or("还没有找到是哪个程序，请先点「开始找」。")?;
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || medkit_core::platform::windows::reveal_file(&path))
            .await
            .map_err(|e| format!("内部错误：{e}"))?
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("只有在 Windows 上才能打开资源管理器。".into())
    }
}

/// 找大文件和重复文件：数一遍这个文件夹，记下结果里列出的文件（「在资源管理器中显示」按编号找）。
async fn space_run(state: State<'_, AppState>, root: std::path::PathBuf) -> CmdResult<SpaceReport> {
    let slot = state.space.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (report, shown) = space::scan(&root);
        let mut s = slot.lock().map_err(|_| "状态异常。")?;
        s.root = Some(root);
        s.shown = shown;
        Ok(report)
    })
    .await
    .map_err(|e| format!("内部错误：{e}"))?
}

/// 找大文件和重复文件：用系统的选择框选一个文件夹，数一遍。只读，不删不改。没选返回 null。
#[tauri::command]
pub async fn space_pick_folder(state: State<'_, AppState>) -> CmdResult<Option<SpaceReport>> {
    #[cfg(windows)]
    let folder = tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new().set_title("选择要找大文件、重复文件的文件夹").pick_folder()
    })
    .await
    .map_err(|e| format!("打开文件夹选择器失败：{e}"))?;
    #[cfg(not(windows))]
    let folder: Option<std::path::PathBuf> = None;
    match folder {
        Some(folder) => space_run(state, folder).await.map(Some),
        None => Ok(None),
    }
}

/// 删掉一些文件以后，把上次选的文件夹再数一遍。还没选过返回 null。
#[tauri::command]
pub async fn space_rescan(state: State<'_, AppState>) -> CmdResult<Option<SpaceReport>> {
    let root = state.space.lock().map_err(|_| "状态异常。")?.root.clone();
    match root {
        Some(root) => space_run(state, root).await.map(Some),
        None => Ok(None),
    }
}

/// 在资源管理器里打开这个文件所在的文件夹并选中它（只是显示，不打开文件）。只接受最近一次结果里的编号。
#[tauri::command]
pub async fn space_reveal(state: State<'_, AppState>, id: usize) -> CmdResult<()> {
    let path = state.space.lock().map_err(|_| "状态异常。")?.shown.get(id).cloned();
    let path = path.ok_or("这个文件不在刚才的结果里，请重新查一遍。")?;
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || medkit_core::platform::windows::reveal_file(&path))
            .await
            .map_err(|e| format!("内部错误：{e}"))?
            .map_err(|e| e.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("只有在 Windows 上才能打开资源管理器。".into())
    }
}

/// 显示器亮度：每个显示器现在的亮度（外接显示器用 DDC/CI 读；电脑调不了的也列出来，亮度是 null）。
#[tauri::command]
pub async fn brightness_list(state: State<'_, AppState>) -> CmdResult<Vec<medkit_core::platform::MonitorBrightness>> {
    with_engine(state, |e| e.monitor_brightness()).await
}

/// 显示器亮度：把一个显示器调到 `percent`（0–100），返回调完以后读回来的亮度。
#[tauri::command]
pub async fn brightness_set(state: State<'_, AppState>, id: String, percent: u8) -> CmdResult<u8> {
    with_engine(state, move |e| e.set_monitor_brightness(&id, percent)).await
}
