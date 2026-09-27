// 前后端接口的类型定义：逐字照抄 docs/architecture.md 第 9 节，这是和 Rust 后端的合同。
// 不要在这里改字段；要改，先改文档和后端，再同步到这里。

export type Status = 'ok' | 'advice' | 'manual' | 'unknown' | 'na'
export type Fixer = 'medkit' | 'system' | 'user' | 'helper' | 'vendor' | 'isp' | 'hardware'
export type Risk = 'safe' | 'caution' | 'danger'
export type Level = 'light' | 'medium' | 'heavy'
export type Recommend = 'recommended' | 'optional' | 'not-recommended'
export type Reboot = 'none' | 'explorer' | 'logoff' | 'reboot'
export type Maturity = 'guide' | 'semi' | 'one-click'
export type FeatureStateKind = 'applied' | 'not-applied' | 'partial' | 'unknown'

export interface SystemInfo {
  osCaption: string; build: number; edition: string
  isAdmin: boolean; interactiveUser: string | null; elevatedUserMismatch: boolean
  appVersion: string; catalogVersion: string
}
export interface ProfileSummary { id: string; title: string; checkCount: number }
export interface SymptomSummary { id: string; title: string; summary: string | null; keywords: string[]; maturity: Maturity }
export interface FeatureSummary {
  id: string; title: string; description: string; category: string
  risk: Risk; level: Level; recommend: Recommend; subjective: boolean; reboot: Reboot
  reversible: boolean; irreversibleReason: string | null
  applicable: boolean                  // 这台电脑能不能用（系统版本、Windows 版本不对就不能）
  notApplicableReason: string | null   // 不能用的原因，给用户看
}
export interface CatalogSummary {
  profiles: ProfileSummary[]; symptoms: SymptomSummary[]; features: FeatureSummary[]
  tools: ToolSummary[]
}
export interface SymptomStep { check: string; checkTitle: string; stopOn: Status[]; fixes: FeatureSummary[] }
export interface SymptomDetail extends SymptomSummary { causes: string[]; guide: string | null; steps: SymptomStep[] }
export interface CheckResult {
  id: string; title: string; category: string
  status: Status; resultCode: string | null; message: string
  fixer: Fixer | null; next: string | null; links: string[]
  facts: Record<string, unknown>; error: string | null; durationMs: number
}
export interface FeatureState { id: string; state: FeatureStateKind; details: string[]; error: string | null }
export interface PreviewChange { target: string; current: string; planned: string }
export interface Preview { feature: FeatureSummary; changes: PreviewChange[]; willCreateRestorePoint: boolean; notes: string[] }
export interface ApplyResult {
  feature: string; sessionId: string; entryIds: string[]; ok: boolean
  verified: FeatureStateKind; message: string; reboot: Reboot; notes: string[]; error: string | null
}
// ApplyResult 的三种结果，界面要分开显示：
//   ok && verified === 'applied'   已经改好、复查也确认生效
//   ok && verified !== 'applied'   改了，但复查没确认生效（message 会说明）
//   !ok                            没改成，已经退回原样；这时 reboot 一定是 'none'
// ok && entryIds 为空：本来就是好的，什么都没改
export interface JournalEntryView {
  id: string; sessionId: string; time: string; feature: string; featureTitle: string
  target: string; before: string; after: string; ok: boolean
  pending: boolean              // 程序在改的过程中退出，状态不确定
  undone: boolean; undoneAt: string | null
  canUndo: boolean              // 界面据此决定显不显示「恢复原状」
  error: string | null
}
export interface JournalSession { id: string; startedAt: string; entries: JournalEntryView[] }
export interface UndoResult {
  entryId: string; ok: boolean; drift: boolean; message: string; error: string | null
  reboot: Reboot               // 恢复以后要做什么才看得到效果；没恢复成功时是 'none'
}

// 开机启动项（第 9 节）：开关和任务管理器的「启动应用」是同一个
export type StartupSource = 'user-run' | 'machine-run' | 'machine-run32' | 'user-folder' | 'common-folder'
export type StartupSignature = 'valid' | 'unsigned' | 'invalid' | 'unknown' | 'skipped'
export type StartupAdvice = 'keep' | 'can-disable'
export interface StartupItem {
  id: string                   // 改开关时原样传回
  source: StartupSource
  name: string                 // 注册表里的值名，或「启动」文件夹里的文件名
  title: string                // 程序文件里写的说明，没有就是 name
  program: string; path: string; exists: boolean
  publisher: string | null     // 签名的发布者，没有签名时是文件里写的公司名
  signature: StartupSignature
  location: string             // 「当前用户（注册表）」这类说明
  enabled: boolean             // 开机时会不会自动启动
  advice: StartupAdvice; reason: string
}

// 右键菜单里软件加的项目（Windows 自带的不列）。command 下次右键就生效；extension、app 按 CLSID 拿掉，要重启资源管理器
export type ContextMenuKind = 'command' | 'extension' | 'app'
export interface ContextMenuItem {
  id: string                   // 改开关时原样传回
  kind: ContextMenuKind
  title: string                // 菜单上的字，或者扩展、应用的名字
  program: string; path: string; exists: boolean
  publisher: string | null
  signature: StartupSignature
  scopes: string[]             // 在哪里右键时出现：「文件」「文件夹空白处」这类
  location: string             // 命令：「所有用户」「当前用户」；扩展和应用是空的
  visible: boolean             // 现在在菜单里显示不显示
  shiftOnly: boolean           // 只在按住 Shift 再右键时显示
  note: string                 // 补充说明，没有是空的
}

/** 别让电脑自己睡着：只在小药箱开着时有效 */
export interface AwakeStatus { on: boolean; display: boolean }

/** 定时关机：小药箱安排的那一次（at 是 Unix 毫秒）。系统查不到别处安排的，重新打开小药箱也不知道 */
export interface ShutdownPlan { at: number; restart: boolean }
export interface ShutdownStatus { plan: ShutdownPlan | null }
/** cancelled 为 false：本来就没有安排（可能已经在别处取消了） */
export interface ShutdownCancel { cancelled: boolean }

// 找大文件和重复文件（路径都是相对选的文件夹的）
export interface SpaceFile {
  /** 「在资源管理器中显示」时传回去 */
  id: number
  name: string
  /** 所在的文件夹，相对选的文件夹；就在选的文件夹里时是空的 */
  folder: string
  size: number
  /** 修改时间（毫秒） */
  modified: number | null
  /** Windows、程序自己的文件，别手动删 */
  protected: boolean
}
export interface DuplicateGroup { size: number; count: number; files: SpaceFile[] }
export interface SpaceReport {
  folder: string
  files: number
  totalBytes: number
  /** 没有权限打开的子文件夹 */
  skipped: number
  /** 没算的「仅在线」网盘文件 */
  onlineOnly: number
  /** 文件太多或者时间到了，没数完 */
  truncated: boolean
  largest: SpaceFile[]
  duplicates: DuplicateGroup[]
  duplicateGroups: number
  /** 每组只留一个能腾出多少字节 */
  wastedBytes: number
  /** 可能重复的都比较完了 */
  comparedAll: boolean
}

// 文件删不掉：是谁占着（只有文件名，查文件夹时是相对这个文件夹的路径，没有完整路径）
export type FileUserKind = 'window' | 'console' | 'explorer' | 'other' | 'service' | 'critical'
export interface FileLockUser {
  pid: number
  /** 程序的说明、服务的显示名，没有就是程序的文件名 */
  name: string
  /** 程序的文件名（不带路径） */
  program: string | null
  kind: FileUserKind
  /** 服务名（系统服务才有） */
  service: string | null
  /** 它在用的文件；查文件夹、里面的文件又很多时说不出是哪几个，是空的 */
  files: string[]
  moreFiles: number
  /** 就是小药箱自己 */
  isSelf: boolean
  /** 在另一个用户的登录会话里 */
  otherSession: boolean
}
export interface FileLockReport {
  mode: 'files' | 'folder'
  /** 选中的文件的名字，或者文件夹的名字 */
  targets: string[]
  checked: number
  /** 选好以后又不见了的文件 */
  missing: string[]
  /** 没能查的文件 */
  failed: string[]
  /** 文件太多，只查了前面一部分 */
  truncated: boolean
  /** 没有权限打开的子文件夹个数 */
  unreadable: number
  users: FileLockUser[]
}

// 小工具（第 11 节）
export type ToolGroup = 'info' | 'action' | 'open'
export type ToolOpens = 'program' | 'settings'
export type Audience = 'everyone' | 'helper'
export interface ToolSummary {
  id: string; title: string; description: string; category: string
  group: ToolGroup
  opens: ToolOpens | null      // 只有 open 有值：打开的是系统工具，还是「设置」里的一页
  audience: Audience           // helper：给懂哥用的，界面上标出来
  confirm: string | null       // 只有 action 可能有：执行前要用户确认的说明
}
export interface ToolRow { label: string; value: string; secret: boolean; qr: boolean }   // secret：默认遮住，不进「复制全部」；qr：值画成二维码（扫码连 WiFi）
export interface ToolSection { title: string; rows: ToolRow[] }
export interface ToolResult {
  id: string; title: string
  status: Status; resultCode: string | null; message: string; next: string | null; links: string[]
  sections: ToolSection[]      // 只有 info 有内容
  error: string | null; durationMs: number
}

// 批量重命名（界面只传规则，文件夹由后端的系统选择框决定）
export type RenameOrder = 'name' | 'modified'
export type RenameExtension = 'keep' | 'lower' | 'set'
export interface RenameRules {
  extensions: string           // 只处理这些扩展名（「jpg, png」）；空的表示全部
  order: RenameOrder           // 序号按文件名还是修改时间排
  find: string; replace: string
  numbering: boolean           // 整个名字换成「新名字 + 序号」
  base: string; start: number
  digits: number               // 0：自动
  prefix: string; suffix: string
  extension: RenameExtension; newExtension: string
}
export interface RenameEntry { source: string; target: string; changed: boolean }
export interface RenamePreview {
  folder: string
  entries: RenameEntry[]
  changed: number              // 名字会变的文件数
  skipped: number              // 扩展名不对、不处理的文件数
}
