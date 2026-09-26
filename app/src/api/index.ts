import { invoke, type InvokeArgs } from '@tauri-apps/api/core'
import type { CommandArgs, CommandName, CommandResult } from './commands'
import type {
  ApplyResult,
  CatalogSummary,
  CheckResult,
  FeatureState,
  JournalSession,
  Preview,
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

/** 生成已脱敏的纯文本诊断报告 */
export function reportGenerate(): Promise<string> {
  return call('report_generate', {})
}

/** 跑一个「看信息」或「一键处理」小工具。不改设置，也不记进修改日志 */
export function toolRun(id: string): Promise<ToolResult> {
  return call('tool_run', { id })
}

/** 打开一个系统自带的工具，或「设置」里的一页。打不开时 reject 一句说明（例如这台电脑上没有这个工具） */
export function toolOpen(id: string): Promise<null> {
  return call('tool_open', { id })
}
