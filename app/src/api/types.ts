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
export interface SymptomDetail extends SymptomSummary {
  causes: string[]
  guide: string | null
  steps: SymptomStep[]
  /** 手动步骤下面的按钮：tool:<id>、symptom:<id> */
  links: string[]
}
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

/** 右键「新建」菜单里的一项（软件加的「新建 Word 文档」这类，也有 Windows 自带的） */
export interface NewMenuItem {
  id: string                   // 扩展名（小写），改开关时原样传回
  title: string                // 菜单上的字
  ext: string
  windowsOwn: boolean          // Windows 自带的（位图图像、文本文档、压缩文件夹这类）
  location: string             // 「所有用户」「当前用户」「所有用户和当前用户」
  visible: boolean             // 现在在菜单里显示不显示
}

/** 资源管理器里的图标在哪：左边导航栏的最上面一层，或者「此电脑」里 */
export type ShellPlace = 'nav' | 'pc'

/** 软件加在资源管理器导航栏或者「此电脑」里的一个图标（网盘、WPS 云文档这些） */
export interface ShellPlaceItem {
  id: string                   // CLSID（大写、带花括号），改开关时原样传回
  title: string                // 资源管理器里显示的名字
  places: ShellPlace[]         // 在哪（导航栏的在前）；两处都有的一起隐藏
  windowsOwn: boolean          // Windows 自带的（OneDrive、图库、3D 对象这些）
  visible: boolean             // 现在显示不显示
  note: string                 // 要特别说明的，多数是空的
}

/** 图片转文字：ok 认完了（可能一个字也没有）；no-language 一种识别都没装；unsupported 没有 Windows 的文字识别；bad-image 读不了图片 */
export type OcrStatus = 'ok' | 'no-language' | 'unsupported' | 'bad-image'
export interface OcrView {
  status: OcrStatus
  text: string                 // 一行一行，中文的字之间没有空格
  lines: number
  language: string | null      // 用的识别语言，说成人话（「中文（简体）」）
  languages: string[]          // 这台电脑装了的识别语言
  chinese: boolean             // 装了中文的识别
  truncated: boolean           // 图片太长，只认了前面一部分
  detail: string | null        // 读不了图片、没有文字识别时 Windows 自己的说法
}

/** 改键能选的一个键：id 和浏览器 KeyboardEvent.code 的名字一样 */
export interface KeyOption {
  id: string
  label: string
  targetOnly: boolean          // 只能当「变成」的键（音量、播放这类多媒体键）
}
/** 已经设置的一条改键：按下 from 变成 to；to 为 null 是这个键不起作用 */
export interface KeyMappingView { from: string; to: string | null; text: string }
/** 改键的现状 */
export interface KeyRemapView {
  keys: KeyOption[]
  mappings: KeyMappingView[]
  foreign: boolean             // 有别的软件设的、小药箱认不出来的键：只能「全部恢复」，不能在这里改
  foreignText: string | null   // 认不出来的那些，说成人话
}
/** 保存改键时的一条；to 为 null：这个键不起作用 */
export interface KeyMappingInput { from: string; to: string | null }

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

// 此应用无法在你的电脑上运行
/** 程序文件本身能不能在这台电脑上运行（只有文件名，见 medkit_core::exe_info） */
export interface ExeCheckView {
  name: string
  size: number
  verdict: 'empty' | 'not-exe' | 'truncated' | 'dll' | 'old16' | 'wrong-machine' | 'not-desktop' | 'ok'
  guess: 'msi' | 'archive' | 'html' | 'pdf' | 'unknown' | null   // 不是程序时像什么
  machine: 'x86' | 'x64' | 'arm64' | 'arm32' | 'ia64' | 'other' | null  // 程序是给哪种处理器的
  pc: 'x86' | 'x64' | 'arm64' | 'arm32' | 'ia64' | 'other'   // 这台电脑的处理器
  windows11: boolean
  console: boolean              // 命令行程序
  dotnet: boolean
}

// 回收站坏了
/** 一个盘的回收站里有多少东西（只有个数和大小） */
export interface RecycleDriveView {
  letter: string
  label: string
  removable: boolean
  system: boolean
  exists: boolean              // 这个盘上有回收站文件夹
  files: number
  bytes: number
  complete: boolean            // 数完了；没数完的是「至少这么多」
}
/** absent：本来就没有；done：删干净了；partly：有的删不掉（left 个） */
export interface RecycleRepairView { letter: string; outcome: 'absent' | 'done' | 'partly'; left: number }

// 硬盘测速
export interface DriveView {
  /** 盘符，比如「C」 */
  letter: string
  label: string
  fileSystem: string
  removable: boolean
  /** Windows 装在这个盘上 */
  system: boolean
  total: number
  free: number
  /** 剩余空间够不够测（至少 2 GB） */
  canTest: boolean
}
export type SpeedVerdict = 'nvme' | 'sata-ssd' | 'ssd-slow' | 'hdd' | 'slow'
export interface SpeedResult {
  /** MB/s（1 MB = 1048576 字节） */
  seqWrite: number
  seqRead: number
  randomRead: number
  randomIops: number
  testedBytes: number
  verdict: SpeedVerdict
}

// U 盘里的文件不见了（只有名字，编号用来勾选）
export interface HiddenItem {
  id: number
  name: string
  isDir: boolean
  /** 文件的大小（文件夹是 0） */
  size: number
  /** 文件夹里一共有多少个文件和文件夹、其中藏起来的有多少 */
  inside: number
  hiddenInside: number
  /** 数完了（太多了或者时间到了就是 false） */
  countedAll: boolean
}
export interface HiddenReport {
  folder: string
  /** 被藏起来的文件和文件夹（能显示出来的） */
  items: HiddenItem[]
  /** 被藏起来的程序和脚本文件：多半是病毒，不显示出来 */
  programs: string[]
  /** 病毒放的快捷方式 */
  shortcuts: string[]
  /** 上一次「显示出来」改了多少处，能撤销 */
  canUndo: number
}
export interface HiddenRestore {
  result: { changed: number; keptPrograms: number; failed: number; truncated: boolean }
  report: HiddenReport
}
export interface HiddenUndo {
  result: { restored: number; failed: number }
  report: HiddenReport | null
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

/** 「弹窗是哪个软件的」：鼠标指着的是什么 */
export type WindowOwnerKind = 'program' | 'system' | 'notification' | 'shell' | 'medkit' | 'nothing' | 'unreadable'
/** 窗口在屏幕的哪一块 */
export type WindowPosition =
  | 'full'
  | 'top-left'
  | 'top'
  | 'top-right'
  | 'left'
  | 'center'
  | 'right'
  | 'bottom-left'
  | 'bottom'
  | 'bottom-right'
/** 「弹窗是哪个软件的」的结果：路径里的用户文件夹名换成了 *，完整路径留在后端 */
export interface WindowOwnerReport {
  kind: WindowOwnerKind
  /** 程序的文件名 */
  exe: string | null
  /** 程序文件里写的说明、公司、产品名 */
  description: string | null
  company: string | null
  product: string | null
  /** 程序所在的文件夹 */
  folder: string | null
  /** 属于「应用和功能」里的哪个软件 */
  installed: string | null
  publisher: string | null
  position: WindowPosition | null
  width: number
  height: number
}

// 小工具（第 11 节）
export type ToolGroup = 'info' | 'action' | 'open'
/** 打开的是系统工具、「设置」里的一页，还是「获取帮助」里微软的疑难解答 */
export type ToolOpens = 'program' | 'settings' | 'get-help' | 'website'
export type Audience = 'everyone' | 'helper'
export interface ToolSummary {
  id: string; title: string; description: string; category: string
  group: ToolGroup
  opens: ToolOpens | null      // 只有 open 有值
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
