import { invoke, type InvokeArgs } from '@tauri-apps/api/core'
import type { CommandArgs, CommandName, CommandResult } from './commands'
import type {
  ApplyResult,
  AwakeStatus,
  CatalogSummary,
  CheckResult,
  ContextMenuItem,
  DriveView,
  FeatureState,
  FileLockReport,
  HiddenReport,
  HiddenRestore,
  HiddenUndo,
  JournalSession,
  KeyMappingInput,
  KeyRemapView,
  NewMenuItem,
  Preview,
  RenamePreview,
  RenameRules,
  ShellPlaceItem,
  ShutdownCancel,
  ShutdownStatus,
  SpaceReport,
  SpeedResult,
  StartupItem,
  SymptomDetail,
  SystemInfo,
  ToolResult,
  UndoResult,
  WindowOwnerReport,
} from './types'

export type * from './types'

/**
 * 在 Tauri 窗口里运行时为 true。
 * 在普通浏览器里打开（`pnpm --dir app dev`）时为 false，这时所有命令都改走 ./mock.ts 里的示例数据。
 */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

// 示例后端单独打包，只有在浏览器里才会加载；Tauri 窗口里永远不会用到它。
let mockModule: Promise<typeof import('./mock')> | null = null

async function call<K extends CommandName>(cmd: K, args: CommandArgs<K>): Promise<CommandResult<K>> {
  if (isTauri()) {
    return invoke<CommandResult<K>>(cmd, args as InvokeArgs)
  }
  return callMock(cmd, args)
}

async function callMock<K extends CommandName>(cmd: K, args: CommandArgs<K>): Promise<CommandResult<K>> {
  mockModule ??= import('./mock')
  const mock = await mockModule
  return mock.mockInvoke(cmd, args)
}

/** 系统版本、是否管理员等 */
export function systemInfo(): Promise<SystemInfo> {
  return call('system_info', {})
}

/** 所有检测清单、症状、功能、小工具的摘要 */
export function catalogSummary(): Promise<CatalogSummary> {
  return call('catalog_summary', {})
}

/** 一个症状的原因、检查步骤和手动指引 */
export function symptomDetail(id: string): Promise<SymptomDetail> {
  return call('symptom_detail', { id })
}

/** 跑一个检测清单（例如体检 healthcheck） */
export function runProfile(id: string): Promise<CheckResult[]> {
  return call('run_profile', { id })
}

/** 跑单个检测 */
export function runCheck(id: string): Promise<CheckResult> {
  return call('run_check', { id })
}

/** 查一个功能现在是不是已经生效 */
export function featureDetect(id: string): Promise<FeatureState> {
  return call('feature_detect', { id })
}

/** 预览一个功能会改哪些地方（只读，不改动） */
export function featurePreview(id: string): Promise<Preview> {
  return call('feature_preview', { id })
}

/** 执行一个功能。改动会记进修改日志 */
export function featureApply(id: string): Promise<ApplyResult> {
  return call('feature_apply', { id })
}

/** 修改日志，按会话分组，新的在前 */
export function journalList(): Promise<JournalSession[]> {
  return call('journal_list', {})
}

/** 撤销一条修改。force 为 false 时，如果这一项后来被改过，会返回 drift: true 而不撤销 */
export function journalUndo(entryId: string, force: boolean): Promise<UndoResult> {
  return call('journal_undo', { entryId, force })
}

/** 按倒序撤销一个会话里所有还没撤销的修改；被改过的项目会跳过 */
export function journalUndoSession(sessionId: string): Promise<UndoResult[]> {
  return call('journal_undo_session', { sessionId })
}

/** 生成已脱敏的纯文本诊断报告。note 是用户自己写的「遇到了什么问题」，会和报告一起脱敏 */
export function reportGenerate(note?: string): Promise<string> {
  return call('report_generate', note ? { note } : {})
}

/** 跑一个「看信息」或「一键处理」小工具。不改设置，也不记进修改日志 */
export function toolRun(id: string): Promise<ToolResult> {
  return call('tool_run', { id })
}

/** 打开一个系统自带的工具，或「设置」里的一页。打不开时 reject 一句说明（例如这台电脑上没有这个工具） */
export function toolOpen(id: string): Promise<null> {
  return call('tool_open', { id })
}

/** 开机启动项：注册表 Run 项和「启动」文件夹，开关状态和任务管理器里的一样 */
export function startupList(): Promise<StartupItem[]> {
  return call('startup_list', {})
}

/** 停用（enabled 为 false）或恢复一个开机启动项；和任务管理器同一个开关，记进修改日志 */
export function startupSet(id: string, enabled: boolean): Promise<ApplyResult> {
  return call('startup_set', { id, enabled })
}

/** 右键菜单里软件加的项目（Windows 自带的不列），显示不显示和改的时候读写同一个位置 */
export function contextMenuList(): Promise<ContextMenuItem[]> {
  return call('context_menu_list', {})
}

/** 从右键菜单里拿掉（visible 为 false）或者恢复一项，记进修改日志；只接受最近一次列表里的 ID */
export function contextMenuSet(id: string, visible: boolean): Promise<ApplyResult> {
  return call('context_menu_set', { id, visible })
}

/** 右键「新建」菜单里的项 */
export function newMenuList(): Promise<NewMenuItem[]> {
  return call('new_menu_list', {})
}

/** 从「新建」菜单里关掉（visible 为 false）或者恢复一项，记进修改日志 */
export function newMenuSet(id: string, visible: boolean): Promise<ApplyResult> {
  return call('new_menu_set', { id, visible })
}

/** 软件加在资源管理器导航栏和「此电脑」里的图标 */
export function shellPlacesList(): Promise<ShellPlaceItem[]> {
  return call('shell_places_list', {})
}

/** 隐藏（visible 为 false）或者恢复一个图标，记进修改日志；只接受最近一次列表里的 ID */
export function shellPlacesSet(id: string, visible: boolean): Promise<ApplyResult> {
  return call('shell_places_set', { id, visible })
}

/** 现在的改键（键位重映射），和能选的键 */
export function keyRemapGet(): Promise<KeyRemapView> {
  return call('key_remap_get', {})
}

/** 把改键整个换成这些（空的：全部恢复），记进修改日志，重启电脑以后生效 */
export function keyRemapSet(mappings: KeyMappingInput[]): Promise<ApplyResult> {
  return call('key_remap_set', { mappings })
}

/** 批量重命名：用系统的选择框选文件夹；取消返回 null */
export function renameSelectFolder(): Promise<string | null> {
  return call('rename_select_folder', {})
}

/** 按规则预览选中文件夹里的文件会改成什么名字（不改任何东西） */
export function renamePreview(rules: RenameRules): Promise<RenamePreview> {
  return call('rename_preview', { rules })
}

/** 按刚才的预览改名，返回改了几个文件 */
export function renameApply(): Promise<number> {
  return call('rename_apply', {})
}

/** 撤销上一次改名，返回改回了几个文件 */
export function renameUndo(): Promise<number> {
  return call('rename_undo', {})
}

/** 图片批量处理：用系统的选择框选处理好的图片存到哪里；取消返回 null */
export function imageSelectFolder(): Promise<string | null> {
  return call('image_select_folder', {})
}

/**
 * 图片批量处理：把处理好的一张图片存进选好的文件夹，只新建、不覆盖（重名时后端在名字后面加「 (2)」），
 * 返回实际用的文件名。图片内容直接作为二进制请求体传（比转成 JSON 数组快得多）；文件名按 URL 编码放进
 * 请求头（请求头只能是 ASCII），modified 是原图的修改时间（毫秒）。
 */
export function imageSave(name: string, modified: number, bytes: Uint8Array): Promise<string> {
  if (isTauri()) {
    return invoke<string>('image_save', bytes, {
      headers: {
        'x-medkit-name': encodeURIComponent(name),
        'x-medkit-modified': String(Math.max(0, Math.round(modified))),
      },
    })
  }
  return callMock('image_save', { name, modified, bytes })
}

/** 图片批量处理：在资源管理器里打开选好的保存文件夹 */
export function imageOpenFolder(): Promise<null> {
  return call('image_open_folder', {})
}

/**
 * 图片合成 PDF：弹出系统的「另存为」对话框，把拼好的 PDF 存到用户选的地方（同名时对话框会先问要不要替换）。
 * 返回存好的完整路径；用户点了「取消」返回 null。内容直接作为二进制请求体传，建议的文件名按 URL 编码放进请求头。
 */
export function pdfSave(name: string, bytes: Uint8Array): Promise<string | null> {
  if (isTauri()) {
    return invoke<string | null>('pdf_save', bytes, { headers: { 'x-medkit-name': encodeURIComponent(name) } })
  }
  return callMock('pdf_save', { name, bytes })
}

/** 图片合成 PDF：在资源管理器里显示刚存好的 PDF */
export function pdfReveal(): Promise<null> {
  return call('pdf_reveal', {})
}

/**
 * 长图拼接：弹出系统的「另存为」对话框，把拼好的长图（JPG 或 PNG）存到用户选的地方。
 * 返回存好的完整路径；用户点了「取消」返回 null。内容直接作为二进制请求体传，建议的文件名按 URL 编码放进请求头。
 */
export function longImageSave(name: string, bytes: Uint8Array): Promise<string | null> {
  if (isTauri()) {
    return invoke<string | null>('long_image_save', bytes, { headers: { 'x-medkit-name': encodeURIComponent(name) } })
  }
  return callMock('long_image_save', { name, bytes })
}

/** 长图拼接：在资源管理器里显示刚存好的长图 */
export function longImageReveal(): Promise<null> {
  return call('long_image_reveal', {})
}

/** U 盘里的文件不见了：用系统的选择框选 U 盘（系统盘不行），列出被藏起来的东西；取消返回 null */
export function hiddenPickFolder(): Promise<HiddenReport | null> {
  return call('hidden_pick_folder', {})
}

/** U 盘里的文件不见了：把上次选的再查一遍；还没选过返回 null */
export function hiddenRescan(): Promise<HiddenReport | null> {
  return call('hidden_rescan', {})
}

/** U 盘里的文件不见了：把勾选的（结果里的编号）显示出来，程序和脚本文件照样藏着；能撤销 */
export function hiddenRestore(ids: number[]): Promise<HiddenRestore> {
  return call('hidden_restore', { ids })
}

/** U 盘里的文件不见了：撤销上一次「显示出来」（重新藏起来） */
export function hiddenUndo(): Promise<HiddenUndo> {
  return call('hidden_undo', {})
}

/** 硬盘测速：本机的硬盘分区和 U 盘（光驱、网络驱动器不列） */
export function diskSpeedDrives(): Promise<DriveView[]> {
  return call('disk_speed_drives', {})
}

/** 硬盘测速：测这个盘（写一个关掉就删的临时文件，大约 20 秒） */
export function diskSpeedRun(letter: string): Promise<SpeedResult> {
  return call('disk_speed_run', { letter })
}

/** 屏幕坏点测试：小药箱窗口进入、退出全屏 */
export function screenFullscreen(on: boolean): Promise<null> {
  return call('screen_fullscreen', { on })
}

/** 别让电脑自己睡着：现在开没开 */
export function awakeGet(): Promise<AwakeStatus> {
  return call('awake_get', {})
}

/** 别让电脑自己睡着：打开（display 为 true 时屏幕也亮着）或者关掉 */
export function awakeSet(on: boolean, display: boolean): Promise<AwakeStatus> {
  return call('awake_set', { on, display })
}

/** 定时关机：小药箱安排的那一次 */
export function shutdownGet(): Promise<ShutdownStatus> {
  return call('shutdown_get', {})
}

/** 定时关机：seconds 秒以后关机（restart 时重启），到时间强制关掉所有程序；小药箱安排过的换成新的时间 */
export function shutdownSchedule(seconds: number, restart: boolean): Promise<ShutdownStatus> {
  return call('shutdown_schedule', { seconds, restart })
}

/** 定时关机：取消已经安排的关机或重启（不管是谁安排的） */
export function shutdownCancel(): Promise<ShutdownCancel> {
  return call('shutdown_cancel', {})
}

/** 文件删不掉：用系统的选择框选文件（可以多选），查哪些程序在用它们；取消返回 null */
export function lockersPickFiles(): Promise<FileLockReport | null> {
  return call('lockers_pick_files', {})
}

/** 文件夹删不掉：用系统的选择框选文件夹，查哪些程序在用里面的文件；取消返回 null */
export function lockersPickFolder(): Promise<FileLockReport | null> {
  return call('lockers_pick_folder', {})
}

/** 关掉程序以后，再查一次上次选的文件或文件夹；还没选过返回 null */
export function lockersRefresh(): Promise<FileLockReport | null> {
  return call('lockers_refresh', {})
}

/** 弹窗是哪个软件的：等 seconds 秒（用户把鼠标移到弹窗上），看鼠标指着的窗口是哪个程序的（只读） */
export function popupFind(seconds: number): Promise<WindowOwnerReport> {
  return call('popup_find', { seconds })
}

/** 在资源管理器里打开刚才找到的程序所在的文件夹，并选中它 */
export function popupReveal(): Promise<null> {
  return call('popup_reveal', {})
}

/** 找大文件和重复文件：用系统的选择框选文件夹，数一遍（只读）；取消返回 null */
export function spacePickFolder(): Promise<SpaceReport | null> {
  return call('space_pick_folder', {})
}

/** 把上次选的文件夹再数一遍；还没选过返回 null */
export function spaceRescan(): Promise<SpaceReport | null> {
  return call('space_rescan', {})
}

/** 在资源管理器里显示结果里的这个文件（选中它，不打开） */
export function spaceReveal(id: number): Promise<null> {
  return call('space_reveal', { id })
}
