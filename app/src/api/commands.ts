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
} from './types'

type NoArgs = Record<string, never>

/**
 * 前端能调用的全部后端命令（docs/architecture.md 第 9 节）。
 * 参数名用 camelCase，Tauri 会自动转成 Rust 那边的 snake_case。
 * 系统诊断命令只接受 ID；批量重命名、图片批量处理的目录由原生选择器取得，界面只传改名规则、文件名和图片内容。
 * image_save、pdf_save、long_image_save 的内容走二进制请求体、文件名走请求头（见 ./index.ts 的 imageSave、pdfSave、
 * longImageSave），不走 JSON。
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
  report_generate: { args: { note?: string }; result: string }
  tool_run: { args: { id: string }; result: ToolResult }
  tool_open: { args: { id: string }; result: null }
  startup_list: { args: NoArgs; result: StartupItem[] }
  startup_set: { args: { id: string; enabled: boolean }; result: ApplyResult }
  context_menu_list: { args: NoArgs; result: ContextMenuItem[] }
  context_menu_set: { args: { id: string; visible: boolean }; result: ApplyResult }
  new_menu_list: { args: NoArgs; result: NewMenuItem[] }
  new_menu_set: { args: { id: string; visible: boolean }; result: ApplyResult }
  shell_places_list: { args: NoArgs; result: ShellPlaceItem[] }
  shell_places_set: { args: { id: string; visible: boolean }; result: ApplyResult }
  rename_select_folder: { args: NoArgs; result: string | null }
  rename_preview: { args: { rules: RenameRules }; result: RenamePreview }
  rename_apply: { args: NoArgs; result: number }
  rename_undo: { args: NoArgs; result: number }
  image_select_folder: { args: NoArgs; result: string | null }
  image_save: { args: { name: string; modified: number; bytes: Uint8Array }; result: string }
  image_open_folder: { args: NoArgs; result: null }
  pdf_save: { args: { name: string; bytes: Uint8Array }; result: string | null }
  pdf_reveal: { args: NoArgs; result: null }
  long_image_save: { args: { name: string; bytes: Uint8Array }; result: string | null }
  long_image_reveal: { args: NoArgs; result: null }
  hidden_pick_folder: { args: NoArgs; result: HiddenReport | null }
  hidden_rescan: { args: NoArgs; result: HiddenReport | null }
  hidden_restore: { args: { ids: number[] }; result: HiddenRestore }
  hidden_undo: { args: NoArgs; result: HiddenUndo }
  disk_speed_drives: { args: NoArgs; result: DriveView[] }
  disk_speed_run: { args: { letter: string }; result: SpeedResult }
  screen_fullscreen: { args: { on: boolean }; result: null }
  awake_get: { args: NoArgs; result: AwakeStatus }
  awake_set: { args: { on: boolean; display: boolean }; result: AwakeStatus }
  shutdown_get: { args: NoArgs; result: ShutdownStatus }
  shutdown_schedule: { args: { seconds: number; restart: boolean }; result: ShutdownStatus }
  shutdown_cancel: { args: NoArgs; result: ShutdownCancel }
  lockers_pick_files: { args: NoArgs; result: FileLockReport | null }
  lockers_pick_folder: { args: NoArgs; result: FileLockReport | null }
  lockers_refresh: { args: NoArgs; result: FileLockReport | null }
  space_pick_folder: { args: NoArgs; result: SpaceReport | null }
  space_rescan: { args: NoArgs; result: SpaceReport | null }
  space_reveal: { args: { id: number }; result: null }
}

export type CommandName = keyof CommandMap
export type CommandArgs<K extends CommandName> = CommandMap[K]['args']
export type CommandResult<K extends CommandName> = CommandMap[K]['result']
