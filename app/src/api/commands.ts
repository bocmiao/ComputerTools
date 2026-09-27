import type {
  ApplyResult,
  CatalogSummary,
  CheckResult,
  FeatureState,
  JournalSession,
  Preview,
  SymptomDetail,
  SystemInfo,
  StartupItem,
  ToolResult,
  UndoResult,
} from './types'

type NoArgs = Record<string, never>

/**
 * 前端能调用的全部后端命令（docs/architecture.md 第 9 节）。
 * 参数名用 camelCase，Tauri 会自动转成 Rust 那边的 snake_case。
 * 系统诊断命令只接受 ID；批量重命名的目录由原生选择器取得，界面只传文件名前缀。
 */
export type CommandMap = {
  system_info: { args: NoArgs; result: SystemInfo }
  catalog_summary: { args: NoArgs; result: CatalogSummary }
  symptom_detail: { args: { id: string }; result: SymptomDetail }
  run_profile: { args: { id: string }; result: CheckResult[] }
  run_check: { args: { id: string }; result: CheckResult }
  feature_detect: { args: { id: string }; result: FeatureState }
  feature_preview: { args: { id: string }; result: Preview }
  feature_apply: { args: { id: string }; result: ApplyResult }
  journal_list: { args: NoArgs; result: JournalSession[] }
  journal_undo: { args: { entryId: string; force: boolean }; result: UndoResult }
  journal_undo_session: { args: { sessionId: string }; result: UndoResult[] }
  report_generate: { args: NoArgs; result: string }
  tool_run: { args: { id: string }; result: ToolResult }
  tool_open: { args: { id: string }; result: null }
  startup_list: { args: NoArgs; result: StartupItem[] }
  startup_disable: { args: { id: string }; result: string }
  rename_select_folder: { args: NoArgs; result: string | null }
  rename_preview: { args: { prefix: string }; result: { folder: string; entries: { source: string; target: string }[] } }
  rename_apply: { args: NoArgs; result: number }
}

export type CommandName = keyof CommandMap
export type CommandArgs<K extends CommandName> = CommandMap[K]['args']
export type CommandResult<K extends CommandName> = CommandMap[K]['result']
