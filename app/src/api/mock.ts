/**
 * 示例后端：在普通浏览器里打开界面（`pnpm --dir app dev`）时，src/api/index.ts 会改用这里。
 * 不装 Rust、不在 Windows 上也能开发和演示界面。
 *
 * 所有数据都是编的，不会读取或改动这台电脑。它模拟了一台「有点小毛病」的电脑：
 * 预览、执行、检测、撤销都读写同一份「设置值」，所以修完再查会变正常，撤销以后又会变回来。
 *
 * 地址栏参数可以切换演示场景（可以组合）：
 *   ?allok      体检全部正常
 *   ?mismatch   用别的管理员账户运行（显示提示条）
 *   ?notadmin   没有以管理员身份运行（「查看 WiFi 密码」「刷新 DNS 缓存」这两个小工具会查不出来 / 做不了）
 *   ?win10      这台电脑是 Win10（只有 Win11 能用的功能会标成「这台电脑用不了」）
 *   ?openfail   「打开系统工具」「打开设置里的页面」都打不开（像精简系统那样，工具被删掉了）
 *   ?toolfail   「看信息」「一键处理」的小工具命令本身出错（reject 一个字符串）
 *   ?explorerfail  「重启资源管理器」关掉以后没有自己重新打开（结果带「下一步」和「打开任务管理器」按钮）
 *   ?raid       硬盘接在 RAID 控制器上，「硬盘健康」没查出来（和 ?allok 一起用：一切正常，但有一项没查出来）
 *   ?proxyalive 系统代理指向本机一个正在运行的代理软件：「代理设置」正常，但带一句「下一步」
 *
 * 默认场景里也有演示用的情况：
 *   - 「关闭任务栏上的资讯和兴趣（Win10）」在这台 Win11 上用不了
 *   - 「切换到高性能电源计划」执行一定失败（已退回，reboot 为 none）
 *   - 「打印机共享 0x0000011b 兼容设置」执行后复查没确认生效（ok 但 verified 不是 applied）
 *   - 体检和症状检查的结果里有 tool: 链接（打开「存储」设置、设备管理器、可靠性历史记录，
 *     查看电脑配置，刷新 DNS 缓存）
 */
import type { CommandArgs, CommandName, CommandResult } from './commands'
import type {
  ApplyResult,
  CatalogSummary,
  CheckResult,
  ContextMenuItem,
  DriveView,
  FeatureState,
  FeatureStateKind,
  FeatureSummary,
  FileLockReport,
  HiddenReport,
  FileLockUser,
  JournalEntryView,
  JournalSession,
  KeyMappingInput,
  KeyOption,
  NewMenuItem,
  Preview,
  ShellPlaceItem,
  SpaceReport,
  SpeedResult,
  StartupItem,
  Status,
  SymptomDetail,
  SystemInfo,
  ToolResult,
  ToolRow,
  ToolSection,
  ToolSummary,
  UndoResult,
  WindowOwnerReport,
} from './types'

// ─────────────────────────── 演示场景 ───────────────────────────

const params = new URLSearchParams(window.location.search)
const DEMO_ALL_OK = params.has('allok')
const DEMO_MISMATCH = params.has('mismatch')
const DEMO_NOT_ADMIN = params.has('notadmin')
const DEMO_WIN10 = params.has('win10')
const DEMO_OPEN_FAIL = params.has('openfail')
const DEMO_TOOL_FAIL = params.has('toolfail')
const DEMO_EXPLORER_FAIL = params.has('explorerfail')
const DEMO_RAID = params.has('raid')
const DEMO_PROXY_ALIVE = params.has('proxyalive')

// ─────────────────────────── 辅助函数 ───────────────────────────

const ID_RE = /^[a-z0-9]+([.-][a-z0-9]+)*$/
const MINUTE = 60_000
const DAY = 24 * 60 * MINUTE

function randomInt(min: number, max: number): number {
  return Math.floor(min + Math.random() * (max - min + 1))
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

/** 数字最多保留一位小数（和引擎渲染 message 的规则一致） */
function num(n: number): string {
  return String(Math.round(n * 10) / 10)
}

function iso(ms: number): string {
  return new Date(ms).toISOString()
}

function uuid(): string {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
    const r = Math.floor(Math.random() * 16)
    return (c === 'x' ? r : (r & 0x3) | 0x8).toString(16)
  })
}

/** 模拟 Tauri 的 JSON 序列化：界面拿到的永远是副本 */
function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

function requireId(id: string): string {
  if (typeof id !== 'string' || !ID_RE.test(id)) {
    throw `ID 不合法：${String(id)}`
  }
  return id
}

function localTime(ms: number): string {
  const d = new Date(ms)
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
}

// ─────────────────────────── 系统信息 ───────────────────────────

const SYSTEM: SystemInfo = {
  osCaption: DEMO_WIN10 ? 'Microsoft Windows 10 家庭中文版' : 'Microsoft Windows 11 家庭中文版',
  build: DEMO_WIN10 ? 19045 : 26100,
  edition: '家庭中文版',
  isAdmin: !DEMO_NOT_ADMIN,
  interactiveUser: '小明',
  elevatedUserMismatch: DEMO_MISMATCH,
  appVersion: '0.0.1',
  catalogVersion: '2026.09.26',
}

// ─────────────────────────── 这台示例电脑上的「设置值」 ───────────────────────────

const ABSENT = '（不存在）'
const CLEARED = '清空'

const HKCU_CV = 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion'
const HKCU_ADVANCED = `${HKCU_CV}\\Explorer\\Advanced`
const HKCU_CDM = `${HKCU_CV}\\ContentDeliveryManager`

/** 被好几个检测引用的位置 */
const T = {
  proxyEnable: `${HKCU_CV}\\Internet Settings\\ProxyEnable`,
  winhttpProxy: 'WinHTTP 代理（给系统服务用）',
  dns: '以太网 的 DNS 服务器',
  winsock: 'Winsock 目录',
  discovery: '网络发现（专用网络）',
  sharing: '文件和打印机共享（专用网络）',
  rpcAuth: 'HKLM\\SYSTEM\\CurrentControlSet\\Control\\Print\\RpcAuthnLevelPrivacyEnabled',
  rpcPipe: 'HKLM\\SOFTWARE\\Policies\\Microsoft\\Windows NT\\Printers\\RPC\\RpcUseNamedPipeProtocol',
  tempDir: '临时文件夹（%TEMP%）',
  updateCache: 'Windows 更新下载缓存',
  hiberFile: '休眠文件大小',
} as const

const HIBER_FULL = '完整（12.7 GB）'
const HIBER_REDUCED = '缩小（6.4 GB）'

/** 位置 → 现在的值。没有记录的位置按「不存在」处理 */
const values = new Map<string, string>()

function valueOf(target: string): string {
  return values.get(target) ?? ABSENT
}

// ─────────────────────────── 功能 ───────────────────────────

interface MockChange {
  target: string
  /** 这台示例电脑上一开始的值 */
  initial: string
  /** 执行以后的值 */
  planned: string
  /** 「部分生效」时，检测结果里怎么称呼这一项 */
  label?: string
}

interface MockFeature {
  summary: FeatureSummary
  changes: MockChange[]
  notes: string[]
  /** 一次性的动作（例如刷新 DNS 缓存）：做完以后状态不会一直保持 */
  oneShot?: boolean
  /** 执行时一定失败，用来演示出错的样子 */
  failWith?: string
  /** 检测时一定查不出来 */
  detectError?: string
  /** 执行后的复查结果；不写时按改动后的值判断 */
  verifyAs?: FeatureStateKind
  /** 创建还原点失败（系统还原没开） */
  restorePointFails?: boolean
  /** 适用的系统版本号范围（和引擎的 applies_to 一样）；这台电脑不在范围里就是「用不了」 */
  minBuild?: number
  maxBuild?: number
}

type FeatureInput = Partial<Omit<FeatureSummary, 'applicable' | 'notApplicableReason'>> &
  Pick<FeatureSummary, 'id' | 'title' | 'description' | 'category'>

/** 这台示例电脑用不了的原因（说法和引擎一样）；能用时为 null */
/** Win11 的第一个版本号 */
const WIN11_BUILD = 22000

function notApplicableReason(minBuild?: number, maxBuild?: number): string | null {
  // 说法和引擎一样：最常见的「只有 Win11 有」「只有 Win10 有」说人话，其余的才报版本号
  if (minBuild !== undefined && SYSTEM.build < minBuild) {
    return minBuild === WIN11_BUILD && SYSTEM.build < WIN11_BUILD
      ? '只适用于 Windows 11，这台电脑装的是 Windows 10'
      : `要先把系统更新到版本号 ${minBuild} 或更新（这台是 ${SYSTEM.build}）`
  }
  if (maxBuild !== undefined && SYSTEM.build > maxBuild) {
    return maxBuild < WIN11_BUILD && SYSTEM.build >= WIN11_BUILD
      ? '只适用于 Windows 10，这台电脑装的是 Windows 11'
      : `只适用于版本号 ${maxBuild} 及以前的系统（这台是 ${SYSTEM.build}）`
  }
  return null
}

function defineFeature(summary: FeatureInput, rest: Omit<MockFeature, 'summary'>): MockFeature {
  const reason = notApplicableReason(rest.minBuild, rest.maxBuild)
  return {
    summary: {
      risk: 'safe',
      level: 'light',
      recommend: 'optional',
      subjective: false,
      reboot: 'none',
      reversible: true,
      irreversibleReason: null,
      ...summary,
      applicable: reason === null,
      notApplicableReason: reason,
    },
    ...rest,
  }
}

const FEATURE_LIST: MockFeature[] = [
  // ── 常用设置：资源管理器 ──
  defineFeature(
    {
      id: 'explorer.show-extensions',
      title: '显示文件扩展名',
      description: '在资源管理器里显示 .docx、.exe 这类扩展名，更容易认出伪装成文档的病毒。',
      category: 'explorer',
      recommend: 'recommended',
      reboot: 'explorer',
    },
    {
      changes: [{ target: `${HKCU_ADVANCED}\\HideFileExt`, initial: 'DWORD 1', planned: 'DWORD 0' }],
      notes: ['只改你自己账户的设置，不影响这台电脑上的其他用户。'],
    },
  ),
  defineFeature(
    {
      id: 'explorer.show-hidden-files',
      title: '显示隐藏的文件和文件夹',
      description: '能看到平时被隐藏的文件夹，比如 AppData。找聊天记录、软件配置时有用，平时用不上可以不开。',
      category: 'explorer',
    },
    {
      changes: [{ target: `${HKCU_ADVANCED}\\Hidden`, initial: 'DWORD 2', planned: 'DWORD 1' }],
      notes: ['系统自己的重要文件仍然保持隐藏，不用担心误删。'],
    },
  ),
  defineFeature(
    {
      id: 'explorer.classic-context-menu',
      title: '恢复经典右键菜单（Win11）',
      description: 'Win11 的右键菜单要再点「显示更多选项」才能看到全部功能。开启后，右键直接显示完整的老式菜单。',
      category: 'explorer',
      subjective: true,
      reboot: 'explorer',
    },
    {
      changes: [
        {
          target: 'HKCU\\Software\\Classes\\CLSID\\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\\InprocServer32\\（默认）',
          initial: ABSENT,
          planned: '字符串（空）',
        },
      ],
      notes: ['以后的 Windows 大版本更新可能让它失效，到时候再开一次就行。'],
      minBuild: 22000,
    },
  ),
  defineFeature(
    {
      id: 'explorer.photo-viewer',
      title: '找回 Windows 照片查看器',
      description:
        '老的「Windows 照片查看器」打开快、占内存少，其实还在系统里，只是 Windows 10 以后没登记成能打开 JPG、PNG 这些图片。这里把它登记回来（图片的类型和图标不变），之后在图片上点右键「打开方式 → 选择其他应用」就能选它。不改默认程序，随时可以撤销。',
      category: 'explorer',
      subjective: true,
    },
    {
      changes: [
        {
          target: 'HKLM\\SOFTWARE\\Microsoft\\Windows Photo Viewer\\Capabilities\\FileAssociations\\.jpg',
          initial: ABSENT,
          planned: '字符串 PhotoViewer.FileAssoc.Jpeg',
        },
        {
          target: 'HKLM\\SOFTWARE\\Classes\\PhotoViewer.FileAssoc.Jpeg\\FriendlyTypeName',
          initial: ABSENT,
          planned: '可扩展字符串 @%SystemRoot%\\System32\\shell32.dll,-30596',
        },
      ],
      notes: ['只是登记，不改默认程序：想双击就用它打开，在「打开方式 → 选择其他应用」里选它并勾上「始终使用」。'],
    },
  ),
  // ── 常用设置：桌面 ──
  defineFeature(
    {
      id: 'explorer.jpeg-extension',
      title: '网页上保存的图片是 .jpg，不是 .jfif',
      description:
        '在 Chrome、Edge 里「图片另存为」、在 Teams 这些软件里下载照片，文件名后面变成了 .jfif，有的软件、网站上传时不认。这里把 Windows 登记的 JPEG 图片扩展名改回 .jpg，以后存下来的就是 .jpg；重新打开浏览器以后生效，能撤销。',
      category: 'explorer',
    },
    {
      changes: [
        {
          target: 'HKLM\\SOFTWARE\\Classes\\MIME\\Database\\Content Type\\image/jpeg\\Extension',
          initial: '字符串 .jfif',
          planned: '字符串 .jpg',
        },
      ],
      notes: ['Windows 大更新以后可能又被改回 .jfif，到时候再执行一次就行。'],
    },
  ),
  defineFeature(
    {
      id: 'desktop.show-this-pc',
      title: '桌面显示「此电脑」',
      description: '在桌面上放一个「此电脑」图标，双击就能打开 C 盘、D 盘。',
      category: 'desktop',
      recommend: 'recommended',
      subjective: true,
    },
    {
      changes: [
        {
          target: `${HKCU_CV}\\Explorer\\HideDesktopIcons\\NewStartPanel\\{20D04FE0-3AEA-1069-A2D8-08002B30309D}`,
          initial: ABSENT,
          planned: 'DWORD 0',
        },
      ],
      notes: [],
    },
  ),
  // ── 常用设置：任务栏 ──
  defineFeature(
    {
      id: 'taskbar.align-left',
      title: '任务栏图标靠左（Win11）',
      description: '把 Win11 任务栏上的开始按钮和图标放回左边，和 Win10 的习惯一样。',
      category: 'taskbar',
      subjective: true,
    },
    {
      changes: [{ target: `${HKCU_ADVANCED}\\TaskbarAl`, initial: ABSENT, planned: 'DWORD 0' }],
      notes: [],
      minBuild: 22000,
    },
  ),
  // 只有 Win10 有：在默认的 Win11 示例电脑上演示「这台电脑用不了」（推荐项，也不能算进「只应用推荐项」）
  defineFeature(
    {
      id: 'taskbar.hide-news-interests',
      title: '关闭任务栏上的「资讯和兴趣」（Win10）',
      description: '任务栏右下角不再显示天气和新闻，鼠标划过时也不会突然弹出一大块资讯。',
      category: 'taskbar',
      recommend: 'recommended',
      reboot: 'explorer',
    },
    {
      changes: [{ target: `${HKCU_CV}\\Feeds\\ShellFeedsTaskbarViewMode`, initial: 'DWORD 0', planned: 'DWORD 2' }],
      notes: [],
      maxBuild: 19045,
    },
  ),
  // ── 常用设置：开始菜单 ──
  defineFeature(
    {
      id: 'start.disable-recommendations',
      title: '关闭开始菜单和锁屏的推荐广告',
      description: '开始菜单的「推荐」里不再出现推广的应用，锁屏也不再显示「趣味知识和提示」这类广告。',
      category: 'start',
      recommend: 'recommended',
      reboot: 'logoff',
    },
    {
      changes: [
        {
          target: `${HKCU_ADVANCED}\\Start_IrisRecommendations`,
          initial: ABSENT,
          planned: 'DWORD 0',
          label: '开始菜单「推荐」里的推广',
        },
        {
          target: `${HKCU_CDM}\\RotatingLockScreenOverlayEnabled`,
          initial: 'DWORD 1',
          planned: 'DWORD 0',
          label: '锁屏上的「趣味知识和提示」',
        },
        {
          target: `${HKCU_CDM}\\SubscribedContent-338387Enabled`,
          initial: 'DWORD 1',
          planned: 'DWORD 0',
          label: '锁屏上的应用推荐',
        },
      ],
      notes: [
        '锁屏上的改动要注销后重新登录才能看到。',
        '「推荐」里最近打开的文件不受影响；想一起关掉，可以在「设置 → 个性化 → 开始」里关。',
      ],
    },
  ),
  defineFeature(
    {
      id: 'start.disable-web-search',
      title: '关闭开始菜单里的网络搜索',
      description: '在开始菜单里搜索时只找电脑上的程序和文件，不再夹杂必应的网页结果，搜起来也更快。',
      category: 'start',
      recommend: 'recommended',
      reboot: 'explorer',
    },
    {
      changes: [{ target: `${HKCU_CV}\\Search\\BingSearchEnabled`, initial: ABSENT, planned: 'DWORD 0' }],
      notes: ['想搜网页时，直接打开浏览器搜就好。'],
    },
  ),
  // ── 常用设置：电源 ──
  defineFeature(
    {
      id: 'power.disable-fast-startup',
      title: '关闭快速启动',
      description: '快速启动让「关机」其实只是半休眠。装了双系统，或者关机后 U 盘、网卡偶尔不正常时才需要关；关掉后开机会慢几秒。',
      category: 'power',
      recommend: 'not-recommended',
    },
    {
      changes: [
        {
          target: 'HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Power\\HiberbootEnabled',
          initial: 'DWORD 1',
          planned: 'DWORD 0',
        },
      ],
      notes: ['改的是整台电脑的设置，这台电脑上所有用户都会受影响。', '下次关机时生效。'],
    },
  ),
  defineFeature(
    {
      id: 'power.high-performance',
      title: '切换到「高性能」电源计划',
      description: 'CPU 一直保持较高频率，打开程序会快一点，但更耗电、风扇更吵。笔记本用电池时不建议开。',
      category: 'power',
      risk: 'caution',
      subjective: true,
    },
    {
      changes: [{ target: '电源计划', initial: '平衡', planned: '高性能' }],
      notes: ['笔记本插着电源时效果最明显；用电池时会明显更耗电。'],
      detectError: '没找到「高性能」电源计划，没法判断现在的状态。',
      failWith: '这台电脑上没有「高性能」电源计划，可能被品牌自带的电脑管家删掉了。可以在品牌电脑管家里调「性能模式」。',
    },
  ),

  // ── 修复：网络 ──
  defineFeature(
    {
      id: 'network.proxy-off',
      title: '关闭失效的系统代理',
      description: '代理指向的程序已经不在了（多半是梯子或加速器卸载后留下的），浏览器因此打不开网页。关掉它就能恢复直接上网。',
      category: 'network',
      recommend: 'recommended',
    },
    {
      changes: [
        { target: T.proxyEnable, initial: 'DWORD 1', planned: 'DWORD 0' },
        { target: T.winhttpProxy, initial: '127.0.0.1:7890', planned: '直接连接' },
      ],
      notes: ['以后还要用那个代理软件的话，重新打开它，它会自己把代理设置回去。'],
    },
  ),
  // 以前的版本把「刷新 DNS 缓存」做成了功能，现在它是小工具 network.flush-dns（清缓存不改设置）。
  // 这里还留着，是因为修改日志里有当时的记录。
  defineFeature(
    {
      id: 'network.dns-flush',
      title: '刷新 DNS 缓存',
      description: '清掉电脑记住的网址解析结果，解决「换了网络以后某些网站打不开」这类问题。',
      category: 'network',
      recommend: 'recommended',
      reversible: false,
      irreversibleReason: '清掉的只是临时缓存，电脑会自动重新记，不需要也没法撤销。',
    },
    {
      changes: [{ target: 'DNS 缓存', initial: '236 条记录', planned: CLEARED }],
      notes: [],
      oneShot: true,
    },
  ),
  defineFeature(
    {
      id: 'network.dns-public',
      title: '改用国内公共 DNS',
      description: '把网卡的 DNS 改成阿里（223.5.5.5）和腾讯（119.29.29.29）的公共 DNS。宽带自带的 DNS 不稳定时有用。',
      category: 'network',
    },
    {
      changes: [{ target: T.dns, initial: '自动获取', planned: '223.5.5.5、119.29.29.29' }],
      notes: ['公司或学校的网络可能要求用它们自己的 DNS，这种情况下不要改。'],
    },
  ),
  defineFeature(
    {
      id: 'network.winsock-reset',
      title: '重置 Winsock（网络组件）',
      description: '把所有联网程序都要经过的 Winsock 恢复干净（netsh winsock reset）：去掉加速器、老版本安全软件、上网管理软件插进去的网络组件（LSP），和文件已经不在的组件。修「LSP 损坏」引起的上不了网，要重启电脑。',
      category: 'network',
      risk: 'caution',
      level: 'medium',
      reboot: 'reboot',
      reversible: false,
      irreversibleReason: '没有办法把原来的 Winsock 装回去。被去掉的网络组件属于哪个软件，那个软件的网络功能（加速器、某些 VPN、上网管理软件）要重新安装才能用。',
    },
    {
      changes: [{ target: T.winsock, initial: '有 1 个软件插进去的网络组件', planned: '恢复干净' }],
      notes: ['360、腾讯电脑管家可能会弹窗询问，请选择「允许」。'],
    },
  ),
  defineFeature(
    {
      id: 'network.discovery-on',
      title: '开启网络发现和打印机共享',
      description: '让这台电脑能在局域网里看到别的电脑和共享打印机，别人也能找到它。只对家里、办公室这类「专用网络」生效。',
      category: 'network',
      recommend: 'recommended',
    },
    {
      changes: [
        { target: T.discovery, initial: '关闭', planned: '开启' },
        { target: T.sharing, initial: '关闭', planned: '开启' },
      ],
      notes: ['在咖啡馆、机场这类公用网络上不会生效，不用担心被陌生人看到。'],
    },
  ),

  // ── 修复：磁盘 ──
  defineFeature(
    {
      id: 'disk.cleanup-temp',
      title: '清理临时文件和更新缓存',
      description: '删掉程序用完没清理的临时文件，以及已经装好的更新留下的安装包。不会碰你的文档、照片和聊天记录。',
      category: 'disk',
      recommend: 'recommended',
      reversible: false,
      irreversibleReason: '删掉的临时文件找不回来，但它们本来就是没用的东西，不影响任何软件使用。',
    },
    {
      changes: [
        { target: T.tempDir, initial: '3.2 GB', planned: CLEARED },
        { target: T.updateCache, initial: '4.1 GB', planned: CLEARED },
      ],
      notes: ['正在被程序使用的临时文件会自动跳过。'],
    },
  ),
  defineFeature(
    {
      id: 'disk.reduce-hiberfile',
      title: '缩小休眠文件',
      description:
        '把 C 盘根目录下的休眠文件（hiberfil.sys）换成只给快速启动用的精简版，能腾出大约内存两成那么大的空间（16 GB 内存大约 3 GB），马上生效，不用重启。代价是不能再用「休眠」，「混合睡眠」也会失效（睡眠时突然停电，没保存的东西就没了）；关机、睡眠和快速启动照常。有电池的电脑（笔记本、平板）要靠完整的休眠文件在电量快用完时保存正在做的事，小药箱在这些电脑上不做这一项。',
      category: 'disk',
      risk: 'caution',
    },
    {
      changes: [{ target: T.hiberFile, initial: HIBER_FULL, planned: HIBER_REDUCED }],
      notes: [],
      restorePointFails: true,
    },
  ),

  // ── 修复：打印机 ──
  defineFeature(
    {
      id: 'printer.rpc-privacy-compat',
      title: '旧共享打印机 0x0000011b 兼容设置',
      description: '只在共享打印机的主机上作为最后手段使用；会降低打印 RPC 通信的安全保护。',
      category: 'printer',
      risk: 'danger',
      level: 'heavy',
      recommend: 'not-recommended',
      reboot: 'reboot',
    },
    {
      changes: [{ target: T.rpcAuth, initial: ABSENT, planned: 'DWORD 0' }],
      notes: [
        '这会关闭微软为修补打印漏洞加上的保护，局域网里的其他电脑更容易借打印服务发起攻击。',
        '更好的办法：如果打印机本身有网口或 Wi-Fi，让每台电脑直接按 IP 地址添加它，就不需要这项设置。',
      ],
    },
  ),
  defineFeature(
    {
      id: 'printer.rpc-named-pipes',
      title: '共享打印机改用命名管道连接',
      description: '只在这台电脑是连接方、Windows 11 22H2 后连接旧主机报 0x00000709 时尝试。',
      category: 'printer', risk: 'caution', level: 'medium', reboot: 'reboot',
    },
    {
      changes: [{ target: T.rpcPipe, initial: ABSENT, planned: 'DWORD 1' }],
      notes: ['微软优先建议检查 RPC/TCP 通信和防火墙；命名管道仅用于兼容旧主机。'],
      minBuild: 22621,
    },
  ),
]

const FEATURES = new Map(FEATURE_LIST.map((f) => [f.summary.id, f]))

function getFeature(id: string): MockFeature {
  const f = FEATURES.get(requireId(id))
  if (!f) throw `找不到这个功能：${id}`
  return f
}

function detectKind(f: MockFeature): FeatureStateKind {
  if (f.oneShot) return 'not-applied'
  const done = f.changes.filter((c) => valueOf(c.target) === c.planned).length
  if (done === f.changes.length) return 'applied'
  if (done === 0) return 'not-applied'
  return 'partial'
}

function isApplied(id: string): boolean {
  const f = FEATURES.get(id)
  return f !== undefined && detectKind(f) === 'applied'
}

/** 还没到目标状态、需要改的那几项（一次性动作每次都要做） */
function pendingChanges(f: MockFeature): MockChange[] {
  return f.changes.filter((c) => f.oneShot || valueOf(c.target) !== c.planned)
}

// ─────────────────────────── 检测 ───────────────────────────

type Outcome = Pick<CheckResult, 'status' | 'message'> &
  Partial<Pick<CheckResult, 'resultCode' | 'fixer' | 'next' | 'links' | 'facts' | 'error'>>

interface MockCheck {
  title: string
  evaluate: () => Outcome
}

const CHECKS: Record<string, MockCheck> = {
  // ── 磁盘 ──
  'disk.system-free-space': {
    title: 'C 盘剩余空间',
    evaluate: () => {
      const total = 237.9
      let free = DEMO_ALL_OK ? 86.2 : 8.4
      if (valueOf(T.tempDir) === CLEARED) free += 3.2
      if (valueOf(T.updateCache) === CLEARED) free += 4.1
      if (valueOf(T.hiberFile) === HIBER_REDUCED) free += 6.3
      const pct = (free / total) * 100
      const facts = { free_gb: Number(num(free)), free_pct: Number(num(pct)), total_gb: total }
      if (free >= 20) {
        return { status: 'ok', resultCode: 'ok', message: `C 盘还剩 ${num(free)} GB，空间充足。`, facts }
      }
      return {
        status: 'advice',
        resultCode: 'low',
        message: `C 盘只剩 ${num(free)} GB（${num(pct)}%），可能影响更新和软件运行。`,
        fixer: 'medkit',
        next: '打开「C 盘满了」，看看哪些东西可以清理或搬走。',
        links: ['symptom:disk-full', 'tool:settings.storage'],
        facts,
      }
    },
  },
  'disk.error-events': {
    title: '硬盘读写错误',
    evaluate: () => ({ status: 'ok', resultCode: 'none', message: '最近 30 天没有硬盘读写出错的记录。', facts: { days: 30, total: 0 } }),
  },
  'disk.smart-health': {
    title: '硬盘健康',
    evaluate: () => ({ status: 'ok', resultCode: 'healthy', message: '硬盘状态良好（共 2 块）。', facts: { known_count: 2 } }),
  },
  'disk.hiberfile': {
    title: '休眠文件',
    evaluate: () => {
      if (DEMO_ALL_OK || valueOf(T.hiberFile) === HIBER_REDUCED) {
        return {
          status: 'ok',
          resultCode: 'reduced',
          message: '休眠文件已经是只给快速启动用的精简版，占 6.4 GB，不能再小了。',
          facts: { size_gb: 6.4, memory_gb: 31.9, save_gb: 0, percent: 0 },
        }
      }
      return {
        status: 'advice',
        resultCode: 'full',
        message: '休眠文件占 12.7 GB。换成只给快速启动用的精简版，大约能腾出 6.3 GB。',
        fixer: 'medkit',
        next: '换成精简版以后不能再用「休眠」，「混合睡眠」也会失效（睡眠时突然停电，没保存的东西就没了）；关机、睡眠和快速启动照常。平时不用「休眠」的，可以点下面的按钮缩小；以后想恢复，在修改日志里撤销就行。',
        links: ['feature:disk.reduce-hiberfile'],
        facts: { size_gb: 12.7, memory_gb: 31.9, save_gb: 6.3, percent: 0 },
      }
    },
  },
  'disk.wechat-files': {
    title: '微信和 QQ 的文件',
    evaluate: () => {
      if (DEMO_ALL_OK) {
        return { status: 'ok', message: '微信和 QQ 的文件不多（3.1 GB），不用管。', facts: { wechat_gb: 2.4, qq_gb: 0.7 } }
      }
      return {
        status: 'advice',
        resultCode: 'large',
        message: '微信的聊天文件占了 38.2 GB，全都放在 C 盘。',
        fixer: 'user',
        next: '在微信里打开「设置 → 文件管理」，把保存位置改到 D 盘，旧文件会一起搬过去。QQ 在「设置 → 存储管理」里改。',
        facts: { wechat_gb: 38.2, qq_gb: 2.6, wechat_version: '4.1' },
      }
    },
  },
  'disk.windows-old': {
    title: '旧系统文件夹（Windows.old）',
    evaluate: () => ({ status: 'ok', message: '没有旧系统留下的 Windows.old 文件夹。', facts: { exists: false } }),
  },

  // ── 网络 ──
  'network.connectivity': {
    title: '网络连通',
    evaluate: () => ({
      status: 'ok',
      message: '网络正常：能连上路由器，也能直接连上外网。',
      facts: { gateway_ok: true, internet_ok: true, rtt_ms: 18 },
    }),
  },
  'network.adapter': {
    title: '网卡和飞行模式',
    evaluate: () => ({
      status: 'ok',
      message: '网卡工作正常，飞行模式没有打开。',
      facts: { adapter: '以太网', media: '有线', link_speed_mbps: 1000, airplane_mode: false },
    }),
  },
  'network.ip-address': {
    title: '有没有拿到网络地址',
    evaluate: () => ({ status: 'ok', message: '已经从路由器拿到了网络地址。', facts: { dhcp: true, apipa: false } }),
  },
  'network.gateway': {
    title: '能不能连上路由器',
    evaluate: () => ({ status: 'ok', message: '能连上路由器，响应时间 1 毫秒。', facts: { rtt_ms: 1, loss_pct: 0 } }),
  },
  'network.internet': {
    title: '能不能连上外网',
    evaluate: () => ({
      status: 'ok',
      message: '能直接连上外网（测试了两个国内常用网站的服务器）。',
      facts: { targets_ok: 2, targets_total: 2, rtt_ms: 18 },
    }),
  },
  'network.dns': {
    title: '网址解析（DNS）',
    evaluate: () => {
      if (DEMO_ALL_OK || isApplied('network.dns-public')) {
        return { status: 'ok', message: '网址解析正常，试了 6 次都成功了。', facts: { tries: 6, failures: 0 } }
      }
      return {
        status: 'advice',
        resultCode: 'flaky',
        message: '网址解析不太稳定：试了 6 次，有 2 次没解析出来，网页会时好时坏。',
        fixer: 'medkit',
        next: '先试试「刷新 DNS 缓存」；还不行，可以改用国内公共 DNS。',
        links: ['tool:network.flush-dns'],
        facts: { tries: 6, failures: 2, server: '宽带自动分配' },
      }
    },
  },
  'network.proxy-dead': {
    title: '代理设置',
    evaluate: () => {
      if (DEMO_PROXY_ALIVE) {
        return {
          status: 'ok',
          resultCode: 'alive',
          message:
            '系统代理指向本机 127.0.0.1:7890，那里有程序在运行（一般是代理软件或加速器）。这只说明软件开着，不保证一定能上网。',
          next: '如果网页打不开，可以先把这个代理软件或加速器正常退出，再试试。',
          facts: { proxy_enabled: true, proxy_server: '127.0.0.1:7890', listening: true },
        }
      }
      if (DEMO_ALL_OK || valueOf(T.proxyEnable) === 'DWORD 0') {
        return { status: 'ok', message: '没有设置代理，浏览器直接上网。', facts: { proxy_enabled: false } }
      }
      return {
        status: 'advice',
        resultCode: 'dead',
        message:
          '系统代理指向 127.0.0.1:7890，但那里没有程序在监听，多半是梯子或加速器卸载后留下的。浏览器会打不开网页，微信、QQ 不受影响。',
        fixer: 'medkit',
        next: '关掉这个失效的代理，就能恢复上网。',
        links: ['feature:network.proxy-off', 'symptom:network'],
        facts: { proxy_enabled: true, proxy_server: '127.0.0.1:7890', listening: false, winhttp_proxy: valueOf(T.winhttpProxy) },
      }
    },
  },
  'network.winsock': {
    title: '网络组件（Winsock）有没有被改坏',
    evaluate: () => {
      if (valueOf(T.winsock) === '恢复干净') {
        return {
          status: 'ok',
          resultCode: 'ok',
          message: 'Winsock 正常：26 项都是 Windows 自己的，没有别的软件插进去的网络组件。',
          facts: { entries: 26, lsp_count: 0, missing_count: 0 },
        }
      }
      if (DEMO_ALL_OK) {
        return {
          status: 'ok',
          resultCode: 'ok',
          message: 'Winsock 正常：26 项都是 Windows 自己的，没有别的软件插进去的网络组件。',
          facts: { entries: 26, lsp_count: 0, missing_count: 0 },
        }
      }
      return {
        status: 'advice',
        resultCode: 'lsp',
        message:
          '有 1 个软件往 Winsock 里插了网络组件（LSP）：某网游加速器（xxlsp.dll）。所有联网的程序都要经过它，它出问题时，常见的样子是 QQ、微信能用、网页打不开，或者所有软件都上不了网。',
        fixer: 'medkit',
        next: '先退出、卸载这个软件（常见的是老版本的加速器、上网管理、安全软件），重启电脑再试。还是上不了网，再点下面的「重置 Winsock」：要重启电脑，撤销不了，这个软件的网络功能要重新安装才能用。',
        links: ['feature:network.winsock-reset', 'tool:system.installed-programs'],
        facts: { entries: 31, lsp: '某网游加速器（xxlsp.dll）', lsp_count: 1, missing_count: 0 },
      }
    },
  },
  'network.time': {
    title: '系统时间',
    evaluate: () => ({
      status: 'ok',
      message: '系统时间准确，和标准时间只差 0.3 秒。',
      facts: { offset_sec: 0.3, time_zone: '(UTC+08:00) 北京，重庆，香港特别行政区，乌鲁木齐' },
    }),
  },

  // ── 打印机 ──
  'printer.spooler': {
    title: '打印服务',
    evaluate: () => ({
      status: 'ok',
      message: '打印服务正在运行。若打印任务卡住，可以尝试重启打印服务。',
      links: ['tool:printer.restart-spooler'],
      facts: { service: 'Spooler', state: 'running', start_type: 'auto' },
    }),
  },
  'printer.network-discovery': {
    title: '网络发现和打印机共享',
    evaluate: () => {
      if (isApplied('network.discovery-on')) {
        return { status: 'ok', message: '网络发现和打印机共享都已经开启。', facts: { discovery: true, sharing: true } }
      }
      return {
        status: 'advice',
        resultCode: 'off',
        message: '网络发现和打印机共享都没有开，局域网里的电脑互相看不见。',
        fixer: 'medkit',
        facts: { network_profile: '专用网络', discovery: false, sharing: false },
      }
    },
  },
  'printer.rpc-privacy': {
    title: '共享主机的打印通信保护',
    evaluate: () => {
      if (valueOf(T.rpcAuth) === 'DWORD 0') {
        return { status: 'manual', message: '这台共享主机已关闭打印 RPC 数据包级隐私保护；继续报 0x0000011b，原因不在这个开关。', facts: { shared_printers: 1 } }
      }
      return {
        status: 'advice',
        resultCode: 'secure',
        message: '这台电脑共享了打印机，传入打印通信保护保持开启。若连接方确实报 0x0000011b，可考虑临时兼容设置。',
        fixer: 'medkit',
        next: '如果打印机有网口或 Wi-Fi，更推荐让每台电脑直接按 IP 地址添加打印机。',
        facts: { shared_printers: 1 },
      }
    },
  },
  'printer.rpc-named-pipes': {
    title: '共享打印机连接方式',
    evaluate: () => valueOf(T.rpcPipe) === 'DWORD 1'
      ? { status: 'ok', message: '这台电脑已启用命名管道兼容方式。' }
      : { status: 'advice', message: '这台电脑仍使用默认打印 RPC 连接方式。连接旧主机报 0x00000709 时，可尝试兼容方式。', fixer: 'medkit' },
  },

  // ── 体检里的其他项目 ──
  'system.component-store': {
    title: '系统文件',
    evaluate: () => ({ status: 'ok', message: '系统文件没有发现损坏。', facts: { check_health: 'healthy' } }),
  },
  'update.status': {
    title: 'Windows 更新',
    evaluate: () => ({
      status: 'ok',
      message: 'Windows 更新正常，最近一次装更新是 12 天前。',
      facts: { last_install_days: 12, service_running: true, paused: false },
    }),
  },
  'system.pending-reboot': {
    title: '等待重启',
    evaluate: () => {
      if (DEMO_ALL_OK) return { status: 'ok', message: '没有在等重启的更新。' }
      return {
        status: 'advice',
        resultCode: 'pending',
        message: '有更新已经装好了，正在等你重启电脑。',
        fixer: 'user',
        next: '保存好手头的文件，找个方便的时候重启一下。',
        facts: { since_hours: 26, reasons: ['Windows 更新', '待重命名的文件'] },
      }
    },
  },
  'hardware.disk-health': {
    title: '硬盘健康',
    evaluate: () => {
      if (DEMO_RAID) {
        return {
          status: 'unknown',
          resultCode: null,
          message: '硬盘接在 RAID（磁盘阵列）控制器上，系统读不到它的健康信息，小药箱没法判断。',
          next: '这不代表硬盘有问题。想确认的话，可以请懂哥用品牌自带的管理软件看看。',
          facts: { bus_type: 'RAID' },
        }
      }
      if (DEMO_ALL_OK) {
        return {
          status: 'ok',
          message: '硬盘健康状况良好。',
          facts: { model: 'Samsung SSD 980 1TB', media: '固态硬盘', percentage_used: 3, power_on_hours: 1207 },
        }
      }
      return {
        status: 'manual',
        resultCode: 'reallocated',
        message: '这块机械硬盘出现了 12 个重新分配的扇区，这是硬盘开始老化的迹象。',
        fixer: 'hardware',
        next: '尽快把重要的文件备份到别的硬盘或网盘，然后考虑换一块固态硬盘。',
        links: ['tool:system.hardware-info'],
        facts: {
          model: 'WDC WD10EZEX-08WN4A0',
          media: '机械硬盘',
          reallocated_sectors: 12,
          pending_sectors: 0,
          power_on_hours: 21873,
          temperature_c: 41,
          smart: { attributes: [{ id: 5, raw: 12 }, { id: 197, raw: 0 }] },
        },
      }
    },
  },
  'hardware.battery': {
    title: '电池健康',
    evaluate: () => ({ status: 'na', resultCode: 'no-battery', message: '这台电脑没有电池。' }),
  },
  'system.recent-bsod': {
    title: '最近的蓝屏',
    evaluate: () => ({ status: 'ok', message: '最近 30 天没有蓝屏记录。', facts: { days: 30, bugchecks: 0 } }),
  },
  'system.reliability': {
    title: '程序崩溃记录',
    evaluate: () => {
      if (DEMO_ALL_OK) return { status: 'ok', message: '最近 7 天没有程序崩溃。', facts: { crashes_7d: 0 } }
      return {
        status: 'advice',
        resultCode: 'crashes',
        message: '最近 7 天有 5 次程序崩溃，大部分是「WPS Office」。',
        fixer: 'helper',
        next: '先把 WPS 更新到最新版；还是经常崩溃的话，把诊断报告发给懂哥看看。',
        links: ['tool:open.reliability'],
        facts: { crashes_7d: 5, top_app: 'WPS Office', update_failures_7d: 0 },
      }
    },
  },
  'security.bitlocker': {
    title: 'BitLocker 加密',
    // 说法和 catalog/checks/security/bitlocker-status.yaml 一样：开着加密只是说明情况，不算要处理（计划书原则 7）
    evaluate: () => {
      if (DEMO_ALL_OK) {
        return { status: 'ok', resultCode: 'off', message: '硬盘没有开启 BitLocker 加密，重装系统时不需要恢复密钥。', facts: { protection: 'off' } }
      }
      return {
        status: 'ok',
        resultCode: 'on',
        message:
          '硬盘分区 C: 已开启 BitLocker 加密，数据更安全，平时用电脑不受影响。只是重装系统、换主板或刷 BIOS 以后，开机可能要求输入 48 位恢复密钥，输不出来就打不开硬盘。',
        next: '现在不用做什么，也不用关掉加密。有空的时候确认一下恢复密钥已经备份：用手机或电脑打开 aka.ms/myrecoverykey，登录你的微软账户就能看到（家庭版的恢复密钥一般就存在当初登录这台电脑的那个微软账户里）。专业版还可以在开始菜单搜索「管理 BitLocker」，点「备份恢复密钥」存到 U 盘或打印出来。家庭版在微软账户里找不到的话，请懂哥帮忙用管理员命令查出恢复密钥抄下来。重装系统、换主板或刷 BIOS 之前，一定要先备份好恢复密钥。',
        facts: { protection: 'on', encrypted_volumes: 'C:', method: 'XTS-AES 128' },
      }
    },
  },
  'boot.secure-boot-cert': {
    title: '安全启动证书',
    evaluate: () => {
      if (DEMO_ALL_OK) return { status: 'ok', message: '安全启动证书已经更新到 2023 版。', facts: { db_2023: true } }
      return {
        status: 'unknown',
        resultCode: null,
        message: '没查出安全启动证书的版本。',
        next: '不影响现在使用，过几天再体检一次就好。',
        error: '读取 UEFI 变量 db 失败：拒绝访问（0x80070005）。',
      }
    },
  },
  'system.device-problems': {
    title: '设备和驱动',
    evaluate: () => {
      if (DEMO_ALL_OK) return { status: 'ok', message: '所有设备都工作正常。', facts: { problem_devices: 0 } }
      return {
        status: 'advice',
        resultCode: 'basic-display',
        message: '显卡驱动没装好，现在用的是「Microsoft 基本显示适配器」。屏幕分辨率可能不对，看视频、玩游戏会卡。',
        fixer: 'system',
        next: '打开「设置 → Windows 更新 → 高级选项 → 可选更新」，看看里面有没有显卡驱动；没有的话，到电脑品牌官网按型号下载。',
        links: ['tool:settings.windows-update', 'tool:open.device-manager'],
        facts: { problem_devices: 1, device: 'Microsoft 基本显示适配器', problem_code: 28 },
      }
    },
  },
  'system.winre': {
    title: '系统恢复环境',
    evaluate: () => ({ status: 'ok', message: '系统恢复环境（WinRE）正常，电脑启动不了时可以进恢复模式。', facts: { enabled: true } }),
  },
  'system.temp-profile': {
    title: '是不是用临时配置文件登录的',
    evaluate: () => ({ status: 'ok', resultCode: 'ok', message: '这次登录用的是你自己的账户配置，不是临时配置文件。', facts: { bak: false, events: 0 } }),
  },
  'display.gpus': {
    title: '显卡：集成显卡和独立显卡',
    evaluate: () => ({
      status: 'ok',
      resultCode: 'hybrid',
      message:
        '这台电脑有集成显卡 Intel(R) UHD Graphics 和独立显卡 NVIDIA GeForce RTX 3050 Laptop GPU。平时 Windows 自己决定用哪块；哪个游戏、软件卡，可以在「图形设置」里把它设成「高性能」，让它用独立显卡。',
      links: ['tool:settings.graphics'],
      facts: { count: 2, desktop: false, display_on: 'integrated' },
    }),
  },
  'system.jpeg-extension': {
    title: '网页上保存的图片的扩展名',
    evaluate: () => {
      if (DEMO_ALL_OK) {
        return { status: 'ok', resultCode: 'jpg', message: 'Windows 给 JPEG 图片登记的扩展名是 .jpg，浏览器存下来的图片就是 .jpg。' }
      }
      return {
        status: 'advice',
        resultCode: 'jfif',
        message: 'Windows 给 JPEG 图片登记的扩展名是 .jfif，所以 Chrome、Edge「图片另存为」出来的是 .jfif，有的软件、网站上传时不认。',
        fixer: 'medkit',
        next: '点下面的「网页上保存的图片是 .jpg，不是 .jfif」改回 .jpg，再把浏览器关掉重新打开。',
        links: ['feature:explorer.jpeg-extension'],
        facts: { where: 'machine', value: '.jfif' },
      }
    },
  },
  'system.managed': {
    title: '单位管理',
    evaluate: () => ({ status: 'ok', message: '这台电脑没有被单位管理（没有加入域，也没有设备管理）。', facts: { domain_joined: false, mdm: false } }),
  },
  'security.win10-esu': {
    title: 'Win10 安全更新登记',
    evaluate: () => ({ status: 'na', resultCode: 'not-win10', message: '这台电脑是 Win11，不需要登记。' }),
  },
  'boot.last-boot-duration': {
    title: '开机用时',
    evaluate: () => ({
      status: DEMO_ALL_OK ? 'ok' : 'advice',
      message: DEMO_ALL_OK ? '最近一次开机用了 28 秒，速度正常。' : '最近一次开机用了 96 秒，有点慢。',
      fixer: 'user',
      next: '可以在下面逐项停用不需要的 Run 启动项。',
    }),
  },
}

function runMockCheck(id: string): CheckResult {
  const check = CHECKS[requireId(id)]
  if (!check) throw `找不到这个检测：${id}`
  const o = check.evaluate()
  return {
    id,
    title: check.title,
    category: id.split('.')[0] ?? 'other',
    status: o.status,
    resultCode: o.resultCode === undefined ? (o.status === 'ok' ? 'ok' : null) : o.resultCode,
    message: o.message,
    fixer: o.fixer ?? null,
    next: o.next ?? null,
    links: o.links ?? [],
    facts: o.facts ?? {},
    error: o.error ?? null,
    durationMs: randomInt(15, 1800),
  }
}

const PROFILES: Record<string, { title: string; checks: string[] }> = {
  healthcheck: {
    title: '系统体检',
    checks: [
      'disk.system-free-space',
      'network.connectivity',
      'network.proxy-dead',
      'system.component-store',
      'update.status',
      'system.pending-reboot',
      'hardware.disk-health',
      'hardware.battery',
      'system.recent-bsod',
      'system.reliability',
      'security.bitlocker',
      'boot.secure-boot-cert',
      'system.device-problems',
      'display.gpus',
      'system.winre',
      'system.managed',
      'security.win10-esu',
    ],
  },
}

// ─────────────────────────── 症状 ───────────────────────────

interface MockSymptom {
  id: string
  title: string
  summary: string | null
  keywords: string[]
  maturity: SymptomDetail['maturity']
  causes: string[]
  guide: string | null
  steps: { check: string; stopOn?: Status[]; fixes: string[] }[]
  links?: string[]
}

const SYMPTOMS: MockSymptom[] = [
  {
    id: 'slow-boot',
    title: '开机慢',
    summary: '开机要等好几分钟，进了桌面还卡半天。',
    keywords: ['开机慢', '启动慢', '开机自启动太多'],
    maturity: 'semi',
    causes: ['开机自动启动的软件太多', '系统装在机械硬盘上', '内存不足'],
    guide: '上面只管理注册表 Run 启动项。任务计划和启动文件夹里的项目，可在任务管理器的「启动应用」里查看。',
    steps: [{ check: 'boot.last-boot-duration', fixes: [] }],
  },
  {
    id: 'network',
    title: '上不了网',
    summary: '网页打不开、微信能用但浏览器不行、Wi-Fi 连上了却没网。',
    keywords: ['没网', '断网', '网断了', '无法上网', '上网', '网络', 'wifi', 'wifi连不上', '无线网', '网页打不开', '浏览器打不开', '微信能用网页打不开'],
    maturity: 'semi',
    causes: [
      '代理设置残留（梯子、加速器卸载后没清干净）',
      '宽带自带的 DNS 不稳定，网址解析不出来',
      'Winsock 网络组件被第三方软件改坏（常说的 LSP 断网）',
      '路由器或宽带本身出了问题，这种情况不是电脑的毛病',
    ],
    guide: [
      '如果上面几步都正常，但还是上不了网：',
      '1. 把路由器和光猫都断电，等 30 秒再插上，等指示灯稳定下来（大约 2 分钟）。',
      '2. 用手机连同一个 Wi-Fi 试试。手机也上不了网，就是宽带的问题，请联系运营商：电信 10000、移动 10086、联通 10010。',
      '3. 如果是校园网、酒店或机场的 Wi-Fi，打开浏览器随便访问一个网址，看会不会跳出登录页面。',
    ].join('\n'),
    steps: [
      // Winsock 放在最前面、查出问题也不停（和真实数据一样）
      { check: 'network.winsock', fixes: ['network.winsock-reset'] },
      { check: 'network.adapter', stopOn: ['manual'], fixes: [] },
      { check: 'network.ip-address', fixes: [] },
      { check: 'network.gateway', fixes: [] },
      { check: 'network.internet', fixes: [] },
      // 刷新 DNS 缓存现在是小工具，从检测结果里的 tool: 链接过去（docs/architecture.md 第 11 节）
      { check: 'network.dns', fixes: ['network.dns-public'] },
      { check: 'network.proxy-dead', fixes: ['network.proxy-off'] },
      { check: 'network.time', fixes: [] },
    ],
  },
  {
    id: 'disk-full',
    title: 'C 盘满了',
    summary: 'C 盘变红、提示磁盘空间不足、更新装不上。',
    keywords: ['c盘满了', 'c盘红了', 'c盘', '磁盘空间不足', '空间不足', '内存不足', '存储空间', '清理', '垃圾', '微信占空间'],
    maturity: 'one-click',
    causes: [
      '临时文件和更新缓存越积越多',
      '休眠文件占了好几 GB',
      '微信、QQ 的聊天文件默认都存在 C 盘',
      '桌面、「下载」文件夹里放了大文件（桌面其实也在 C 盘）',
    ],
    guide: [
      '想从根上解决：',
      '1. 打开「设置 → 系统 → 存储 → 高级存储设置 → 新内容的保存位置」，把文档、音乐、照片、视频的保存位置改到 D 盘。',
      '2. 在微信里打开「设置 → 文件管理」，把文件保存位置改到 D 盘；QQ 在「设置 → 存储管理」里改。',
      '3. 桌面上别放大文件。在「此电脑」里右键「桌面」→「属性」→「位置」，可以把整个桌面搬到 D 盘。',
    ].join('\n'),
    steps: [
      { check: 'disk.system-free-space', fixes: ['disk.cleanup-temp'] },
      { check: 'disk.hiberfile', fixes: ['disk.reduce-hiberfile'] },
      { check: 'disk.wechat-files', fixes: [] },
      { check: 'disk.windows-old', fixes: [] },
    ],
  },
  {
    id: 'printer-share',
    title: '打印机连不上或不打印',
    summary: '打印任务卡住、打印机无响应，或者共享打印机连不上。',
    keywords: ['打印机', '打印', '共享打印机', '打印机连不上', '打印不了'],
    maturity: 'semi',
    causes: [
      'Windows 更新加强了打印安全，老的共享方式被拦住了（0x0000011b）',
      'Win11 22H2 以后，连接旧系统共享的打印机会报 709',
      '网络发现或打印机共享没有开',
      '打印服务（Print Spooler）没有运行',
    ],
    guide: [
      '先分清你是哪一边：',
      '· 主机：打印机用 USB 线连在这台电脑上，共享给别人用。',
      '· 连接的电脑：要去连别人共享出来的打印机。',
      '',
      '在主机上：',
      '1. 打开「设置 → 蓝牙和其他设备 → 打印机和扫描仪」，点开打印机，选「打印机属性 → 共享」，勾选「共享这台打印机」。',
      '2. 记下这台电脑的名字：「设置 → 系统 → 系统信息」里的「设备名称」。',
      '',
      '在连接的电脑上：',
      '1. 按 Win + R，输入两个反斜杠加主机的名字，例如 \\\\DESKTOP-ABC，然后回车。',
      '2. 双击共享的打印机，等它装好驱动。',
      '',
      '最省心的办法：如果打印机本身有网口或 Wi-Fi，直接按 IP 地址添加它（「添加设备 → 手动添加 → 使用 IP 地址或主机名添加打印机」），就不用再折腾共享了。',
    ].join('\n'),
    steps: [{ check: 'printer.spooler', fixes: [] }],
  },
  {
    id: 'printer-709', title: '连接共享打印机报 0x00000709',
    summary: '这台电脑连接另一台电脑共享的打印机时显示 0x00000709。',
    keywords: ['打印机709', '0x00000709', '709打印机'], maturity: 'semi',
    causes: ['共享主机或防火墙挡住 RPC 通信', '旧主机与新版 Windows 的连接方式不兼容'],
    guide: '请在连接别人的打印机的电脑上检查。先更新两边系统和驱动；打印机能联网时，优先直接按 IP 添加。',
    steps: [
      { check: 'printer.spooler', stopOn: ['advice', 'manual'], fixes: [] },
      { check: 'printer.rpc-named-pipes', fixes: ['printer.rpc-named-pipes'] },
    ],
  },
  {
    id: 'printer-11b', title: '共享打印机报 0x0000011b',
    summary: '别的电脑连接这台电脑共享的打印机时报 0x0000011b。',
    keywords: ['打印机11b', '0x0000011b', '11b打印机'], maturity: 'semi',
    causes: ['两边的打印 RPC 保护要求不一致', '系统更新、驱动或共享权限有问题'],
    guide: '请在共享打印机的主机上检查。先更新两边系统和驱动，优先让每台电脑直接按打印机 IP 添加。兼容设置会降低安全性。',
    steps: [
      { check: 'printer.spooler', stopOn: ['advice', 'manual'], fixes: [] },
      { check: 'printer.rpc-privacy', stopOn: ['manual'], fixes: ['printer.rpc-privacy-compat'] },
    ],
  },
  {
    id: 'app-cannot-run', title: '双击程序提示「此应用无法在你的电脑上运行」',
    summary: '打开刚下载的软件、安装包时弹出「此应用无法在你的电脑上运行。若要找到适用于你的电脑的版本，请咨询软件发布者」。',
    keywords: ['此应用无法在你的电脑上运行', '请咨询软件发布者', '不是有效的 win32 应用程序'], maturity: 'guide',
    causes: ['下载的版本和电脑对不上：ARM64 版装在普通电脑上，或者 64 位的程序装在 32 位的 Windows 上', '文件没下载完整，或者下载下来的其实是网页、压缩包'],
    guide: '先用上面的「看看这个程序能不能在这台电脑上运行」选一下那个程序文件，它会说出是哪一种问题、该下载哪个版本。',
    steps: [],
  },
  {
    id: 'file-in-use', title: '删不掉、改不了名：提示「文件已在另一程序中打开」',
    summary: '删除、重命名、移动文件时提示「操作无法完成，因为文件已在另一程序中打开」。',
    keywords: ['文件已在另一程序中打开', '文件被占用', '删不掉文件'], maturity: 'guide',
    causes: ['文件还开在某个软件里，或者软件关了窗口却还在后台运行', '资源管理器自己在用：缩略图、预览窗格'],
    guide: '用上面的「文件删不掉：是谁占着」选中删不掉的文件，看是哪个程序在用，再照说明处理。',
    steps: [],
  },
  {
    id: 'recycle-bin-corrupted', title: '提示「回收站已损坏」',
    summary: '删文件、打开回收站或者开机时弹出「C:\\ 上的回收站已损坏。是否清空该驱动器上的回收站?」，点了「是」还是一再弹出来。',
    keywords: ['回收站已损坏', '回收站损坏', '是否清空该驱动器上的回收站', '回收站打不开', '回收站清空不了'], maturity: 'semi',
    causes: ['删东西的时候断电、强制关机，回收站里的记录没写完', '移动硬盘没点「安全弹出」就拔掉了，或者盘上有坏道、文件系统出了错'],
    guide: '先点提示框里的「是」；还是一再提示的，用上面的「清空并重建回收站」清空提示里说的那个盘，再重启电脑。',
    steps: [
      { check: 'disk.error-events', fixes: [] },
      { check: 'disk.smart-health', fixes: [] },
    ],
  },
  {
    id: 'temp-profile', title: '提示「你已使用临时配置文件登录」，桌面和文件全没了',
    summary: '开机登录以后右下角提示「你已使用临时配置文件登录」，桌面、文档、浏览器收藏都空了，像是个新账户。',
    keywords: ['临时配置文件', '你已使用临时配置文件登录', '桌面全没了'], maturity: 'semi',
    causes: ['开机时杀毒软件、同步软件正好占着你的配置文件，Windows 没能加载它', 'C 盘满了，配置文件加载不了', '上次关机时断电、强制关机，配置文件损坏了'],
    guide: '先别慌：你原来的文件一般还在 C:\\Users 下面你自己的那个文件夹里。现在别往桌面、文档里存东西；重启电脑一两次，还是这样的请懂哥帮忙。',
    steps: [
      { check: 'system.temp-profile', fixes: [] },
      { check: 'disk.system-free-space', fixes: ['disk.cleanup-temp'] },
      { check: 'disk.error-events', fixes: [] },
    ],
  },
  {
    id: 'screen-rotated', title: '屏幕倒过来了、横过来了',
    summary: '画面上下颠倒了，或者转了 90 度变成竖着的，鼠标也跟着反了方向。',
    keywords: ['屏幕倒过来了', '屏幕旋转了', '显示方向'], maturity: 'guide',
    causes: ['不小心按到了 Ctrl + Alt + 方向键：有的电脑（Intel 显卡）的显卡驱动把它当成转屏的快捷键', '「显示设置」里的「显示方向」被改成了纵向或者翻转'],
    guide: '先同时按 Ctrl + Alt + ↑ 试试；没反应的，在「设置 → 系统 → 屏幕」的「显示方向」里选「横向」，再点「保留更改」。',
    steps: [],
  },
  {
    id: 'deleted-files', title: '误删了文件，怎么找回来',
    summary: '文件不小心删了、清空了回收站、按 Shift + Delete 删的，或者 U 盘、存储卡上的文件删了，想找回来。',
    keywords: ['误删文件', '文件恢复', '回收站清空了'], maturity: 'guide',
    causes: ['删的时候没进回收站：按了 Shift + Delete、在 U 盘和存储卡上删的、文件太大放不进回收站', '回收站被清空了，或者被「清理垃圾」的软件清掉了'],
    guide: '先按 Ctrl + Z 撤销、看回收站、看文件夹属性里的「以前的版本」和网盘的回收站。都找不到的话，别再往那个盘里存东西，用下面的「拼出恢复命令」和微软的 Windows File Recovery 找。',
    steps: [],
  },
  {
    id: 'jfif-images', title: '保存的图片变成了 .jfif，上传不了',
    summary: '在浏览器里「图片另存为」、下载的照片，文件名后面是 .jfif 不是 .jpg，传到网站、发给别的软件时提示格式不对、打不开。',
    keywords: ['jfif', '图片变成jfif', 'jfif转jpg'], maturity: 'semi',
    causes: ['Windows 登记的 JPEG 图片扩展名被改成了 .jfif（Windows 更新会改它），Chrome、Edge 存图片时照它起名'],
    guide: '以后存下来就是 .jpg：上面的检查说扩展名是 .jfif 的，点它下面的修复，再把浏览器关掉重新打开。已经存成 .jfif 的，把文件名最后的 .jfif 改成 .jpg 就能用。',
    steps: [{ check: 'system.jpeg-extension', fixes: ['explorer.jpeg-extension'] }],
  },
  {
    id: 'gpu-not-used', title: '玩游戏卡，没用上独立显卡',
    summary: '明明有独立显卡，玩游戏还是卡、帧数低；任务管理器里游戏用的是集成显卡；或者打开 NVIDIA 控制面板，提示「您当前未使用连接到 NVIDIA GPU 的显示器」。',
    keywords: ['独立显卡', '独显', '玩游戏卡', '显卡切换', '独显直连'], maturity: 'semi',
    causes: ['笔记本在用电池，或者电源模式是「节能」「最佳能效」，独立显卡被限制了', '游戏被分到了集成显卡上，没在「图形设置」里设成「高性能」', '台式机的显示器线插在主板的接口上，画面走的是集成显卡'],
    guide: '笔记本先插上电源再玩；点下面的「图形设置」，把游戏的程序加进来设成「高性能」，关掉游戏重新打开。台式机的显示器线要插在独立显卡的接口上，不要插在主板上。',
    steps: [{ check: 'display.gpus', fixes: [] }],
    links: ['tool:settings.graphics'],
  },
]

function getSymptom(id: string): MockSymptom {
  const s = SYMPTOMS.find((x) => x.id === requireId(id))
  if (!s) throw `找不到这个症状：${id}`
  return s
}

// ─────────────────────────── 修改日志 ───────────────────────────

/** 新的会话在前 */
const sessions: JournalSession[] = []
/** 这次打开小药箱以后的修改算一个会话；第一次真的改了东西时才出现在日志里 */
const currentSessionId = uuid()
let currentSession: JournalSession | null = null
/** 「随机返回一次 drift」只演示一次 */
let randomDriftShown = false
/** Windows 默认 24 小时内只允许建一个还原点 */
let restorePointMade = false

interface EntryOptions {
  error?: string | null
  /** 程序在改的过程中退出了：日志里只有 apply、没有 commit，状态不确定 */
  pending?: boolean
}

/** 和后端的规则一样：没撤销过、改成功了（或者状态不确定），而且功能本身能撤销 */
function canUndo(e: JournalEntryView): boolean {
  const reversible =
    e.feature === 'startup' || e.feature === 'context-menu' || (FEATURES.get(e.feature)?.summary.reversible ?? false)
  return !e.undone && (e.ok || e.pending) && reversible
}

function makeEntry(
  session: JournalSession,
  f: MockFeature,
  target: string,
  before: string,
  after: string,
  timeMs: number,
  opts: EntryOptions = {},
): JournalEntryView {
  const error = opts.error ?? null
  const pending = opts.pending ?? false
  const entry: JournalEntryView = {
    id: uuid(),
    sessionId: session.id,
    time: iso(timeMs),
    feature: f.summary.id,
    featureTitle: f.summary.title,
    target,
    before,
    after,
    ok: error === null && !pending,
    pending,
    undone: false,
    undoneAt: null,
    canUndo: false,
    error,
  }
  entry.canUndo = canUndo(entry)
  session.entries.push(entry)
  return entry
}

/** 失败的那一步本身也会按原值退回（引擎记一条 rollback） */
function markRolledBack(entry: JournalEntryView, timeMs: number): void {
  entry.undone = true
  entry.undoneAt = iso(timeMs)
  entry.canUndo = false
}

/** 预置的历史记录：按时间顺序「执行」，这样设置值和日志对得上 */
function seedJournal(): void {
  for (const f of FEATURE_LIST) {
    for (const c of f.changes) values.set(c.target, c.initial)
  }

  const seed = (session: JournalSession, featureId: string, changeIndex: number, timeMs: number) => {
    const f = getFeature(featureId)
    const c = f.changes[changeIndex]
    if (!c) throw new Error(`示例数据有误：${featureId} 没有第 ${changeIndex} 项改动`)
    const entry = makeEntry(session, f, c.target, valueOf(c.target), c.planned, timeMs)
    if (!f.oneShot) values.set(c.target, c.planned)
    return entry
  }

  const now = Date.now()

  // 较早的一次：6 天前
  const aStart = now - 6 * DAY - 5 * 60 * MINUTE
  const a: JournalSession = { id: uuid(), startedAt: iso(aStart), entries: [] }
  seed(a, 'explorer.classic-context-menu', 0, aStart + 1 * MINUTE)
  seed(a, 'desktop.show-this-pc', 0, aStart + 2 * MINUTE)
  const aligned = seed(a, 'taskbar.align-left', 0, aStart + 3 * MINUTE)
  seed(a, 'network.dns-public', 0, aStart + 12 * MINUTE)
  const hp = getFeature('power.high-performance')
  const hpChange = hp.changes[0]
  if (hpChange) {
    const t = aStart + 14 * MINUTE
    const failed = makeEntry(a, hp, hpChange.target, valueOf(hpChange.target), '没有改动', t, {
      error: hp.failWith ?? '执行失败',
    })
    markRolledBack(failed, t)
  }
  // 任务栏靠左后来被撤销了
  values.set(aligned.target, aligned.before)
  aligned.undone = true
  aligned.undoneAt = iso(aStart + 9 * MINUTE)
  aligned.canUndo = false

  // 较近的一次：2 天前。「关闭推荐广告」改到第二项时小药箱被强行关掉了：
  // 第一项改好了，第二项状态不确定，第三项没来得及改。所以常用设置里会显示「部分开启」。
  const bStart = now - 2 * DAY - 3 * 60 * MINUTE
  const b: JournalSession = { id: uuid(), startedAt: iso(bStart), entries: [] }
  seed(b, 'start.disable-recommendations', 0, bStart + 1 * MINUTE)
  const recs = getFeature('start.disable-recommendations')
  const interrupted = recs.changes[1]
  if (interrupted) {
    makeEntry(b, recs, interrupted.target, valueOf(interrupted.target), '（不确定）', bStart + 1 * MINUTE + 2000, {
      pending: true,
    })
  }
  seed(b, 'network.dns-flush', 0, bStart + 4 * MINUTE)

  sessions.push(b, a)

  // 随机挑较早那次里的一项，假装后来被别的程序改回去了（漂移），
  // 这样「撤销这次的全部修改」时能看到被跳过的项目。
  const candidates = a.entries.filter((e) => canUndo(e) && !e.pending)
  const drifted = candidates[randomInt(0, candidates.length - 1)]
  if (drifted) values.set(drifted.target, drifted.before)
}

seedJournal()

function restorePointNote(f: MockFeature): string {
  if (f.restorePointFails) {
    return '没能创建还原点：这台电脑的「系统还原」没有开启。修改日志照样记下了原来的状态，随时可以恢复。'
  }
  if (restorePointMade) {
    return 'Windows 24 小时内只允许建一个还原点，今天已经建过了，这次没有再建。修改日志照样记下了原来的状态，随时可以恢复。'
  }
  restorePointMade = true
  return `已创建还原点「电脑小药箱：${f.summary.title}之前」。`
}

function findEntry(entryId: string): JournalEntryView {
  for (const s of sessions) {
    const e = s.entries.find((x) => x.id === entryId)
    if (e) return e
  }
  throw `找不到这条修改记录：${entryId}`
}

function ensureSession(): JournalSession {
  if (!currentSession) {
    currentSession = { id: currentSessionId, startedAt: iso(Date.now()), entries: [] }
    sessions.unshift(currentSession)
  }
  return currentSession
}

/** 「别的程序」把值改掉了：改成一个和小药箱写入的值不同的值 */
function simulateDrift(entry: JournalEntryView): void {
  const other = entry.before !== entry.after ? entry.before : '（被别的程序改过）'
  values.set(entry.target, other)
}

function driftResult(entry: JournalEntryView, skipped: boolean): UndoResult {
  const now = valueOf(entry.target)
  return {
    entryId: entry.id,
    ok: false,
    drift: true,
    message: skipped
      ? `「${entry.featureTitle}」后来被改过（现在是 ${now}，不是小药箱改成的 ${entry.after}），已跳过。`
      : `现在的值是 ${now}，不是小药箱当初改成的 ${entry.after}。`,
    error: null,
    reboot: 'none',
  }
}

function undoOne(entry: JournalEntryView, force: boolean, inSession: boolean): UndoResult {
  if (entry.undone) {
    return { entryId: entry.id, ok: false, drift: false, message: '这一项已经恢复过了。', error: null, reboot: 'none' }
  }
  if (!canUndo(entry)) {
    const f = FEATURES.get(entry.feature)
    if (f && !f.summary.reversible) {
      return {
        entryId: entry.id,
        ok: false,
        drift: false,
        message: '这一项不能撤销。',
        error: f.summary.irreversibleReason,
        reboot: 'none',
      }
    }
    return { entryId: entry.id, ok: false, drift: false, message: '这一项当初就没有改成，不需要恢复。', error: null, reboot: 'none' }
  }
  // 状态不确定的记录直接按修改前的值恢复，不核对
  if (!force && !entry.pending) {
    if (valueOf(entry.target) !== entry.after) return driftResult(entry, inSession)
    // 单条撤销时随机来一次漂移，好演示「被改过，是否仍要撤销」
    if (!inSession && !randomDriftShown && Math.random() < 0.5) {
      randomDriftShown = true
      simulateDrift(entry)
      return driftResult(entry, false)
    }
  }
  values.set(entry.target, entry.before)
  entry.undone = true
  entry.undoneAt = iso(Date.now())
  entry.canUndo = false
  let message: string
  if (inSession) message = `已恢复：${entry.featureTitle}`
  else if (entry.pending) message = `已按修改前的记录恢复：${entry.target} 设为 ${entry.before}。`
  else message = `已恢复原状：${entry.target} 改回了 ${entry.before}。`
  // 和执行时一样：恢复以后要重启资源管理器、注销……才看得到变化
  const reboot = entry.feature === 'key-remap' ? 'reboot' : (FEATURES.get(entry.feature)?.summary.reboot ?? 'none')
  return { entryId: entry.id, ok: true, drift: false, message, error: null, reboot }
}

// ─────────────────────────── 诊断报告 ───────────────────────────

const REPORT_STATUS: Record<CheckResult['status'], string> = {
  ok: '正常',
  advice: '建议处理',
  manual: '需要人工',
  unknown: '没查出来',
  na: '不适用',
}

function buildReport(note?: string): string {
  const profile = PROFILES.healthcheck
  const results = (profile?.checks ?? []).map(runMockCheck).filter((r) => r.status !== 'na')
  const order = { manual: 0, advice: 1, unknown: 2, ok: 3, na: 4 } as const
  results.sort((x, y) => order[x.status] - order[y.status])
  const okTitles = results.filter((r) => r.status === 'ok').map((r) => r.title)

  const lines: string[] = [
    '电脑小药箱 诊断报告',
    `生成时间：${localTime(Date.now())}`,
    `小药箱版本：${SYSTEM.appVersion}（数据版本 ${SYSTEM.catalogVersion}）`,
    '',
    ...(note?.trim() ? ['== 我遇到的问题 ==', note.trim(), ''] : []),
    '【这台电脑】',
    `系统：${SYSTEM.osCaption.replace(/^Microsoft\s+/, '')}，版本号 ${SYSTEM.build}`,
    `以管理员身份运行：${SYSTEM.isAdmin ? '是' : '否'}`,
    '用户名：[已隐藏]',
    '电脑名：[已隐藏]',
    'CPU：Intel Core i5-10400（6 核 12 线程）',
    '内存：16 GB',
    '系统盘：WDC WD10EZEX（机械硬盘，1 TB），序列号 [已隐藏]',
    '',
    '【网络】',
    '网卡：以太网（有线，1000 Mbps）',
    'IP 地址：[已隐藏]    MAC 地址：[已隐藏]    Wi-Fi 名称：[已隐藏]',
    `系统代理：${valueOf(T.proxyEnable) === 'DWORD 0' ? '没有开' : '开着，指向 127.0.0.1:7890（没有程序在监听）'}`,
    `DNS：${valueOf(T.dns)}`,
    '',
    '【体检】',
  ]
  for (const r of results) {
    if (r.status === 'ok') continue
    lines.push(`${REPORT_STATUS[r.status]} · ${r.title}：${r.message}`)
    if (r.error) lines.push(`    出错信息：${r.error}`)
  }
  if (okTitles.length > 0) lines.push(`正常 · ${okTitles.join('、')}`)

  lines.push('', '【最近的修改】')
  const recent = sessions.filter((s) => s.entries.length > 0).slice(0, 3)
  if (recent.length === 0) lines.push('没有修改记录。')
  for (const s of recent) {
    lines.push(`${localTime(Date.parse(s.startedAt))} 开始，共 ${s.entries.length} 项：`)
    for (const e of s.entries) {
      if (e.pending) {
        lines.push(`  · ${e.featureTitle}：${e.before} → ？（改到一半程序退出了，状态不确定）${e.undone ? '（已恢复原状）' : ''}`)
        continue
      }
      const state = !e.ok ? '（没有改成）' : e.undone ? '（已恢复原状）' : ''
      lines.push(`  · ${e.featureTitle}：${e.before} → ${e.after}${state}`)
    }
  }

  lines.push('', '——', '本报告已去掉用户名、电脑名、IP / MAC 地址、Wi-Fi 名称和序列号。', '报告只保存在这台电脑上，小药箱不会自动上传。')
  return lines.join('\n')
}

// ─────────────────────────── 小工具 ───────────────────────────
//
// 引擎返回的 ToolResult 已经按数据文件里的 labels 渲染成中文，这里直接写渲染好的样子。
// 和真的脚本一样，不输出序列号、MAC 地址、电脑名、用户名。

type ToolOutcome = Pick<ToolResult, 'status' | 'message'> &
  Partial<Pick<ToolResult, 'resultCode' | 'next' | 'links' | 'sections' | 'error'>>

type ToolInput = Pick<ToolSummary, 'id' | 'title' | 'description' | 'category' | 'group'> &
  Partial<Pick<ToolSummary, 'opens' | 'audience' | 'confirm'>>

interface MockTool {
  summary: ToolSummary
  /** info、action 小工具：跑一次的结果 */
  run?: () => ToolOutcome
  /** open 小工具实际打开的东西（程序名或 ms-settings: 页面），只用来编出错信息 */
  target?: string
  /** 没用管理员身份运行时做不了 */
  requiresAdmin?: boolean
  /** 比一般的命令多等一会儿（毫秒），好看清「正在…」的样子 */
  slowMs?: number
}

function defineTool(summary: ToolInput, rest: Omit<MockTool, 'summary'> = {}): MockTool {
  return { summary: { opens: null, audience: 'everyone', confirm: null, ...summary }, ...rest }
}

function openTool(
  id: string,
  title: string,
  description: string,
  category: string,
  opens: 'program' | 'settings' | 'get-help',
  target: string,
  audience: ToolSummary['audience'] = 'everyone',
): MockTool {
  return defineTool({ id, title, description, category, group: 'open', opens, audience }, { target })
}

function row(label: string, value: string, secret = false, qr = false): ToolRow {
  return { label, value, secret, qr }
}

function hardwareSections(): ToolSection[] {
  const osName = SYSTEM.osCaption.replace(/^Microsoft\s+/, '')
  // 默认场景里显卡驱动没装好、系统盘是一块老化的机械硬盘（和体检结果对得上）
  const gpu: ToolSection = DEMO_ALL_OK
    ? {
        title: '显卡：Intel(R) UHD Graphics 630',
        rows: [row('显存', '共用内存，最多 8 GB'), row('驱动版本', '31.0.101.2127'), row('驱动日期', '2024-02-06')],
      }
    : {
        title: '显卡：Microsoft 基本显示适配器',
        rows: [row('显存', '共用内存'), row('驱动版本', '10.0.26100.1'), row('驱动日期', '2006-06-21')],
      }
  const systemDisk: ToolSection = DEMO_ALL_OK
    ? {
        title: '硬盘：Samsung SSD 980 1TB',
        rows: [row('容量', '932 GB'), row('类型', '固态硬盘'), row('接口', 'NVMe'), row('分区', 'C:、D:')],
      }
    : {
        title: '硬盘：WDC WD10EZEX-08WN4A0',
        rows: [row('容量', '932 GB'), row('类型', '机械硬盘'), row('接口', 'SATA'), row('分区', 'C:、D:')],
      }
  return [
    { title: '电脑', rows: [row('制造商', 'LENOVO'), row('型号', 'ThinkCentre M720t'), row('类型', '台式机')] },
    {
      title: '系统',
      rows: [
        row('名称', osName),
        row('版本', DEMO_WIN10 ? '22H2（19045.6332）' : '24H2（26100.4946）'),
        row('位数', '64 位'),
        row('安装日期', '2024-03-18'),
      ],
    },
    {
      title: '处理器',
      rows: [
        row('型号', 'Intel(R) Core(TM) i5-10400 CPU @ 2.90GHz'),
        row('核心', '6 核 12 线程'),
        row('基准频率', '2.9 GHz'),
      ],
    },
    { title: '主板', rows: [row('制造商', 'LENOVO'), row('型号', '3136'), row('BIOS 版本', 'M1UKT4BA（2023-05-12）')] },
    {
      title: '内存',
      rows: [
        row('总容量', '16 GB'),
        row('插槽', '一共 4 个，用了 2 个'),
        row('内存条', '8 GB DDR4 2666 MHz（Samsung，插在 DIMM1）'),
        row('内存条', '8 GB DDR4 2666 MHz（Kingston，插在 DIMM3）'),
      ],
    },
    gpu,
    systemDisk,
    {
      title: '硬盘：Samsung SSD 870 EVO 500GB',
      rows: [row('容量', '466 GB'), row('类型', '固态硬盘'), row('接口', 'SATA'), row('分区', 'E:')],
    },
    {
      title: '网卡：Realtek PCIe GbE Family Controller',
      rows: [row('类型', '有线'), row('状态', '已连接'), row('速度', '1 Gbps')],
    },
    {
      title: '网卡：Intel(R) Wi-Fi 6 AX201 160MHz',
      rows: [row('类型', '无线'), row('状态', '没有连接')],
    },
  ]
}

const WIFI_SECTIONS: ToolSection[] = [
  {
    title: 'WiFi：我家的WiFi-5G',
    rows: [
      row('密码', 'Lin1990@home', true),
      row('加密方式', 'WPA2 个人'),
      row('自动连接', '是'),
      row('手机扫码连接', 'WIFI:T:WPA;S:我家的WiFi-5G;P:Lin1990@home;;', true, true),
    ],
  },
  {
    title: 'WiFi：CMCC-WEB',
    rows: [
      row('密码', '没有密码（开放的网络，谁都能连）'),
      row('加密方式', '不加密'),
      row('自动连接', '否'),
      row('手机扫码连接', 'WIFI:T:nopass;S:CMCC-WEB;;', true, true),
    ],
  },
  {
    title: 'WiFi：eduroam',
    rows: [
      row('密码', '用个人账号登录，没有统一的 WiFi 密码'),
      row('加密方式', 'WPA2 企业'),
      row('自动连接', '是'),
    ],
  },
]

/** 刷新 DNS 缓存：第一次清掉的多，之后没多少可清 */
let dnsFlushed = false

const TOOL_LIST: MockTool[] = [
  // ── 看信息 ──
  defineTool(
    {
      id: 'system.hardware-info',
      title: '电脑配置',
      description: '看看这台电脑的处理器、内存、显卡、硬盘都是什么型号。升级内存、买软件、找人帮忙时用得上。',
      category: 'hardware',
      group: 'info',
    },
    {
      run: () => ({
        status: 'ok',
        resultCode: 'ok',
        message: '已经读出这台电脑的配置。',
        sections: hardwareSections(),
      }),
      slowMs: 1200,
    },
  ),
  defineTool(
    {
      id: 'network.wifi-passwords',
      title: '查看 WiFi 密码',
      description: '忘了 WiFi 密码？这里能看到这台电脑连过的 WiFi 和它们的密码，方便给手机或新电脑连上。',
      category: 'network',
      group: 'info',
    },
    {
      run: () => ({
        status: 'ok',
        resultCode: 'ok',
        message: `这台电脑记住了 ${WIFI_SECTIONS.length} 个 WiFi。密码默认遮住，点「显示」才能看到。`,
        sections: WIFI_SECTIONS,
      }),
      requiresAdmin: true,
    },
  ),

  // ── 一键处理 ──
  defineTool(
    {
      id: 'network.flush-dns',
      title: '刷新 DNS 缓存',
      description: '清掉电脑记住的网址解析结果。换了网络、改了路由器以后某些网站打不开时，先试试这个。',
      category: 'network',
      group: 'action',
    },
    {
      run: () => {
        const entries = dnsFlushed ? randomInt(3, 30) : 236
        dnsFlushed = true
        return {
          status: 'ok',
          resultCode: 'done',
          message: `已经刷新了 DNS 缓存（清掉了 ${entries} 条记录）。`,
          next: '网页还是打不开的话，把浏览器关掉再打开试试；还不行，去「上不了网」里一步一步查。',
          links: ['symptom:network'],
        }
      },
      requiresAdmin: true,
    },
  ),
  defineTool(
    {
      id: 'network.open-login',
      title: '打开网络登录页',
      description: '酒店、校园或商场的 WiFi 连上后仍没网时，打开 Windows 使用的联网检测地址，让网络跳转到登录页。',
      category: 'network',
      group: 'action',
    },
    {
      run: () => ({
        status: 'ok', resultCode: 'opened',
        message: '已在默认浏览器中打开联网检测地址。如果跳出了登录页，确认是当前 WiFi 提供方的页面后再登录。',
        next: '登录后回到小药箱，重新运行「上不了网」诊断。',
        links: ['symptom:network'],
      }),
    },
  ),
  defineTool(
    {
      id: 'system.restart-explorer',
      title: '重启资源管理器',
      description: '任务栏点不动、桌面图标不见了、文件夹窗口卡住时，把资源管理器关掉再重新打开，不用重启电脑。',
      category: 'system',
      group: 'action',
      confirm:
        '资源管理器会关掉再重新打开：桌面和任务栏会消失几秒钟，已经打开的文件夹窗口会关掉。正在复制或移动文件的话，先等它做完再点。',
    },
    {
      // 说法和 catalog/tools/system/restart-explorer.yaml 一样
      run: () =>
        DEMO_EXPLORER_FAIL
          ? {
              status: 'advice',
              resultCode: 'not-restarted',
              message: '资源管理器关掉以后，等了 15 秒还没有自己重新打开，所以桌面和任务栏暂时不见了。',
              next: '点下面的按钮（或者按 Ctrl + Shift + Esc）打开任务管理器，点「运行新任务」（Windows 10 在「文件」菜单里），输入 explorer，按回车。「以系统管理权限创建此任务」不要勾。',
              links: ['tool:open.task-manager'],
            }
          : {
              status: 'ok',
              resultCode: 'restarted',
              message: '资源管理器已经重新打开了。桌面和任务栏要是还没出来，稍等几秒钟。',
            },
      slowMs: 1500,
    },
  ),
  defineTool(
    {
      id: 'system.sync-time',
      title: '同步系统时间',
      description: '请求 Windows 从已配置的时间源校准时钟，不改时区或时间服务器。',
      category: 'system', group: 'action',
    },
    {
      run: () => ({
        status: 'ok', resultCode: 'requested',
        message: '已请求 Windows 同步时间。请再看任务栏时钟和时区是否正确。',
        next: '如果每次断电后时间又跳回过去，可能需要更换主板电池。',
        links: ['tool:settings.date-time'],
      }),
      requiresAdmin: true,
    },
  ),
  defineTool(
    {
      // 说法和 catalog/tools/system/file-recovery.yaml 一样（演示：还没装，打开了商店页面）
      id: 'system.file-recovery',
      title: '打开微软的「Windows File Recovery」（找回误删的文件）',
      description: '微软免费的误删文件恢复工具，清空了回收站、按 Shift + Delete 删的、U 盘上删的文件，有机会找回来。没装的会打开 Microsoft Store 里它的页面。',
      category: 'disk', group: 'action',
    },
    {
      run: () => ({
        status: 'advice', resultCode: 'store',
        message: '这台电脑上还没有「Windows File Recovery」，已经打开了 Microsoft Store 里它的页面。',
        next: '点「获取」（或者「安装」）装好以后，再点一次这个小工具。它要 Windows 10 2004 或者更新的版本。',
      }),
    },
  ),
  defineTool(
    {
      id: 'printer.restart-spooler', title: '重启打印服务',
      description: '打印任务卡住时重启 Print Spooler，不改启动类型、不清空队列。',
      category: 'printer', group: 'action',
      confirm: '正在打印的任务会暂时中断，之后可能需要重新提交。现在重启打印服务吗？',
    },
    { run: () => ({ status: 'ok', resultCode: 'restarted', message: '打印服务已重启。', links: ['symptom:printer-share'] }), requiresAdmin: true },
  ),

  // ── 打开系统工具 ──
  openTool(
    'open.task-manager',
    '任务管理器',
    '看看哪个程序占着 CPU 和内存，结束卡死的程序，管理开机自动启动的软件。',
    'system',
    'program',
    'Taskmgr.exe',
  ),
  openTool(
    'open.device-manager',
    '设备管理器',
    '看看有没有设备带黄色感叹号（驱动没装好），网卡、声卡、显卡是不是都认到了。',
    'hardware',
    'program',
    'devmgmt.msc',
  ),
  openTool(
    'open.disk-cleanup',
    '磁盘清理',
    'Windows 自带的清理工具，可以删掉临时文件、回收站和旧的更新文件。',
    'disk',
    'program',
    'cleanmgr.exe',
  ),
  openTool(
    'open.system-restore',
    '系统还原',
    '把系统退回到以前某个还原点的样子。文档和照片不受影响，但之后装的软件可能要重新装。',
    'system',
    'program',
    'rstrui.exe',
  ),
  openTool(
    'open.reliability',
    '可靠性历史记录',
    '按日期列出程序崩溃、更新失败这些问题，看看电脑是从哪天开始不对劲的。',
    'system',
    'program',
    'perfmon.exe',
  ),
  openTool(
    'open.memory-diagnostic',
    'Windows 内存诊断',
    '检查内存条有没有问题。要重启电脑才能检查，大约要 10 到 20 分钟。',
    'hardware',
    'program',
    'MdSched.exe',
  ),
  openTool(
    'open.disk-management',
    '磁盘管理',
    '查看和调整硬盘分区、给新硬盘分区。操作不当会丢数据，不熟悉的话别动里面的东西。',
    'disk',
    'program',
    'diskmgmt.msc',
    'helper',
  ),
  openTool(
    'open.system-information',
    '系统信息',
    '最全的硬件和系统信息。懂哥帮你远程查问题时，可能会让你打开它看看。',
    'system',
    'program',
    'msinfo32.exe',
    'helper',
  ),

  // ── 打开「设置」里的页面 ──
  openTool(
    'settings.windows-update',
    'Windows 更新',
    '检查和安装更新，看看有没有装好了、等着重启的更新。',
    'settings',
    'settings',
    'ms-settings:windowsupdate',
  ),
  openTool('settings.storage', '存储', '看看 C 盘被什么占满了，开启存储感知自动清理。', 'settings', 'settings', 'ms-settings:storagesense'),
  openTool(
    'settings.storage-sense',
    '存储感知',
    '打开「设置」里的存储感知：打开以后，系统会定期自动清理用不上的临时文件，C 盘不容易再满。注意「回收站」那一项：默认会删掉放了 30 天以上的文件，回收站里还有想要的东西，就选「从不」；「下载」文件夹那一项保持「从不」。',
    'settings',
    'settings',
    'ms-settings:storagepolicies',
  ),
  openTool(
    'settings.apps',
    '已安装的应用',
    '卸载不用的软件，看看每个软件占了多少空间。',
    'settings',
    'settings',
    'ms-settings:appsfeatures',
  ),
  openTool(
    'settings.startup-apps',
    'Windows 启动应用设置',
    '管理小药箱列表以外的软件，关闭或重新开启开机自启。',
    'settings',
    'settings',
    'ms-settings:startupapps',
  ),
  openTool(
    'settings.default-apps',
    '默认应用',
    '设置用哪个浏览器打开网页、用哪个软件看图片和视频。',
    'settings',
    'settings',
    'ms-settings:defaultapps',
  ),
  openTool(
    'settings.network',
    '网络和 Internet',
    '看看网络连上了没有，设置 WiFi 和代理，网络实在不行时可以在这里重置网络。',
    'settings',
    'settings',
    'ms-settings:network-status',
  ),
  openTool(
    'settings.printers',
    '打印机和扫描仪',
    '添加或删除打印机，设置默认打印机，看看卡在队列里的打印任务。',
    'settings',
    'settings',
    'ms-settings:printers',
  ),
  openTool('open.services', '服务管理', '查看 Print Spooler 等服务的状态。', 'system', 'program', 'services.msc'),
  openTool('settings.sound', '声音', '选择从哪个喇叭或耳机出声、用哪个麦克风，调整音量。', 'settings', 'settings', 'ms-settings:sound'),
  openTool('troubleshoot.audio', '声音疑难解答', '电脑没声音、声音断断续续时用：微软「获取帮助」里的自动疑难解答，能修的直接修。', 'audio', 'get-help', 'AudioTroubleshooter'),
  openTool('troubleshoot.network', '网络和 Internet 疑难解答', '上不了网时用：检查网卡、WiFi、IP 地址和 DNS，能修的直接修。', 'network', 'get-help', 'NetworkAndInternetTroubleshooter'),
  openTool('troubleshoot.printer', '打印机疑难解答', '打印机不打印、显示脱机时用：检查打印服务、打印队列和驱动。', 'printer', 'get-help', 'PrinterTroubleshooter'),
  openTool('troubleshoot.windows-update', 'Windows 更新疑难解答', 'Windows 更新装不上、一直失败时用。', 'update', 'get-help', 'WUTroubleshooter'),
  openTool(
    'settings.date-time',
    '日期和时间',
    '打开「自动设置时间」、马上同步一次，或者改时区。电脑时间不对时，网页会报证书错误。',
    'settings',
    'settings',
    'ms-settings:dateandtime',
  ),
  openTool(
    'settings.graphics',
    '图形设置（让游戏用独立显卡）',
    '打开「设置」里的图形设置（Windows 11 叫「显示卡」），给游戏、剪辑软件选「高性能」，让它用独立显卡；选「节能」就用集成显卡、更省电。改完要把这个程序关掉重新打开才生效。',
    'settings',
    'settings',
    'ms-settings:display-advancedgraphics',
  ),
]

const TOOLS = new Map(TOOL_LIST.map((t) => [t.summary.id, t]))

function getTool(id: string): MockTool {
  const t = TOOLS.get(requireId(id))
  if (!t) throw `找不到小工具：${id}`
  return t
}

function runMockTool(id: string): ToolResult {
  const t = getTool(id)
  // 说法都和引擎一样
  if (!t.run) throw `「${t.summary.title}」不用运行，直接打开就行`
  if (DEMO_TOOL_FAIL) throw `「${t.summary.title}」的脚本运行超时（30 秒），已经停下来了。`
  // 和检测一样：没有管理员权限时不跑脚本，结果算「没查出来」
  const o: ToolOutcome =
    t.requiresAdmin && !SYSTEM.isAdmin
      ? {
          status: 'unknown',
          resultCode: null,
          message: t.summary.group === 'info' ? '没能读出来。' : '没能完成。',
          error: '这一项需要管理员权限',
        }
      : t.run()
  return {
    id,
    title: t.summary.title,
    status: o.status,
    resultCode: o.resultCode === undefined ? (o.status === 'ok' ? 'ok' : null) : o.resultCode,
    message: o.message,
    next: o.next ?? null,
    links: o.links ?? [],
    sections: o.sections ?? [],
    error: o.error ?? null,
    durationMs: randomInt(200, 2400),
  }
}

function openMockTool(id: string): null {
  const t = getTool(id)
  const title = t.summary.title
  if (t.summary.group !== 'open') throw `「${title}」不是用来打开的工具`
  if (DEMO_OPEN_FAIL) {
    // ShellExecuteEx 失败，GetLastError 是 2（说法和引擎一样）
    if (t.summary.opens === 'get-help') {
      throw `这台电脑上没有「获取帮助」应用（精简过的系统、服务器版常常没有），打不开微软的「${title}」。可以在 Microsoft Store 里搜「获取帮助」装上再试，或者到「设置」的「疑难解答」页里找。`
    }
    throw t.summary.opens === 'settings'
      ? `没能打开「${title}」：系统没有响应（错误代码 2）。可以点开始菜单里的齿轮图标，自己打开「设置」找这一项`
      : `这台电脑上没有「${title}」（找不到 ${t.target ?? id}），可能被精简系统删掉了。`
  }
  return null
}

// ─────────────────────────── 命令 ───────────────────────────

type Handlers = { [K in CommandName]: (args: CommandArgs<K>) => CommandResult<K> }

// 开机启动项：开关的值用和后端修改日志一样的说法
const STARTUP_ON = '开机自动启动（默认）'
const STARTUP_OFF = '不自动启动（已停用）'
const startupMock: StartupItem[] = [
  {
    id: 'user-chat', source: 'user-run', name: 'WeChat', title: '微信', program: 'WeChat.exe',
    path: 'C:\\Program Files\\Tencent\\WeChat\\WeChat.exe', exists: true,
    publisher: 'Tencent Technology(Shenzhen) Company Limited', signature: 'valid', location: '当前用户（注册表）',
    enabled: true, advice: 'can-disable', reason: '不需要一开机就用的话，可以停用；软件本身还在，想用时照样能打开。',
  },
  {
    id: 'user-cloud', source: 'user-folder', name: '网盘同步.lnk', title: '网盘同步', program: 'cloud.exe',
    path: 'C:\\Program Files\\Cloud\\cloud.exe', exists: true, publisher: null, signature: 'unsigned',
    location: '当前用户（启动文件夹）', enabled: true, advice: 'can-disable',
    reason: '不需要一开机就用的话，可以停用；软件本身还在，想用时照样能打开。',
  },
  {
    id: 'all-audio', source: 'machine-run', name: 'RtkAudUService', title: 'Realtek HD Audio Universal Service',
    program: 'RtkAudUService64.exe', path: 'C:\\Windows\\System32\\RtkAudUService64.exe', exists: true,
    publisher: 'Realtek Semiconductor Corp.', signature: 'valid', location: '所有用户（注册表）', enabled: true,
    advice: 'keep', reason: '硬件驱动或电脑厂商的功能（触控板、声音、快捷键这类），停用后可能不好用，建议保持原样。',
  },
  {
    id: 'all-old', source: 'machine-run32', name: 'OldUpdater', title: 'OldUpdater', program: 'updater.exe',
    path: 'C:\\Program Files (x86)\\Old\\updater.exe', exists: false, publisher: null, signature: 'unknown',
    location: '所有用户（注册表）', enabled: false, advice: 'can-disable',
    reason: '找不到它要启动的程序，软件可能已经卸载了，停用它没有坏处。',
  },
]
for (const item of startupMock) values.set(`startup:${item.id}`, item.enabled ? STARTUP_ON : STARTUP_OFF)

// ── 右键菜单里软件加的项目（演示）──
const MENU_SHOWN = '显示'
const MENU_HIDDEN = '不显示（已拿掉）'
// 「新建」菜单（演示）：WPS、XMind 加的和 Windows 自带的几项
const NEW_MENU_SHOWN = '显示'
const NEW_MENU_HIDDEN = '不显示（已关掉）'
const newMenuMock: NewMenuItem[] = [
  { id: '.docx', title: 'DOCX 文档', ext: '.docx', windowsOwn: false, location: '所有用户', visible: true },
  { id: '.xlsx', title: 'XLSX 工作表', ext: '.xlsx', windowsOwn: false, location: '所有用户', visible: true },
  { id: '.pptx', title: 'PPTX 演示文稿', ext: '.pptx', windowsOwn: false, location: '所有用户', visible: true },
  { id: '.xmind', title: 'XMind 思维导图', ext: '.xmind', windowsOwn: false, location: '当前用户', visible: true },
  { id: '.bmp', title: 'BMP 图像', ext: '.bmp', windowsOwn: true, location: '所有用户', visible: true },
  { id: '.contact', title: '联系人', ext: '.contact', windowsOwn: true, location: '所有用户', visible: true },
  { id: '.txt', title: '文本文档', ext: '.txt', windowsOwn: true, location: '所有用户', visible: true },
  { id: '.zip', title: '压缩(zipped)文件夹', ext: '.zip', windowsOwn: true, location: '所有用户', visible: true },
].sort((a, b) => (a.title.toLowerCase() < b.title.toLowerCase() ? -1 : 1)) // 和引擎一样按名字排
// 资源管理器里软件加的图标（演示）：网盘、WPS 云文档，和 Windows 自带的 OneDrive、图库
const PLACE_SHOWN = '显示'
const PLACE_HIDDEN = '不显示（已隐藏）'
const shellPlacesMock: ShellPlaceItem[] = (
  [
    { id: '{018D5C66-4533-4307-9B53-224DE2ED1FE6}', title: 'OneDrive - Personal', places: ['nav'], windowsOwn: true, visible: true, note: '' },
    { id: '{E88865EA-0E1C-4E20-9AA6-EDCD0212C87C}', title: '图库', places: ['nav'], windowsOwn: true, visible: true, note: '' },
    { id: '{6D5C1F2A-0B1E-4C5A-9F3E-2A7B8C9D0E1F}', title: '坚果云', places: ['nav'], windowsOwn: false, visible: true, note: '' },
    { id: '{5FCD4425-CA3A-48F4-A57C-B8A75C32ACB1}', title: 'WPS云文档', places: ['nav', 'pc'], windowsOwn: false, visible: true, note: '' },
    { id: '{679F137C-3162-45DA-BE3C-2F9C3D093F64}', title: '百度网盘', places: ['pc'], windowsOwn: false, visible: true, note: '' },
  ] satisfies ShellPlaceItem[]
).sort((a, b) => (a.title.toLowerCase() < b.title.toLowerCase() ? -1 : 1)) // 和引擎一样按名字排
// 改键（演示）：能选的键挑了常用的一部分（引擎里是整个键盘）。现在的设置按修改日志里的说法存，撤销时原样放回去
const KEYMAP_TARGET = 'HKLM\\SYSTEM\\CurrentControlSet\\Control\\Keyboard Layout\\Scancode Map'
const KEYMAP_NONE = '没有改键（Windows 默认）'
const keymapKey = (id: string, label: string): KeyOption => ({ id, label, targetOnly: false })
const keymapMedia = (id: string, label: string): KeyOption => ({ id, label, targetOnly: true })
const keymapKeys: KeyOption[] = [
  keymapKey('Escape', 'Esc'),
  ...Array.from({ length: 12 }, (_, i) => keymapKey(`F${i + 1}`, `F${i + 1}`)),
  keymapKey('Backspace', 'Backspace（退格）'),
  keymapKey('Tab', 'Tab'),
  keymapKey('CapsLock', 'Caps Lock（大写锁定）'),
  keymapKey('Enter', 'Enter（回车）'),
  keymapKey('ShiftLeft', '左 Shift'),
  keymapKey('ShiftRight', '右 Shift'),
  keymapKey('ControlLeft', '左 Ctrl'),
  keymapKey('MetaLeft', '左 Win'),
  keymapKey('AltLeft', '左 Alt'),
  keymapKey('Space', '空格'),
  keymapKey('AltRight', '右 Alt'),
  keymapKey('MetaRight', '右 Win'),
  keymapKey('ContextMenu', '菜单键（右 Ctrl 左边）'),
  keymapKey('ControlRight', '右 Ctrl'),
  keymapKey('PrintScreen', 'Print Screen（截屏）'),
  keymapKey('ScrollLock', 'Scroll Lock'),
  keymapKey('Insert', 'Insert（插入）'),
  keymapKey('Delete', 'Delete（删除）'),
  keymapKey('Home', 'Home'),
  keymapKey('End', 'End'),
  keymapKey('PageUp', 'Page Up'),
  keymapKey('PageDown', 'Page Down'),
  keymapKey('NumLock', 'Num Lock（数字锁定）'),
  keymapMedia('AudioVolumeMute', '静音'),
  keymapMedia('AudioVolumeDown', '音量减小'),
  keymapMedia('AudioVolumeUp', '音量增大'),
  keymapMedia('MediaPlayPause', '播放 / 暂停'),
  keymapMedia('MediaTrackPrevious', '上一首'),
  keymapMedia('MediaTrackNext', '下一首'),
]
/** 修改日志里的说法 → 那时的改键 */
const keymapStates = new Map<string, KeyMappingInput[]>([[KEYMAP_NONE, []]])

function keymapText(m: KeyMappingInput): string {
  const name = (id: string): string => keymapKeys.find((k) => k.id === id)?.label ?? id
  return `${name(m.from)} → ${m.to === null ? '不起作用' : name(m.to)}`
}

function keymapCurrent(): KeyMappingInput[] {
  return keymapStates.get(values.get(KEYMAP_TARGET) ?? KEYMAP_NONE) ?? []
}

/** 和引擎一样检查：键要在名单里、多媒体键只能当目标、不能改成自己、同一个键不能改两次 */
function keymapResolve(mappings: KeyMappingInput[]): KeyMappingInput[] {
  const seen = new Set<string>()
  return mappings.map((m) => {
    const from = keymapKeys.find((k) => k.id === m.from)
    if (!from) throw `认不出这个键：${m.from}`
    if (from.targetOnly) throw `「${from.label}」只能当「变成」的键，不能改它本身。`
    const to = m.to ?? null
    if (to !== null && !keymapKeys.some((k) => k.id === to)) throw `认不出这个键：${to}`
    if (to === m.from) throw `「${from.label}」改成它自己，等于没改。`
    if (seen.has(m.from)) throw `「${from.label}」改了两次，只能留一个。`
    seen.add(m.from)
    return { from: m.from, to }
  })
}
const contextMenuMock: ContextMenuItem[] = [
  {
    id: 'menu-rar', kind: 'extension', title: 'WinRAR shell extension', program: 'rarext.dll',
    path: 'C:\\Program Files\\WinRAR\\rarext.dll', exists: true, publisher: 'win.rar GmbH', signature: 'valid',
    scopes: ['文件', '文件夹', '磁盘'], location: '', visible: true, shiftOnly: false,
    note: '同一个软件在右键菜单里的几处会一起拿掉。',
  },
  {
    id: 'menu-cloud', kind: 'extension', title: '上传到网盘', program: 'CloudShellExt.dll',
    path: 'C:\\Program Files\\Cloud\\CloudShellExt.dll', exists: true, publisher: null, signature: 'unsigned',
    scopes: ['文件', '文件夹'], location: '', visible: true, shiftOnly: false, note: '',
  },
  {
    id: 'menu-git', kind: 'command', title: 'Open Git Bash here', program: 'git-bash.exe',
    path: 'C:\\Program Files\\Git\\git-bash.exe', exists: true, publisher: 'Johannes Schindelin', signature: 'valid',
    scopes: ['文件夹', '文件夹空白处'], location: '所有用户', visible: true, shiftOnly: false, note: '',
  },
  {
    id: 'menu-code', kind: 'app', title: 'Visual Studio Code', program: 'code_explorer_command.dll',
    path: 'C:\\Program Files\\Microsoft VS Code\\code_explorer_command.dll', exists: true, publisher: 'Microsoft Corporation',
    signature: 'valid', scopes: ['文件', '文件夹'], location: '', visible: true, shiftOnly: false, note: '',
  },
  {
    id: 'menu-old', kind: 'extension', title: 'OldCloudExt', program: '', path: '', exists: false, publisher: null,
    signature: 'unknown', scopes: ['文件'], location: '', visible: true, shiftOnly: false,
    note: '这个扩展已经没有登记了，多半是软件卸载后留下的，拿掉没有坏处。',
  },
]
for (const item of contextMenuMock) values.set(`menu:${item.id}`, item.visible ? MENU_SHOWN : MENU_HIDDEN)

const handlers: Handlers = {
  system_info: () => SYSTEM,

  catalog_summary: (): CatalogSummary => ({
    profiles: Object.entries(PROFILES).map(([id, p]) => ({ id, title: p.title, checkCount: p.checks.length })),
    symptoms: SYMPTOMS.map((s) => ({ id: s.id, title: s.title, summary: s.summary, keywords: s.keywords, maturity: s.maturity })),
    features: FEATURE_LIST.map((f) => f.summary),
    tools: TOOL_LIST.map((t) => t.summary),
  }),

  symptom_detail: ({ id }): SymptomDetail => {
    const s = getSymptom(id)
    return {
      id: s.id,
      title: s.title,
      summary: s.summary,
      keywords: s.keywords,
      maturity: s.maturity,
      causes: s.causes,
      guide: s.guide,
      steps: s.steps.map((step) => ({
        check: step.check,
        checkTitle: CHECKS[step.check]?.title ?? step.check,
        stopOn: step.stopOn ?? [],
        fixes: step.fixes.map((fid) => getFeature(fid).summary),
      })),
      links: s.links ?? [],
    }
  },

  run_profile: ({ id }) => {
    const profile = PROFILES[requireId(id)]
    if (!profile) throw `找不到这个检测清单：${id}`
    return profile.checks.map(runMockCheck)
  },

  run_check: ({ id }) => runMockCheck(id),

  feature_detect: ({ id }): FeatureState => {
    const f = getFeature(id)
    if (f.detectError) return { id, state: 'unknown', details: [], error: f.detectError }
    const state = detectKind(f)
    const details =
      state === 'partial'
        ? f.changes.map((c) => `${c.label ?? c.target}：${valueOf(c.target) === c.planned ? '已改好' : '还没改'}`)
        : []
    return { id, state, details, error: null }
  },

  feature_preview: ({ id }): Preview => {
    const f = getFeature(id)
    return {
      feature: f.summary,
      changes: f.changes.map((c) => ({ target: c.target, current: valueOf(c.target), planned: c.planned })),
      // 本来就是好的功能不用改，也就不建还原点
      willCreateRestorePoint: f.summary.risk !== 'safe' && pendingChanges(f).length > 0,
      // 和引擎一样：用不了的功能在注意事项最前面写上原因
      notes: f.summary.applicable ? f.notes : [`不能执行：${f.summary.notApplicableReason ?? ''}`, ...f.notes],
    }
  },

  feature_apply: ({ id }): ApplyResult => {
    const f = getFeature(id)
    // 和引擎一样：这台电脑用不了的功能直接报错（Tauri 里是 reject 一个字符串）
    if (!f.summary.applicable) throw `这项不适用于这台电脑：${f.summary.notApplicableReason ?? ''}`
    const title = f.summary.title
    const todo = pendingChanges(f)

    // 已经是目标状态的改动不动，也不写日志；整个功能本来就是好的，直接返回「不用改」
    if (todo.length === 0) {
      return {
        feature: id,
        sessionId: currentSessionId,
        entryIds: [],
        ok: true,
        verified: 'applied',
        message: '这一项本来就是好的，不用改。',
        reboot: 'none',
        notes: [],
        error: null,
      }
    }

    const session = ensureSession()
    const now = Date.now()
    const notes: string[] = []
    if (f.summary.risk !== 'safe') notes.push(restorePointNote(f))

    if (f.failWith) {
      const c = todo[0]
      const entryIds: string[] = []
      if (c) {
        const failed = makeEntry(session, f, c.target, valueOf(c.target), '没有改动', now, { error: f.failWith })
        markRolledBack(failed, now)
        entryIds.push(failed.id)
      }
      return {
        feature: id,
        sessionId: session.id,
        entryIds,
        ok: false,
        verified: f.detectError ? 'unknown' : detectKind(f),
        message: `没有改成「${title}」，已经退回原样，这台电脑上的设置没有变。`,
        reboot: 'none',
        notes,
        error: f.failWith,
      }
    }

    const entryIds = todo.map((c) => {
      const entry = makeEntry(session, f, c.target, valueOf(c.target), c.planned, now)
      if (!f.oneShot) values.set(c.target, c.planned)
      return entry.id
    })
    const verified: FeatureStateKind = f.verifyAs ?? (f.oneShot ? 'applied' : detectKind(f))
    return {
      feature: id,
      sessionId: session.id,
      entryIds,
      ok: true,
      verified,
      message:
        verified === 'applied'
          ? `「${title}」已经生效。`
          : '设置已经写进去了，但要重启电脑以后才能确认有没有生效。',
      reboot: f.summary.reboot,
      notes,
      error: null,
    }
  },

  // canUndo 每次现算，和后端一样由「后端」决定显不显示「恢复原状」
  journal_list: () => sessions.map((s) => ({ ...s, entries: s.entries.map((e) => ({ ...e, canUndo: canUndo(e) })) })),

  journal_undo: ({ entryId, force }) => undoOne(findEntry(entryId), force, false),

  // 同一会话里还能撤销的记录按倒序逐条撤销；漂移或出错的跳过，单独报告
  journal_undo_session: ({ sessionId }) => {
    const session = sessions.find((s) => s.id === sessionId)
    if (!session) throw `找不到这次修改：${sessionId}`
    return [...session.entries]
      .reverse()
      .filter(canUndo)
      .map((e) => undoOne(e, false, true))
  },

  report_generate: ({ note }) => buildReport(note),

  tool_run: ({ id }) => runMockTool(id),

  tool_open: ({ id }) => openMockTool(id),

  startup_list: () => startupMock.map((item) => ({ ...item, enabled: values.get(`startup:${item.id}`) !== STARTUP_OFF })),

  startup_set: ({ id, enabled }) => {
    const item = startupMock.find((x) => x.id === id)
    if (!item) throw '这个启动项不在刚才的列表里了，请刷新一下再试。'
    const target = `startup:${item.id}`
    const before = values.get(target) ?? STARTUP_ON
    const result: ApplyResult = {
      feature: 'startup', sessionId: ensureSession().id, entryIds: [], ok: true, verified: 'applied',
      message: '本来就是这样，不用改。', reboot: 'none', notes: [], error: null,
    }
    if ((before !== STARTUP_OFF) === enabled) return result
    const session = ensureSession()
    const entry: JournalEntryView = {
      id: uuid(), sessionId: session.id, time: iso(Date.now()), feature: 'startup',
      featureTitle: `开机启动项：${item.name}`, target, before, after: enabled ? '开机自动启动' : STARTUP_OFF,
      ok: true, pending: false, undone: false, undoneAt: null, canUndo: true, error: null,
    }
    session.entries.push(entry)
    values.set(target, entry.after)
    result.entryIds = [entry.id]
    result.message = enabled
      ? '已经恢复：下次开机登录时，它会自动启动。'
      : '已经停用：下次开机登录时，它不会再自动启动。软件本身还在，想用时照样能打开；想改回来，在这里或者任务管理器的「启动应用」里都行。'
    return result
  },
  context_menu_list: () => contextMenuMock.map((item) => ({ ...item, visible: values.get(`menu:${item.id}`) !== MENU_HIDDEN })),

  context_menu_set: ({ id, visible }) => {
    const item = contextMenuMock.find((x) => x.id === id)
    if (!item) throw '这一项不在刚才的列表里了，请刷新一下再试。'
    const target = `menu:${item.id}`
    const before = values.get(target) ?? MENU_SHOWN
    const command = item.kind === 'command'
    const result: ApplyResult = {
      feature: 'context-menu', sessionId: ensureSession().id, entryIds: [], ok: true, verified: 'applied',
      message: '本来就是这样，不用改。', reboot: 'none', notes: [], error: null,
    }
    if ((before !== MENU_HIDDEN) === visible) return result
    const session = ensureSession()
    const entry: JournalEntryView = {
      id: uuid(), sessionId: session.id, time: iso(Date.now()), feature: 'context-menu',
      featureTitle: `右键菜单：${item.title}`, target, before, after: visible ? MENU_SHOWN : MENU_HIDDEN,
      ok: true, pending: false, undone: false, undoneAt: null, canUndo: true, error: null,
    }
    session.entries.push(entry)
    values.set(target, entry.after)
    result.entryIds = [entry.id]
    result.reboot = command ? 'none' : 'explorer'
    result.message = visible
      ? command ? '已经恢复了，下次右键就能看到。' : '已经恢复了，重启资源管理器（或者注销再登录）以后就能看到。'
      : command
        ? '已经从右键菜单里拿掉了，下次右键就看不到了。软件本身不受影响；想要回来，在这里点「恢复」，或者在修改日志里撤销。'
        : '已经拿掉了，重启资源管理器（或者注销再登录）以后生效。软件本身不受影响；想要回来，在这里点「恢复」，或者在修改日志里撤销。'
    return result
  },

  new_menu_list: () => newMenuMock.map((item) => ({ ...item, visible: values.get(`newmenu:${item.id}`) !== NEW_MENU_HIDDEN })),

  new_menu_set: ({ id, visible }) => {
    const item = newMenuMock.find((x) => x.id === id)
    if (!item) throw '这一项不在刚才的列表里了，请刷新一下再试。'
    const target = `newmenu:${item.id}`
    const before = values.get(target) ?? NEW_MENU_SHOWN
    const result: ApplyResult = {
      feature: 'new-menu', sessionId: ensureSession().id, entryIds: [], ok: true, verified: 'applied',
      message: '本来就是这样，不用改。', reboot: 'none', notes: [], error: null,
    }
    if ((before !== NEW_MENU_HIDDEN) === visible) return result
    const session = ensureSession()
    const entry: JournalEntryView = {
      id: uuid(), sessionId: session.id, time: iso(Date.now()), feature: 'new-menu',
      featureTitle: `「新建」菜单：${item.title}`, target, before, after: visible ? NEW_MENU_SHOWN : NEW_MENU_HIDDEN,
      ok: true, pending: false, undone: false, undoneAt: null, canUndo: true, error: null,
    }
    session.entries.push(entry)
    values.set(target, entry.after)
    result.entryIds = [entry.id]
    result.message = visible
      ? '已经恢复了，下次在右键「新建」里就能看到。'
      : '已经从右键「新建」菜单里拿掉了。软件本身不受影响；想要回来，在这里点「恢复」，或者在修改日志里撤销。'
    return result
  },

  shell_places_list: () =>
    shellPlacesMock.map((item) => ({ ...item, visible: values.get(`shellplace:${item.id}`) !== PLACE_HIDDEN })),

  shell_places_set: ({ id, visible }) => {
    const item = shellPlacesMock.find((x) => x.id === id)
    if (!item) throw '这一项不在刚才的列表里了，请刷新一下再试。'
    const target = `shellplace:${item.id}`
    const before = values.get(target) ?? PLACE_SHOWN
    const result: ApplyResult = {
      feature: 'shell-places', sessionId: ensureSession().id, entryIds: [], ok: true, verified: 'applied',
      message: '本来就是这样，不用改。', reboot: 'none', notes: [], error: null,
    }
    if ((before !== PLACE_HIDDEN) === visible) return result
    const session = ensureSession()
    const entry: JournalEntryView = {
      id: uuid(), sessionId: session.id, time: iso(Date.now()), feature: 'shell-places',
      featureTitle: `资源管理器里的图标：${item.title}`, target, before, after: visible ? PLACE_SHOWN : PLACE_HIDDEN,
      ok: true, pending: false, undone: false, undoneAt: null, canUndo: true, error: null,
    }
    session.entries.push(entry)
    values.set(target, entry.after)
    result.entryIds = [entry.id]
    result.reboot = 'explorer'
    result.message = visible
      ? '已经恢复了，新打开的资源管理器窗口里就能看到；还看不到的话，重启一下资源管理器。'
      : '已经隐藏了，新打开的资源管理器窗口里就看不到了；还看得到的话，重启一下资源管理器。软件本身不受影响；想要回来，在这里点「恢复」，或者在修改日志里撤销。'
    return result
  },

  key_remap_get: () => ({
    keys: keymapKeys,
    mappings: keymapCurrent().map((m) => ({ from: m.from, to: m.to, text: keymapText(m) })),
    foreign: false,
    foreignText: null,
  }),

  key_remap_set: ({ mappings }) => {
    const wanted = keymapResolve(mappings)
    const before = values.get(KEYMAP_TARGET) ?? KEYMAP_NONE
    const after = wanted.length === 0 ? KEYMAP_NONE : wanted.map(keymapText).join('；')
    const result: ApplyResult = {
      feature: 'key-remap', sessionId: ensureSession().id, entryIds: [], ok: true, verified: 'applied',
      message: '本来就是这样，不用改。', reboot: 'none', notes: [], error: null,
    }
    if (before === after) return result
    keymapStates.set(after, wanted)
    const session = ensureSession()
    const entry: JournalEntryView = {
      id: uuid(), sessionId: session.id, time: iso(Date.now()), feature: 'key-remap',
      featureTitle: '键位重映射（改键）', target: KEYMAP_TARGET, before, after,
      ok: true, pending: false, undone: false, undoneAt: null, canUndo: true, error: null,
    }
    session.entries.push(entry)
    values.set(KEYMAP_TARGET, after)
    result.entryIds = [entry.id]
    result.reboot = 'reboot'
    result.message = wanted.length === 0
      ? '已经把改键全部去掉了，重启电脑以后所有键恢复原样。'
      : `已经改好了：${after}。重启电脑以后生效。想改回来，在这里点「全部恢复」，或者在修改日志里撤销，也是重启以后生效。`
    return result
  },

  ocr_recognize: ({ bytes }) => {
    if (bytes[0] !== 0x89 || bytes[1] !== 0x50 || bytes[2] !== 0x4e || bytes[3] !== 0x47) throw '图片要先转成 PNG 才能认字。'
    return {
      status: 'ok',
      text: '（演示）浏览器里不会真的认字，在 Windows 的小药箱里才会。\n连接到打印机\n操作无法完成(错误 0x00000709)。再次检查打印机名称，并确保打印机已连接到网络。',
      lines: 3,
      language: '中文（简体）',
      languages: ['中文（简体）', '英语'],
      chinese: true,
      truncated: false,
      detail: null,
    }
  },

  rename_select_folder: () => DEMO_FOLDER,
  rename_preview: ({ rules }) => {
    const wanted = rules.extensions
      .split(/[,，;；\s]+/)
      .map((e) => e.trim().replace(/^\./, '').toLowerCase())
      .filter(Boolean)
    const files = demoFiles.filter((f) => !wanted.length || wanted.includes(f.split('.').pop()!.toLowerCase()))
    if (!files.length) throw '文件夹里没有这几种扩展名的文件。'
    const last = rules.start + files.length - 1
    const width = rules.digits || Math.max(2, String(last).length)
    const entries = files.map((source, i) => {
      const dot = source.lastIndexOf('.')
      let name = dot > 0 ? source.slice(0, dot) : source
      let ext: string | null = dot > 0 ? source.slice(dot + 1) : null
      if (rules.find) name = name.split(rules.find).join(rules.replace)
      if (rules.numbering) name = `${rules.base}${String(rules.start + i).padStart(width, '0')}`
      name = `${rules.prefix}${name}${rules.suffix}`
      if (rules.extension === 'lower') ext = ext?.toLowerCase() ?? null
      if (rules.extension === 'set') ext = rules.newExtension.trim().replace(/^\./, '') || null
      const target = ext ? `${name}.${ext}` : name
      return { source, target, changed: source !== target }
    })
    demoPlan = entries
    return { folder: DEMO_FOLDER, entries, changed: entries.filter((e) => e.changed).length, skipped: demoFiles.length - files.length }
  },
  rename_apply: () => {
    const changed = demoPlan.filter((e) => e.changed)
    if (!changed.length) throw '按这些规则，没有文件的名字会变。'
    demoLast = changed
    demoFiles = demoFiles.map((f) => changed.find((e) => e.source === f)?.target ?? f)
    demoPlan = []
    return changed.length
  },
  rename_undo: () => {
    if (!demoLast.length) throw '没有可以撤销的重命名。'
    const back = demoLast
    demoFiles = demoFiles.map((f) => back.find((e) => e.target === f)?.source ?? f)
    demoLast = []
    return back.length
  },
  image_select_folder: () => {
    demoImageFolder = true
    return DEMO_IMAGE_FOLDER
  },
  // 和后端一样：只新建、不覆盖，重名就在名字后面加「 (2)」（演示里只记名字，不真的保存）
  image_save: ({ name, bytes }) => {
    if (!demoImageFolder) throw '请先选择保存到哪个文件夹。'
    if (!bytes.length) throw `「${name}」是空的，没有保存。`
    const dot = name.lastIndexOf('.')
    const [stem, ext] = [name.slice(0, dot), name.slice(dot + 1)]
    let candidate = name
    for (let n = 2; demoSaved.has(candidate.toLowerCase()); n++) candidate = `${stem} (${n}).${ext}`
    demoSaved.add(candidate.toLowerCase())
    return candidate
  },
  screen_fullscreen: () => null,
  awake_get: () => ({ ...demoAwake }),
  awake_set: ({ on, display }) => {
    demoAwake = { on, display: on && display }
    return { ...demoAwake }
  },
  shutdown_get: () => ({ plan: demoShutdown }),
  shutdown_schedule: ({ seconds, restart }) => {
    if (seconds < 60 || seconds > 24 * 3600 + 60) throw '时间要在 1 分钟以后、24 小时以内。'
    demoShutdown = { at: Date.now() + seconds * 1000, restart }
    return { plan: demoShutdown }
  },
  shutdown_cancel: () => {
    const cancelled = demoShutdown !== null
    demoShutdown = null
    return { cancelled }
  },
  image_open_folder: () => {
    if (!demoImageFolder) throw '还没有选择保存的文件夹。'
    return null
  },
  // 演示里不弹「另存为」对话框，也不真的保存：假装存进了演示文件夹
  pdf_save: ({ name, bytes }) => {
    const text = new TextDecoder().decode(bytes.subarray(0, 5))
    if (text !== '%PDF-') throw '内容不是 PDF，没有保存。'
    demoPdfSaved = true
    return `${DEMO_IMAGE_FOLDER}\\${name}`
  },
  pdf_reveal: () => {
    if (!demoPdfSaved) throw '还没有存过 PDF。'
    return null
  },
  long_image_save: ({ name, bytes }) => {
    // 和后端一样只收完整的 JPG、PNG
    const jpeg = bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff
    const png = bytes[0] === 0x89 && new TextDecoder().decode(bytes.subarray(1, 4)) === 'PNG'
    if (!jpeg && !png) throw '内容不是 JPG 或 PNG 图片，没有保存。'
    demoLongImageSaved = true
    return `${DEMO_IMAGE_FOLDER}\\${name}`
  },
  long_image_reveal: () => {
    if (!demoLongImageSaved) throw '还没有存过长图。'
    return null
  },
  lockers_pick_files: () => {
    demoLockTarget = 'files'
    demoLockClosed = false
    return demoLockReport()
  },
  lockers_pick_folder: () => {
    demoLockTarget = 'folder'
    demoLockClosed = false
    return demoLockReport()
  },
  // 演示「关掉程序以后再查一次」：第二次查的时候，只剩下关不掉的系统服务
  lockers_refresh: () => {
    if (!demoLockTarget) return null
    demoLockClosed = true
    return demoLockReport()
  },
  popup_find: () => {
    const r = DEMO_POPUPS[demoPopupIndex % DEMO_POPUPS.length]!
    demoPopupIndex++
    demoPopupFound = r.exe !== null
    return r
  },
  popup_reveal: () => {
    if (!demoPopupFound) throw '还没有找到是哪个程序，请先点「开始找」。'
    return null
  },
  space_pick_folder: () => {
    demoSpace = true
    return demoSpaceReport()
  },
  space_rescan: () => (demoSpace ? demoSpaceReport() : null),
  space_reveal: ({ id }) => {
    if (!demoSpace || id < 0 || id >= DEMO_SPACE_IDS) throw '这个文件不在刚才的结果里，请重新查一遍。'
    return null
  },
  disk_speed_drives: () => DEMO_DRIVES,
  // 回收站（演示）：C 盘的回收站里有东西，D 盘的是空的，U 盘上没有回收站文件夹；清空以后记着
  recycle_drives: () =>
    DEMO_DRIVES.map((d) => {
      const emptied = demoRecycleEmptied.has(d.letter)
      const files = d.letter === 'C' && !emptied ? 356 : 0
      return {
        letter: d.letter, label: d.label, removable: d.removable, system: d.system,
        exists: !d.removable && !emptied, files, bytes: files ? 1_288_490_188 : 0, complete: true,
      }
    }),
  // 此应用无法在你的电脑上运行（演示）：选了一个 ARM64 版的安装包
  exe_check_pick: () => ({
    name: '某软件安装包_ARM64.exe', size: 86_507_520, verdict: 'wrong-machine', guess: null, machine: 'arm64',
    pc: 'x64', windows11: true, console: false, dotnet: false,
  }),
  recycle_repair: ({ letter }) => {
    const drive = DEMO_DRIVES.find((d) => d.letter === letter.toUpperCase())
    if (!drive) throw '这个盘现在不在了，请刷新一下列表。'
    const outcome = drive.removable || demoRecycleEmptied.has(drive.letter) ? 'absent' : 'done'
    demoRecycleEmptied.add(drive.letter)
    return { letter: drive.letter, outcome, left: 0 }
  },
  // 演示：每个盘给一组典型的速度（不真的测）
  disk_speed_run: ({ letter }) => {
    const drive = DEMO_DRIVES.find((d) => d.letter === letter.toUpperCase())
    if (!drive) throw '这个盘现在不在了，请刷新一下列表。'
    if (!drive.canTest) throw '这个盘剩余空间不到 2 GB，测速要写一个临时文件，先腾出点地方再测。'
    const result = DEMO_SPEEDS[drive.letter]
    if (!result) throw '演示里没有这个盘的数据。'
    return result
  },
  // 和后端一样：程序和脚本文件照样藏着，改过的能撤销（演示里只记着，不碰真实文件）
  hidden_pick_folder: () => {
    demoHiddenPicked = true
    return demoHiddenReport()
  },
  hidden_rescan: () => (demoHiddenPicked ? demoHiddenReport() : null),
  hidden_restore: ({ ids }) => {
    if (!demoHiddenPicked) throw '请先选择 U 盘。'
    if (!ids.length) throw '没有勾选要显示出来的文件。'
    const visible = DEMO_HIDDEN.filter((item) => !demoHiddenShown.has(item.name))
    let changed = 0
    for (const id of ids) {
      const item = visible[id]
      if (!item) throw '有的项目不在刚才的结果里，请重新查一遍。'
      demoHiddenShown.add(item.name)
      changed += 1 + item.hiddenInside - (item.name === '作业' ? 1 : 0)
    }
    const keptPrograms = ids.some((id) => visible[id]?.name === '作业') ? 1 : 0
    demoHiddenChanged += changed
    return { result: { changed, keptPrograms, failed: 0, truncated: false }, report: demoHiddenReport() }
  },
  hidden_undo: () => {
    if (!demoHiddenChanged) throw '没有可以撤销的。'
    const restored = demoHiddenChanged
    demoHiddenShown.clear()
    demoHiddenChanged = 0
    return { result: { restored, failed: 0 }, report: demoHiddenPicked ? demoHiddenReport() : null }
  },
}

// ── 硬盘测速（演示）──
const DISK_GB = 1024 ** 3
const DEMO_DRIVES: DriveView[] = [
  { letter: 'C', label: '', fileSystem: 'NTFS', removable: false, system: true, total: 476 * DISK_GB, free: 118 * DISK_GB, canTest: true },
  { letter: 'D', label: '资料', fileSystem: 'NTFS', removable: false, system: false, total: 931 * DISK_GB, free: 402 * DISK_GB, canTest: true },
  { letter: 'F', label: 'KINGSTON', fileSystem: 'FAT32', removable: true, system: false, total: 29 * DISK_GB, free: 12 * DISK_GB, canTest: true },
  { letter: 'G', label: '', fileSystem: 'exFAT', removable: true, system: false, total: 8 * DISK_GB, free: 1.2 * DISK_GB, canTest: false },
]
const DEMO_SPEEDS: Record<string, SpeedResult> = {
  C: { seqWrite: 2803.4, seqRead: 3151.9, randomRead: 58.7, randomIops: 15027, testedBytes: 1024 * 1024 * 1024, verdict: 'nvme' },
  D: { seqWrite: 142.6, seqRead: 156.3, randomRead: 0.8, randomIops: 205, testedBytes: 1024 * 1024 * 1024, verdict: 'hdd' },
  F: { seqWrite: 9.4, seqRead: 27.8, randomRead: 2.6, randomIops: 666, testedBytes: 96 * 1024 * 1024, verdict: 'slow' },
}

// ── U 盘里的文件不见了（演示：一个中了病毒的 U 盘）──
let demoHiddenPicked = false
let demoHiddenChanged = 0
const demoHiddenShown = new Set<string>()
const DEMO_HIDDEN = [
  { name: '作业', isDir: true, size: 0, inside: 128, hiddenInside: 127, countedAll: true },
  { name: '照片', isDir: true, size: 0, inside: 356, hiddenInside: 356, countedAll: true },
  { name: '简历.docx', isDir: false, size: 48_213, inside: 0, hiddenInside: 0, countedAll: true },
]
function demoHiddenReport(): HiddenReport {
  const items = DEMO_HIDDEN.filter((item) => !demoHiddenShown.has(item.name)).map((item, id) => ({ id, ...item }))
  return {
    folder: 'F:\\',
    items,
    programs: ['autorun.inf', 'DeviceConfigManager.vbs'],
    shortcuts: ['作业.lnk', '照片.lnk', '简历.docx.lnk'],
    canUndo: demoHiddenChanged,
  }
}

// ── 找大文件和重复文件（演示）──
let demoSpace = false
const DEMO_SPACE_IDS = 8
const GB = 1024 * 1024 * 1024
const MB = 1024 * 1024

function demoSpaceReport(): SpaceReport {
  const day = (d: string) => new Date(`${d}T10:00:00`).getTime()
  const f = (id: number, name: string, folder: string, size: number, modified: string, isProtected = false) => ({
    id, name, folder, size, modified: day(modified), protected: isProtected,
  })
  const iso = f(0, 'Win11_24H2_Chinese_Simplified_x64.iso', 'Users\\演示\\Downloads', 5.4 * GB, '2025-03-02')
  const dump = f(1, 'MEMORY.DMP', 'Windows', 3.1 * GB, '2026-08-19', true)
  const video = f(2, '课程录屏.mp4', 'Users\\演示\\Videos', 1.3 * GB, '2026-05-11')
  const videoCopy = f(3, '课程录屏.mp4', 'Users\\演示\\Desktop\\备份', 1.3 * GB, '2026-05-11')
  const installer = f(4, 'WeChatSetup.exe', 'Users\\演示\\Downloads', 260 * MB, '2026-07-30')
  const photoSize = 4.8 * MB
  const photo = [5, 6, 7].map((id, i) =>
    f(id, i ? `IMG_2041 (${i}).JPG` : 'IMG_2041.JPG', i === 2 ? 'Users\\演示\\Pictures\\导入' : 'Users\\演示\\Pictures', photoSize, '2026-06-01'),
  )
  return {
    folder: 'C:\\（演示）',
    files: 128_406,
    totalBytes: 86.4 * GB,
    skipped: 3,
    onlineOnly: 214,
    truncated: false,
    largest: [iso, dump, video, videoCopy, installer],
    duplicates: [
      { size: video.size, count: 2, files: [video, videoCopy] },
      { size: photoSize, count: 3, files: photo },
    ],
    duplicateGroups: 2,
    wastedBytes: video.size + 2 * photoSize,
    comparedAll: true,
  }
}

// ── 弹窗是哪个软件的（演示）：依次演示软件的弹窗、Windows 的通知、鼠标还在小药箱上 ──
const DEMO_POPUPS: WindowOwnerReport[] = [
  {
    kind: 'program', exe: 'KanTuNews.exe', description: '热点资讯', company: '示例网络科技有限公司', product: '快看图',
    folder: 'C:\\Program Files (x86)\\KanTu\\News', installed: '快看图', publisher: '示例网络科技有限公司',
    position: 'bottom-right', width: 360, height: 260,
  },
  {
    kind: 'notification', exe: 'ShellExperienceHost.exe', description: 'Windows Shell Experience Host',
    company: 'Microsoft Corporation', product: 'Microsoft® Windows® Operating System',
    folder: 'C:\\Windows\\SystemApps\\ShellExperienceHost_cw5n1h2txyewy', installed: null, publisher: null,
    position: 'bottom-right', width: 380, height: 150,
  },
  {
    kind: 'medkit', exe: null, description: null, company: null, product: null, folder: null, installed: null,
    publisher: null, position: 'center', width: 1200, height: 800,
  },
]
let demoPopupIndex = 0
let demoPopupFound = false

// ── 文件删不掉：是谁占着（演示）──
let demoLockTarget: 'files' | 'folder' | null = null
let demoLockClosed = false

function demoLockReport(): FileLockReport {
  const folder = demoLockTarget === 'folder'
  const users: FileLockUser[] = [
    {
      pid: 4812, name: 'Microsoft Word', program: 'WINWORD.EXE', kind: 'window', service: null,
      files: [folder ? '合同\\合同（终稿）.docx' : '合同（终稿）.docx'], moreFiles: 0, isSelf: false, otherSession: false,
    },
    {
      pid: 7036, name: 'Windows 资源管理器', program: 'explorer.exe', kind: 'explorer', service: null,
      files: [folder ? '照片\\IMG_2041.JPG' : 'IMG_2041.JPG'], moreFiles: 0, isSelf: false, otherSession: false,
    },
    {
      pid: 3120, name: 'Microsoft Defender Antivirus Service', program: 'MsMpEng.exe', kind: 'service', service: 'WinDefend',
      files: [folder ? '下载\\setup.exe' : 'setup.exe'], moreFiles: 0, isSelf: false, otherSession: false,
    },
  ]
  return {
    mode: folder ? 'folder' : 'files',
    targets: folder ? ['旧项目'] : ['合同（终稿）.docx', 'IMG_2041.JPG', 'setup.exe'],
    checked: folder ? 42 : 3,
    missing: [],
    failed: [],
    truncated: false,
    unreadable: 0,
    users: demoLockClosed ? users.filter((u) => u.kind === 'service') : users,
  }
}

// ── 别让电脑自己睡着（演示）──
let demoAwake = { on: false, display: false }

// ── 定时关机（演示：只记着，不会真的关机）──
let demoShutdown: { at: number; restart: boolean } | null = null

// ── 图片批量处理的演示（浏览器里真的处理图片，但不保存）──
const DEMO_IMAGE_FOLDER = '演示文件夹（浏览器里不会真的保存）'
let demoPdfSaved = false
let demoLongImageSaved = false
let demoImageFolder = false
const demoSaved = new Set<string>()

// ── 批量重命名的演示文件（浏览器里预览界面用，不碰真实文件）──
const DEMO_FOLDER = '演示文件夹（不会改动真实文件）'
let demoFiles = ['IMG_0001.JPG', 'IMG_0002.JPG', '海边.png', '说明.txt']
/** 演示里清空过回收站的盘 */
const demoRecycleEmptied = new Set<string>()
let demoPlan: { source: string; target: string; changed: boolean }[] = []
let demoLast: { source: string; target: string; changed: boolean }[] = []

/** 个别小工具要多等一会儿（读电脑配置、重启资源管理器、硬盘测速），好看清「正在…」的样子 */
function extraDelay(cmd: CommandName, args: unknown): number {
  if (cmd === 'disk_speed_run') return 3000
  if (cmd !== 'tool_run' || typeof args !== 'object' || args === null || !('id' in args)) return 0
  const id = args.id
  return typeof id === 'string' ? (TOOLS.get(id)?.slowMs ?? 0) : 0
}

/** 和 Tauri 的 invoke 用法一样：等 300–800 毫秒后返回结果的副本；出错时 reject 一个字符串 */
export async function mockInvoke<K extends CommandName>(cmd: K, args: CommandArgs<K>): Promise<CommandResult<K>> {
  await sleep(randomInt(300, 800) + extraDelay(cmd, args))
  const handler: (a: CommandArgs<K>) => CommandResult<K> = handlers[cmd]
  return clone(handler(args))
}
