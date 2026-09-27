import { invoke, type InvokeArgs } from '@tauri-apps/api/core'
import type { CommandArgs, CommandName, CommandResult } from './commands'
import type {
  ApplyResult,
  CatalogSummary,
  CheckResult,
  ContextMenuItem,
  FeatureState,
  JournalSession,
  Preview,
  RenamePreview,
  RenameRules,
  StartupItem,
  SymptomDetail,
  SystemInfo,
  ToolResult,
  UndoResult,
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

/** 屏幕坏点测试：小药箱窗口进入、退出全屏 */
export function screenFullscreen(on: boolean): Promise<null> {
  return call('screen_fullscreen', { on })
}
