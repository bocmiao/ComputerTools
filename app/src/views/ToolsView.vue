<script setup lang="ts">
import {
  computed,
  nextTick,
  onActivated,
  onBeforeUnmount,
  onDeactivated,
  reactive,
  ref,
  useTemplateRef,
  watch,
  type Component,
} from 'vue'
import { toolRun } from '../api'
import type { ApplyResult, ToolOpens, ToolResult, ToolSummary } from '../api/types'
import AmountWordsTool from '../components/AmountWordsTool.vue'
import AppIcon, { type IconName } from '../components/AppIcon.vue'
import BatchImageTool from '../components/BatchImageTool.vue'
import BatchRenameTool from '../components/BatchRenameTool.vue'
import BrightnessControl from '../components/BrightnessControl.vue'
import BusySpinner from '../components/BusySpinner.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import DateCalcTool from '../components/DateCalcTool.vue'
import DeviceTests from '../components/DeviceTests.vue'
import DiskSpeedTest from '../components/DiskSpeedTest.vue'
import FileHashTool from '../components/FileHashTool.vue'
import FileLockers from '../components/FileLockers.vue'
import HiddenFilesTool from '../components/HiddenFilesTool.vue'
import ImagePrivacyTool from '../components/ImagePrivacyTool.vue'
import ImagesToPdfTool from '../components/ImagesToPdfTool.vue'
import JsonTool from '../components/JsonTool.vue'
import KeepAwake from '../components/KeepAwake.vue'
import LongImageTool from '../components/LongImageTool.vue'
import OcrTool from '../components/OcrTool.vue'
import PopupOwner from '../components/PopupOwner.vue'
import PreviewDialog from '../components/PreviewDialog.vue'
import ShutdownTimer from '../components/ShutdownTimer.vue'
import SpaceFinder from '../components/SpaceFinder.vue'
import TagPill from '../components/TagPill.vue'
import TextDiffTool from '../components/TextDiffTool.vue'
import TextEncodeTool from '../components/TextEncodeTool.vue'
import TextQrTool from '../components/TextQrTool.vue'
import TextTidyTool from '../components/TextTidyTool.vue'
import ToolInfoResult from '../components/ToolInfoResult.vue'
import ToolOutcome from '../components/ToolOutcome.vue'
import WechatCleanup from '../components/WechatCleanup.vue'
import { audienceLabel } from '../labels'
import { catalog, catalogTools, markHealthStale, nav } from '../state'
import { rememberFocus, restoreFocus } from '../utils/dialogs'
import { errorText, normalizeForSearch } from '../utils/format'
import { LOCAL_TOOLS, LOCAL_TOOL_GROUPS, type LocalToolGroup } from '../utils/localTools'
import { useToolOpen } from '../utils/tools'

// 工具箱：上面是搜索框和分类，下面按分类排着一个个图标。点一个图标，在这一页里单独打开那一个工具（单个工具页），
// 面包屑「← 工具箱」回到图标这里，滚回原来的位置，焦点还给刚才点的图标。
// - 文字、图片、文件、测试和控制：界面自己的工具（名单在 utils/localTools.ts，组件在下面按 id 对上）；
// - 电脑信息、一键处理：目录里「看信息」「一键处理」的小工具，单个工具页里查看或者开始，结果显示在下面；
// - 系统工具和设置：打开系统工具、「设置」里的页面、微软的疑难解答、官方网页，点了直接打开，没有单个工具页。
// 看信息、一键处理、打开的这些都不改设置，所以不记进修改日志，也不用撤销。
// 从别的页面跳过来：
// - nav.toolId（检测结果里的 tool: 按钮、导航栏搜索）：打开那个小工具，「看信息」的顺便查一次；打开类的滚到那个按钮、
//   把焦点放上去、标出来；
// - nav.localToolId（导航栏搜索）：打开那个本地工具；
// - nav.testId（症状指引里「测一测喇叭」这类按钮）：打开「屏幕、键盘、鼠标、声音测试」，定位到那一项。

type Category = LocalToolGroup | 'info' | 'action' | 'open'
type TabId = 'all' | Category

/** 图标网格、最近用过、单个工具页用的一项：本地工具或者目录里的小工具 */
interface Item {
  /** local:<id> 或 tool:<id>（最近用过也存这个） */
  key: string
  title: string
  /** 图标下面那一行 */
  short: string
  /** 单个工具页页头的说明 */
  description: string
  icon: IconName
  category: Category
  /** 「给懂哥」这类标签，没有是空的 */
  tag: string
  /** 搜索时认的字，已经换成搜索用的写法 */
  search: string[]
  /** 目录里的小工具；本地工具是 null */
  tool: ToolSummary | null
  /** 本地工具的组件；目录里的小工具是 null */
  component: Component | null
}

// ── 本地工具 ──

/** 本地工具的组件（id 见 utils/localTools.ts） */
const LOCAL_COMPONENTS: Record<string, Component> = {
  'file-hash': FileHashTool,
  'text-tidy': TextTidyTool,
  json: JsonTool,
  'text-encode': TextEncodeTool,
  'text-diff': TextDiffTool,
  'text-qr': TextQrTool,
  'amount-words': AmountWordsTool,
  'date-calc': DateCalcTool,
  'image-batch': BatchImageTool,
  'images-pdf': ImagesToPdfTool,
  'long-image': LongImageTool,
  ocr: OcrTool,
  'image-privacy': ImagePrivacyTool,
  'batch-rename': BatchRenameTool,
  'file-lockers': FileLockers,
  'popup-owner': PopupOwner,
  'space-finder': SpaceFinder,
  wechat: WechatCleanup,
  'hidden-files': HiddenFilesTool,
  'device-tests': DeviceTests,
  'disk-speed': DiskSpeedTest,
  'keep-awake': KeepAwake,
  brightness: BrightnessControl,
  'shutdown-timer': ShutdownTimer,
}

const DEVICE_TESTS_KEY = 'local:device-tests'

const localItems: Item[] = LOCAL_TOOLS.flatMap((t) => {
  const component = LOCAL_COMPONENTS[t.id]
  if (!component) return []
  return [
    {
      key: `local:${t.id}`,
      title: t.title,
      short: t.short,
      description: t.description,
      icon: t.icon,
      category: t.group,
      tag: '',
      search: [t.title, t.short, ...t.keywords].map(normalizeForSearch),
      tool: null,
      component,
    },
  ]
})

// ── 目录里的小工具 ──

/** 给懂哥用的排在后面，其余保持目录里的顺序 */
const tools = computed(() => {
  const list = catalogTools()
  return [...list.filter((t) => t.audience !== 'helper'), ...list.filter((t) => t.audience === 'helper')]
})

function catalogIcon(t: ToolSummary): IconName {
  if (t.group === 'info') return 'info'
  if (t.group === 'action') return 'bolt'
  if (t.opens === 'settings') return 'settings'
  if (t.opens === 'get-help') return 'help'
  if (t.opens === 'website') return 'globe'
  return 'window'
}

const catalogItems = computed<Item[]>(() =>
  tools.value.map((t) => ({
    key: `tool:${t.id}`,
    title: t.title,
    short: t.description,
    description: t.description,
    icon: catalogIcon(t),
    category: t.group,
    tag: audienceLabel[t.audience],
    search: [t.title, t.description].map(normalizeForSearch),
    tool: t,
    component: null,
  })),
)

const allItems = computed(() => [...localItems, ...catalogItems.value])
const itemsByKey = computed(() => new Map(allItems.value.map((i) => [i.key, i])))

// ── 分类 ──

interface CategoryInfo {
  id: Category
  title: string
  /** 分类标题旁边的一句话 */
  note: string
}

const INFO_NOTE = '只读取，不会改动电脑。'
const ACTION_NOTE = '清缓存、重开程序这类一次性的小操作：不改设置，不记进「修改日志」，也不用撤销。'

const CATEGORIES: CategoryInfo[] = [
  ...LOCAL_TOOL_GROUPS.map((g) => ({ id: g.id, title: g.title, note: '' })),
  { id: 'info', title: '电脑信息', note: INFO_NOTE },
  { id: 'action', title: '一键处理', note: ACTION_NOTE },
  { id: 'open', title: '系统工具和设置', note: '' },
]

const TABS: { id: TabId; title: string }[] = [{ id: 'all', title: '全部' }, ...CATEGORIES]

/** 这几类来自目录，目录读好以前是空的 */
const CATALOG_CATEGORIES: ReadonlySet<Category> = new Set<Category>(['info', 'action', 'open'])

function categoryTitle(c: Category): string {
  return CATEGORIES.find((x) => x.id === c)?.title ?? ''
}

/** 系统工具和设置：按打开的是什么分几组，每组一句话 */
const OPEN_GROUPS: { id: ToolOpens; title: string; icon: IconName; note: string }[] = [
  {
    id: 'program',
    title: '打开系统工具',
    icon: 'window',
    note: 'Windows 自带的工具，小药箱只负责打开，在里面改的东西不记进「修改日志」。',
  },
  {
    id: 'settings',
    title: '打开「设置」里的页面',
    icon: 'settings',
    note: '直接跳到 Windows「设置」里的那一页，在那里改的东西同样不记进「修改日志」。',
  },
  {
    id: 'get-help',
    title: '微软的疑难解答',
    icon: 'help',
    note: '微软「获取帮助」里的自动检查，能修的直接修，要联网；它改了什么不记进「修改日志」。',
  },
  {
    id: 'website',
    title: '官方网页',
    icon: 'globe',
    note: '在浏览器里打开核实过的官方网站，用你自己的账户打开，不带管理员身份。',
  },
]
const HELPER_NOTE = '标着「给懂哥」的比较专业，不熟悉的话别动里面的东西。'

// ── 搜索、分类筛选 ──

const query = ref('')
const tab = ref<TabId>('all')
const normalizedQuery = computed(() => normalizeForSearch(query.value))

/**
 * 搜的话里有几个字被这一项的名字、关键词盖住：「压缩图片」里，图片批量处理的「压缩」「图片」盖住 4 个字，
 * JSON 格式化的「压缩」只盖住 2 个
 */
function coverage(item: Item, q: string): number {
  const hit = new Array<boolean>(q.length).fill(false)
  for (const s of item.search) {
    if (s.length < 2) continue
    for (let at = q.indexOf(s); at >= 0; at = q.indexOf(s, at + 1)) hit.fill(true, at, at + s.length)
  }
  return hit.filter(Boolean).length
}

/**
 * 搜到的（Item.key）；没在搜是 null。名字、一句话、关键词、说明里有搜的字的都算；
 * 搜的是几个词连在一起（「压缩图片」）的，算被盖住得最多的那几个
 */
const found = computed<ReadonlySet<string> | null>(() => {
  const q = normalizedQuery.value
  if (!q) return null
  const scored = allItems.value.map((item) => ({ item, covered: coverage(item, q) }))
  let hits = scored.filter(({ item, covered }) => covered === q.length || item.search.some((s) => s.includes(q)))
  if (hits.length === 0) {
    const best = Math.max(0, ...scored.map((x) => x.covered))
    if (best >= 2) hits = scored.filter((x) => x.covered === best)
  }
  return new Set(hits.map((x) => x.item.key))
})

const sections = computed(() => {
  const keys = found.value
  return CATEGORIES.filter((c) => tab.value === 'all' || tab.value === c.id).map((c) => ({
    ...c,
    items: allItems.value.filter((i) => i.category === c.id && (!keys || keys.has(i.key))),
  }))
})
const shownSections = computed(() => sections.value.filter((s) => s.items.length > 0))

/** 目录还没读好，而看的分类里有目录里的小工具 */
const catalogLoading = computed(() => !catalog.value && sections.value.some((s) => CATALOG_CATEGORIES.has(s.id)))

const emptyText = computed(() => {
  if (shownSections.value.length > 0 || catalogLoading.value) return ''
  if (normalizedQuery.value) return '没找到相关的工具。换个说法试试，比如「压缩」「密码」「关机」。'
  return '这个版本的小药箱还没有这类工具。'
})

/** 这一类里没搜到，别的分类里有 */
const foundElsewhere = computed(() => tab.value !== 'all' && (found.value?.size ?? 0) > 0)

/** 搜到几个：读屏软件读出来（看得见的人直接看图标） */
const foundText = computed(() => {
  if (!normalizedQuery.value) return ''
  const n = shownSections.value.reduce((sum, s) => sum + s.items.length, 0)
  return n > 0 ? `找到 ${n} 个工具` : ''
})

function tabId(id: TabId): string {
  return `tools-tab-${id}`
}

const PANEL_ID = 'tools-panel'

/** 分类按钮：左右方向键换一个（换了就是选了），Home、End 到头 */
function onTabKeydown(e: KeyboardEvent): void {
  const i = TABS.findIndex((t) => t.id === tab.value)
  let next: number
  if (e.key === 'ArrowRight') next = (i + 1) % TABS.length
  else if (e.key === 'ArrowLeft') next = (i - 1 + TABS.length) % TABS.length
  else if (e.key === 'Home') next = 0
  else if (e.key === 'End') next = TABS.length - 1
  else return
  e.preventDefault()
  const t = TABS[next]
  if (!t) return
  tab.value = t.id
  document.getElementById(tabId(t.id))?.focus()
}

// ── 元素的 id（返回工具箱时靠它们找回焦点） ──

function domId(key: string): string {
  return key.replace(/[^A-Za-z0-9-]/g, '-')
}

function tileId(key: string): string {
  return `tools-tile-${domId(key)}`
}

function recentId(key: string): string {
  return `tools-recent-${domId(key)}`
}

function openButtonId(id: string): string {
  return `tools-open-${domId(id)}`
}

function categoryTitleId(c: Category): string {
  return `tools-cat-${c}`
}

// ── 最近用过：最近打开过的 4 个工具（本地工具和目录里的小工具），记在这台电脑上 ──

const RECENT_KEY = 'medkit.recentTools'
const RECENT_MAX = 4

function readRecent(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_KEY)
    const list: unknown = raw ? JSON.parse(raw) : []
    return Array.isArray(list) ? list.filter((k): k is string => typeof k === 'string').slice(0, RECENT_MAX) : []
  } catch {
    return []
  }
}

const recent = ref<string[]>(readRecent())

function rememberRecent(key: string): void {
  const next = [key, ...recent.value.filter((k) => k !== key)].slice(0, RECENT_MAX)
  recent.value = next
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(next))
  } catch {
    // 存不下来（例如不让存）：这次开着的时候照样记着
  }
}

/** 目录还没读好、目录里已经没有的先不显示 */
const recentItems = computed(() =>
  recent.value.map((k) => itemsByKey.value.get(k)).filter((i): i is Item => i !== undefined),
)
const showRecent = computed(() => tab.value === 'all' && !normalizedQuery.value && recentItems.value.length > 0)

// ── 看信息、一键处理 ──

interface RunState {
  running: boolean
  result: ToolResult | null
  /** 命令本身出错、没拿到结果时的原因 */
  error: string | null
  /** 只认最后一次的结果 */
  seq: number
}

const runs = reactive<Record<string, RunState>>({})
const idle: RunState = { running: false, result: null, error: null, seq: 0 }

function runState(id: string): RunState {
  if (!runs[id]) runs[id] = { ...idle }
  // 重新读一次，拿到的才是响应式的对象
  return runs[id]!
}

/** 这个页面是不是正显示着（KeepAlive 里切走以后不去动焦点） */
let active = false

async function run(t: ToolSummary, opts: { keepFocus?: boolean } = {}): Promise<void> {
  const s = runState(t.id)
  if (s.running) return
  // 按钮在执行期间被禁用，焦点会掉到页面上：做完以后还回去
  const focus = rememberFocus()
  const seq = ++s.seq
  s.running = true
  s.error = null
  try {
    const r = await toolRun(t.id)
    if (seq !== s.seq) return
    s.result = r
  } catch (e) {
    if (seq !== s.seq) return
    s.result = null
    s.error = errorText(e)
  } finally {
    if (seq === s.seq) s.running = false
  }
  if (opts.keepFocus) return
  await nextTick()
  const lost = !document.activeElement || document.activeElement === document.body
  if (active && lost) restoreFocus(focus)
}

// 一键处理要先确认的（例如重启资源管理器）：确认框开着等它做完，再关掉，焦点回到「开始」上
const confirmTool = ref<ToolSummary | null>(null)
const confirmBusy = computed(() => (confirmTool.value ? (runs[confirmTool.value.id]?.running ?? false) : false))

function startAction(t: ToolSummary): void {
  if (t.confirm) confirmTool.value = t
  else void run(t)
}

async function confirmAction(): Promise<void> {
  const t = confirmTool.value
  if (!t) return
  await run(t, { keepFocus: true })
  confirmTool.value = null
}

function actionButtonText(s: RunState): string {
  if (s.running) return '正在处理…'
  return s.result ? '再做一次' : '开始'
}

// ── 打开系统工具、「设置」里的页面、网页 ──

const opener = useToolOpen()

const openGroups = computed(() => {
  const list = (sections.value.find((s) => s.id === 'open')?.items ?? []).flatMap((i) => (i.tool ? [i.tool] : []))
  return OPEN_GROUPS.map((g) => {
    const groupTools = list.filter((t) => (t.opens ?? 'program') === g.id)
    const helper = g.id === 'program' && groupTools.some((t) => t.audience === 'helper')
    return {
      ...g,
      note: helper ? `${g.note}${HELPER_NOTE}` : g.note,
      items: groupTools.map((t) => ({ t, open: opener.states[t.id] ?? null })),
    }
  }).filter((g) => g.items.length > 0)
})

// ── 单个工具页 ──

const root = useTemplateRef<HTMLElement>('root')
const gridTitle = useTemplateRef<HTMLElement>('gridTitle')
const detailTitle = useTemplateRef<HTMLElement>('detailTitle')

/** 正打开着的工具（Item.key）；null 是在工具箱的图标这里 */
const openKey = ref<string | null>(null)
const openItem = computed(() => (openKey.value ? (itemsByKey.value.get(openKey.value) ?? null) : null))
const detailTool = computed(() => openItem.value?.tool ?? null)
const detailComponent = computed(() => openItem.value?.component ?? null)
const detailRun = computed(() => (detailTool.value ? (runs[detailTool.value.id] ?? idle) : idle))

const detailButtonText = computed(() => {
  const s = detailRun.value
  if (detailTool.value?.group === 'action') return actionButtonText(s)
  return s.running ? '正在查看…' : s.result ? '重新查看' : '查看'
})

function runDetail(): void {
  const t = detailTool.value
  if (!t) return
  if (t.group === 'action') startAction(t)
  else void run(t)
}

/** 页头下面「其他文字工具」那一排：同一类的别的工具，太多的只列几个 */
const SIBLINGS_MAX = 8

const siblings = computed(() => {
  const item = openItem.value
  if (!item) return { title: '', items: [] as Item[], more: 0 }
  const group = allItems.value.filter((i) => i.category === item.category)
  const at = group.findIndex((i) => i.key === item.key)
  // 列不下的时候从它后面的开始列，每个工具看到的不一样
  const others =
    group.length - 1 > SIBLINGS_MAX
      ? [...group.slice(at + 1), ...group.slice(0, Math.max(0, at))]
      : group.filter((i) => i.key !== item.key)
  return {
    title: `其他${categoryTitle(item.category)}工具`,
    items: others.slice(0, SIBLINGS_MAX),
    more: Math.max(0, others.length - SIBLINGS_MAX),
  }
})

/** 页面滚动的是 App.vue 里的 <main> */
function scroller(): HTMLElement | null {
  return root.value?.closest('main') ?? null
}

/** 返回工具箱时焦点还给谁（元素的 id） */
let returnTo: string | null = null
/** 打开单个工具以前，工具箱滚到了哪里：返回时滚回去 */
let gridScroll = 0

interface OpenOptions {
  /** 返回工具箱时焦点还给谁（元素的 id）；不给就还给这个工具的图标 */
  from?: string
  /** false：焦点由调用的地方安排（跳到某一项测试时） */
  focusTitle?: boolean
}

/** 打开单个工具页，焦点放到它的标题上。系统工具和设置没有单个工具页，返回 false */
async function openDetail(key: string, opts: OpenOptions = {}): Promise<boolean> {
  const item = itemsByKey.value.get(key)
  if (!item || item.category === 'open') return false
  if (!openItem.value) gridScroll = scroller()?.scrollTop ?? 0
  returnTo = opts.from ?? tileId(key)
  openKey.value = key
  rememberRecent(key)
  // 「看信息」的打开就查一次；已经有结果、正在查的不重复查
  const t = item.tool
  if (t?.group === 'info') {
    const s = runs[t.id]
    if (!s?.result && !s?.running) void run(t, { keepFocus: true })
  }
  await nextTick()
  scroller()?.scrollTo({ top: 0 })
  if (opts.focusTitle !== false) detailTitle.value?.focus()
  return true
}

/** 回到工具箱：滚回原来的位置，焦点还给打开它的那个图标（不在了就给这个工具的图标，再不行给页面标题） */
async function back(): Promise<void> {
  const key = openKey.value
  if (!key) return
  const target = returnTo
  returnTo = null
  openKey.value = null
  await nextTick()
  scroller()?.scrollTo({ top: gridScroll })
  const el = (target ? document.getElementById(target) : null) ?? document.getElementById(tileId(key))
  if (el) el.focus()
  else gridTitle.value?.focus()
}

/** 「还有几个」：回到工具箱，只看这一类 */
function showWholeCategory(): void {
  const item = openItem.value
  if (!item) return
  tab.value = item.category
  query.value = ''
  gridScroll = 0
  void back()
}

// ── 从别的页面跳过来 ──

/** 还没处理的跳转（页面还没显示出来、目录还没读好时先记着） */
const pendingLocal = ref<string | null>(null)
const pendingTool = ref<string | null>(null)
const pendingTest = ref<string | null>(null)

const highlighted = ref<string | null>(null)
let highlightTimer: ReturnType<typeof setTimeout> | undefined

function highlight(id: string): void {
  clearTimeout(highlightTimer)
  highlighted.value = id
  highlightTimer = setTimeout(() => (highlighted.value = null), 2500)
}

// 「屏幕、键盘、鼠标、声音测试」里正在标出来的那一项（DeviceTests 里的 device-test-<名字>）
const TEST_PREFIX = 'test:'
const highlightedTest = computed(() =>
  highlighted.value?.startsWith(TEST_PREFIX) ? highlighted.value.slice(TEST_PREFIX.length) : null,
)

async function consumeLocal(): Promise<void> {
  const id = pendingLocal.value
  if (!id || !active) return
  pendingLocal.value = null
  await openDetail(`local:${id}`)
}

async function consumeTool(): Promise<void> {
  const id = pendingTool.value
  if (!id || !active) return
  const t = tools.value.find((x) => x.id === id)
  if (!t) {
    // 目录读好了却没有这个小工具：不用再等
    if (catalog.value) pendingTool.value = null
    return
  }
  pendingTool.value = null
  if (t.group === 'open') await showOpenTool(t)
  else await openDetail(`tool:${t.id}`)
}

async function consumeTest(): Promise<void> {
  const id = pendingTest.value
  if (!id || !active) return
  pendingTest.value = null
  if (!(await openDetail(DEVICE_TESTS_KEY, { focusTitle: false }))) return
  await nextTick()
  const card = document.getElementById(`device-test-${id}`)
  if (!card) {
    detailTitle.value?.focus()
    return
  }
  card.scrollIntoView({ block: 'start' })
  card.focus({ preventScroll: true })
  highlight(TEST_PREFIX + id)
}

async function consumePending(): Promise<void> {
  await consumeLocal()
  await consumeTool()
  await consumeTest()
}

/** 系统工具和设置没有单个工具页：回到工具箱，滚到那个按钮、把焦点放上去、标出来 */
async function showOpenTool(t: ToolSummary): Promise<void> {
  openKey.value = null
  returnTo = null
  if (tab.value !== 'all' && tab.value !== 'open') tab.value = 'open'
  if (found.value && !found.value.has(`tool:${t.id}`)) query.value = ''
  await nextTick()
  const button = document.getElementById(openButtonId(t.id))
  if (button) {
    button.scrollIntoView({ block: 'center' })
    button.focus({ preventScroll: true })
  }
  highlight(t.id)
}

watch(
  () => nav.localToolId,
  (id) => {
    if (!id) return
    nav.localToolId = null
    pendingLocal.value = id
    void consumeLocal()
  },
  { immediate: true },
)

watch(
  () => nav.toolId,
  (id) => {
    if (!id) return
    nav.toolId = null
    pendingTool.value = id
    void consumeTool()
  },
  { immediate: true },
)
// 目录比页面晚读好时再试一次
watch(tools, () => void consumeTool())

watch(
  () => nav.testId,
  (id) => {
    if (!id) return
    nav.testId = null
    pendingTest.value = id
    void consumeTest()
  },
  { immediate: true },
)

// 在单个工具页里又点了导航栏上的「工具箱」：回到图标这里
watch(
  () => nav.home.seq,
  () => {
    if (nav.home.page === 'tools' && openKey.value) void back()
  },
)

onActivated(() => {
  active = true
  void consumePending()
})

onDeactivated(() => {
  active = false
  // 「已经打开了」过一会儿就不是真的了，回来时不再显示
  opener.reset()
  clearTimeout(highlightTimer)
  highlighted.value = null
})

onBeforeUnmount(() => clearTimeout(highlightTimer))

// ── 结果里的「预览修复」 ──

const previewId = ref<string | null>(null)

function onApplied(r: ApplyResult): void {
  if (r.entryIds.length > 0) markHealthStale()
}
</script>

<template>
  <div ref="root" class="page">
    <!-- ───── 工具箱：搜索、分类、一个个图标 ───── -->
    <template v-if="!openItem">
      <header class="page-head">
        <div class="page-head-main">
          <h1 ref="gridTitle" class="page-title" tabindex="-1">工具箱</h1>
          <p class="page-lead">处理文字、图片和文件，测试硬件，一键解决小毛病。都在这台电脑上处理，不上传。</p>
        </div>
        <div class="page-actions">
          <label class="search-box tools-search">
            <AppIcon name="search" :size="16" />
            <input
              v-model="query"
              type="search"
              aria-label="搜索工具"
              placeholder="搜索工具，比如：压缩图片、WiFi 密码"
              autocomplete="off"
            />
          </label>
        </div>
      </header>

      <div role="tablist" aria-label="工具分类" class="tabs" @keydown="onTabKeydown">
        <button
          v-for="t in TABS"
          :id="tabId(t.id)"
          :key="t.id"
          type="button"
          role="tab"
          class="tab"
          :aria-selected="tab === t.id"
          :aria-controls="PANEL_ID"
          :tabindex="tab === t.id ? 0 : -1"
          @click="tab = t.id"
        >
          {{ t.title }}
        </button>
      </div>

      <div :id="PANEL_ID" role="tabpanel" class="panel" :aria-labelledby="tabId(tab)">
        <p class="visually-hidden" role="status">{{ foundText }}</p>

        <section v-if="showRecent" class="recent" aria-labelledby="tools-recent-title">
          <h2 id="tools-recent-title" class="recent-title">最近用过</h2>
          <ul class="recent-list">
            <li v-for="r in recentItems" :key="r.key">
              <button
                :id="recentId(r.key)"
                type="button"
                class="recent-pill"
                @click="openDetail(r.key, { from: recentId(r.key) })"
              >
                <AppIcon :name="r.icon" :size="18" class="recent-icon" />{{ r.title }}
              </button>
            </li>
          </ul>
        </section>

        <template v-for="s in shownSections" :key="s.id">
          <!-- 系统工具和设置：按打开的是什么分几组，点了直接打开 -->
          <section v-if="s.id === 'open'" class="cat" :aria-labelledby="categoryTitleId(s.id)">
            <div class="cat-head">
              <h2 :id="categoryTitleId(s.id)" class="cat-title">{{ s.title }}</h2>
              <span class="muted small">{{ s.items.length }} 个</span>
            </div>
            <div class="open-groups">
              <section
                v-for="g in openGroups"
                :key="g.id"
                class="card open-group"
                :aria-labelledby="`tools-opengroup-${g.id}`"
              >
                <div class="open-group-head">
                  <span class="open-group-icon" aria-hidden="true"><AppIcon :name="g.icon" :size="18" /></span>
                  <h3 :id="`tools-opengroup-${g.id}`" class="open-group-title">{{ g.title }}</h3>
                  <p class="muted small">{{ g.note }}</p>
                </div>
                <ul class="open-list">
                  <li
                    v-for="{ t, open } in g.items"
                    :key="t.id"
                    class="open-item"
                    :class="{ highlight: highlighted === t.id }"
                  >
                    <button
                      :id="openButtonId(t.id)"
                      type="button"
                      class="open-btn"
                      :aria-labelledby="`${openButtonId(t.id)}-name`"
                      :aria-describedby="`${openButtonId(t.id)}-desc`"
                      :title="t.description"
                      @click="opener.open(t)"
                    >
                      <span class="open-text">
                        <span :id="`${openButtonId(t.id)}-name`" class="open-name">
                          <span class="open-title">{{ t.title }}</span>
                          <span v-if="audienceLabel[t.audience]" class="mini-tag">{{ audienceLabel[t.audience] }}</span>
                        </span>
                        <span :id="`${openButtonId(t.id)}-desc`" class="open-desc">{{ t.description }}</span>
                      </span>
                      <BusySpinner v-if="open?.busy" size="small" />
                      <AppIcon v-else name="external" :size="16" class="open-icon" />
                    </button>
                    <p
                      v-if="open && !open.busy"
                      class="open-status small"
                      :class="open.ok ? 'success-text' : 'danger-text'"
                      :role="open.ok ? 'status' : 'alert'"
                    >
                      <AppIcon v-if="open.ok" name="check" :size="14" class="open-status-icon" />{{ open.text }}
                    </p>
                  </li>
                </ul>
              </section>
            </div>
          </section>

          <!-- 其余分类：一个个图标，点了打开单个工具页 -->
          <section v-else class="cat" :aria-labelledby="categoryTitleId(s.id)">
            <div class="cat-head">
              <h2 :id="categoryTitleId(s.id)" class="cat-title">{{ s.title }}</h2>
              <span class="muted small">{{ s.items.length }} 个</span>
              <span v-if="s.note" class="muted small">· {{ s.note }}</span>
            </div>
            <ul class="tiles">
              <li v-for="it in s.items" :key="it.key">
                <button
                  :id="tileId(it.key)"
                  type="button"
                  class="tile"
                  :aria-labelledby="`${tileId(it.key)}-name`"
                  :aria-describedby="`${tileId(it.key)}-desc`"
                  :title="it.description"
                  @click="openDetail(it.key, { from: tileId(it.key) })"
                >
                  <span class="tile-icon" aria-hidden="true"><AppIcon :name="it.icon" :size="21" /></span>
                  <span class="tile-text">
                    <span :id="`${tileId(it.key)}-name`" class="tile-name">
                      <span class="tile-title">{{ it.title }}</span>
                      <span v-if="it.tag" class="mini-tag">{{ it.tag }}</span>
                    </span>
                    <span :id="`${tileId(it.key)}-desc`" class="tile-desc">{{ it.short }}</span>
                  </span>
                  <BusySpinner v-if="it.tool && runs[it.tool.id]?.running" size="small" />
                </button>
              </li>
            </ul>
          </section>
        </template>

        <p v-if="catalogLoading" class="loading-line" role="status"><BusySpinner size="small" />正在读取工具列表…</p>
        <div v-else-if="emptyText" class="card empty" role="status">
          <p class="muted">{{ emptyText }}</p>
          <button v-if="foundElsewhere" type="button" class="btn btn-secondary btn-small" @click="tab = 'all'">
            在全部分类里找
          </button>
        </div>
      </div>
    </template>

    <!-- ───── 单个工具 ───── -->
    <template v-else>
      <nav class="crumbs" aria-label="当前位置">
        <button type="button" class="crumb-link" @click="back"><AppIcon name="back" :size="14" />工具箱</button>
        <span aria-hidden="true">/</span>
        <span>{{ categoryTitle(openItem.category) }}</span>
      </nav>

      <header class="page-head">
        <div class="page-head-main detail-head">
          <span class="detail-icon" aria-hidden="true"><AppIcon :name="openItem.icon" :size="26" /></span>
          <div class="detail-text">
            <h1 ref="detailTitle" class="page-title detail-title" tabindex="-1">
              {{ openItem.title }}
              <TagPill v-if="openItem.tag" tone="neutral">{{ openItem.tag }}</TagPill>
            </h1>
            <p class="page-lead">{{ openItem.description }}</p>
          </div>
        </div>
        <div v-if="detailTool" class="page-actions">
          <button
            type="button"
            class="btn"
            :class="detailRun.result ? 'btn-secondary' : 'btn-primary'"
            :disabled="detailRun.running"
            @click="runDetail"
          >
            <BusySpinner v-if="detailRun.running" size="small" />
            {{ detailButtonText }}
          </button>
        </div>
      </header>

      <!-- 看信息、一键处理：结果显示在这里 -->
      <section
        v-if="detailTool"
        :key="detailTool.id"
        class="card run-card"
        aria-labelledby="tools-run-title"
        :aria-busy="detailRun.running"
      >
        <h2 id="tools-run-title" class="visually-hidden">结果</h2>
        <template v-if="detailTool.group === 'info'">
          <p v-if="detailRun.running && !detailRun.result" class="loading-line" role="status">
            <BusySpinner size="small" />正在读取，一般几秒钟就好…
          </p>
          <div v-if="detailRun.error" class="banner banner-error" role="alert">
            <div>
              <p class="banner-title">没能查看</p>
              <p>{{ detailRun.error }}</p>
            </div>
          </div>
          <ToolInfoResult
            v-if="detailRun.result"
            :result="detailRun.result"
            :running="detailRun.running"
            @preview="(featureId) => (previewId = featureId)"
          />
          <p v-else-if="!detailRun.running && !detailRun.error" class="muted">点「查看」读取。{{ INFO_NOTE }}</p>
        </template>
        <template v-else>
          <p v-if="detailRun.running" class="loading-line" role="status"><BusySpinner size="small" />正在处理…</p>
          <div v-if="detailRun.error" class="banner banner-error" role="alert">
            <div>
              <p class="banner-title">没能完成</p>
              <p>{{ detailRun.error }}</p>
            </div>
          </div>
          <ToolOutcome
            v-if="detailRun.result"
            :result="detailRun.result"
            @preview="(featureId) => (previewId = featureId)"
          />
          <p v-else-if="!detailRun.running && !detailRun.error" class="muted">点「开始」做一次。{{ ACTION_NOTE }}</p>
        </template>
      </section>
    </template>

    <!-- 本地工具：换到别的工具、回到工具箱时都留着（和原来一页全摆着时一样，填的东西、正在做的事不会丢） -->
    <div v-show="detailComponent" class="tool-body">
      <KeepAlive>
        <component
          :is="detailComponent"
          v-if="detailComponent"
          :key="openKey"
          hide-title
          v-bind="openKey === DEVICE_TESTS_KEY ? { highlight: highlightedTest } : {}"
        />
      </KeepAlive>
    </div>

    <nav v-if="openItem && siblings.items.length" class="siblings" aria-labelledby="tools-siblings-title">
      <span id="tools-siblings-title" class="siblings-title">{{ siblings.title }}</span>
      <ul class="chips">
        <li v-for="s in siblings.items" :key="s.key">
          <button type="button" class="chip" @click="openDetail(s.key)">{{ s.title }}</button>
        </li>
        <li v-if="siblings.more > 0">
          <button type="button" class="chip chip-soft" @click="showWholeCategory">还有 {{ siblings.more }} 个</button>
        </li>
      </ul>
    </nav>

    <ConfirmDialog
      v-if="confirmTool"
      :title="`${confirmTool.title}？`"
      :confirm-text="confirmTool.title"
      :busy="confirmBusy"
      @confirm="confirmAction"
      @close="confirmTool = null"
    >
      <p>{{ confirmTool.confirm }}</p>
    </ConfirmDialog>

    <PreviewDialog v-if="previewId" :feature-id="previewId" @close="previewId = null" @applied="onApplied" />
  </div>
</template>

<style scoped>
/* ───── 工具箱首页 ───── */

.tools-search {
  width: 340px;
  max-width: 100%;
}

.tools-search input {
  font-size: var(--text-small);
}

/* 分类：一排圆角按钮，选中的填上主色 */
.tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.tab {
  min-height: 34px;
  padding: 4px 14px;
  border: 1px solid var(--color-border);
  border-radius: 17px;
  background: var(--color-surface);
  color: var(--color-text);
  font-size: var(--text-small);
  white-space: nowrap;
  cursor: pointer;
}

.tab:hover {
  border-color: var(--color-primary);
  color: var(--color-primary-soft-text);
}

.tab[aria-selected='true'] {
  border-color: var(--color-primary);
  background: var(--color-primary);
  color: var(--color-primary-text);
  font-weight: 600;
}

.panel {
  display: flex;
  flex-direction: column;
  gap: 22px;
}

/* 最近用过：左边标题，右边一排小按钮（放不下就在右边换行） */
.recent {
  display: flex;
  align-items: flex-start;
  gap: 10px 16px;
}

.recent-title {
  flex: none;
  color: var(--color-text-muted);
  font-size: var(--text-small);
  font-weight: 600;
  line-height: 36px;
  white-space: nowrap;
}

.recent-list {
  display: flex;
  flex: 1;
  flex-wrap: wrap;
  gap: 10px;
  min-width: 0;
  list-style: none;
}

.recent-pill {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  min-height: 36px;
  padding: 4px 14px 4px 10px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius);
  background: var(--color-surface);
  color: var(--color-text);
  font-size: var(--text-small);
  cursor: pointer;
}

.recent-pill:hover {
  border-color: var(--color-primary);
  color: var(--color-primary-soft-text);
}

.recent-icon {
  color: var(--color-primary);
}

/* 每一类：标题、个数，下面是图标 */
.cat {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.cat-head {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 4px 8px;
}

.cat-title {
  font-size: 16px;
}

/* 图标：窗口宽的时候一行五个，窄了自动少几个 */
.tiles {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
  gap: 12px;
  list-style: none;
}

.tile {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  height: 100%;
  min-height: 72px;
  padding: 10px 12px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  background: var(--color-surface);
  box-shadow: var(--shadow-card);
  color: var(--color-text);
  text-align: left;
  cursor: pointer;
  transition: border-color 0.15s;
}

.tile:hover {
  border-color: var(--color-primary);
}

.tile-icon {
  display: flex;
  flex: none;
  align-items: center;
  justify-content: center;
  width: 38px;
  height: 38px;
  border-radius: var(--radius);
  background: var(--color-primary-soft);
  color: var(--color-primary-soft-text);
}

.tile-text {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.tile-name,
.open-name {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

/* 名字和一句话放不下时各自最多两行，同一排的图标一样高；完整的说明在鼠标停上去的提示里 */
.tile-title,
.tile-desc,
.open-title,
.open-desc {
  display: -webkit-box;
  overflow: hidden;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  line-clamp: 2;
}

.tile-title {
  font-size: 14.5px;
  font-weight: 600;
  line-height: 1.4;
}

.tile-desc {
  color: var(--color-text-muted);
  font-size: 12.5px;
  line-height: 1.4;
}

/* 「给懂哥」：图标、按钮里用的小标签 */
.mini-tag {
  flex: none;
  padding: 0 6px;
  border-radius: 999px;
  background: var(--tone-neutral-bg);
  color: var(--tone-neutral-text);
  font-size: 11.5px;
  font-weight: 600;
  line-height: 1.6;
  white-space: nowrap;
}

/* 系统工具和设置：每组一张卡片，里面几列按钮 */
.open-groups {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.open-group {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.open-group-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px 10px;
}

.open-group-icon {
  display: flex;
  flex: none;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border-radius: var(--radius-sm);
  background: var(--color-primary-soft);
  color: var(--color-primary-soft-text);
}

.open-group-title {
  font-size: var(--text-base);
}

.open-list {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 4px 12px;
  list-style: none;
}

.open-item {
  display: flex;
  flex-direction: column;
  min-width: 0;
  border: 1px solid transparent;
  border-radius: var(--radius);
  scroll-margin: 16px;
  transition:
    border-color 0.2s,
    box-shadow 0.2s;
}

/* 从别的页面跳过来时，标出是哪一个 */
.open-item.highlight {
  border-color: var(--color-primary);
  box-shadow: 0 0 0 3px var(--color-primary-soft);
}

.open-btn {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 8px 10px;
  border: none;
  border-radius: var(--radius);
  background: transparent;
  color: var(--color-text);
  text-align: left;
  cursor: pointer;
}

.open-btn:hover {
  background: var(--color-surface-2);
}

/* 按钮排得紧：焦点框贴着按钮画，不压到旁边那一个 */
.open-btn:focus-visible {
  outline-offset: 0;
}

.open-text {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.open-title {
  font-weight: 600;
  line-height: 1.4;
}

/* 没有单个工具页，说明只能在这里看 */
.open-desc {
  color: var(--color-text-muted);
  font-size: 12.5px;
  line-height: 1.45;
}

.open-icon {
  color: var(--color-text-muted);
}

.open-status {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  padding: 0 10px 8px;
}

.open-status-icon {
  margin-top: 3px;
}

.empty {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 10px;
}

/* ───── 单个工具页 ───── */

.detail-head {
  flex-direction: row;
  align-items: center;
  gap: 14px;
}

.detail-icon {
  display: flex;
  flex: none;
  align-items: center;
  justify-content: center;
  width: 48px;
  height: 48px;
  border-radius: var(--radius-lg);
  background: var(--color-primary-soft);
  color: var(--color-primary-soft-text);
}

.detail-text {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.detail-title {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}

.run-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.tool-body {
  min-width: 0;
}

/* 最下面「其他文字工具」那一排：左边一句话，右边的小按钮放不下就在右边换行 */
.siblings {
  display: flex;
  align-items: flex-start;
  gap: 8px 14px;
  padding-top: 14px;
  border-top: 1px solid var(--color-border);
}

.siblings-title {
  flex: none;
  color: var(--color-text-muted);
  font-size: var(--text-small);
  line-height: 32px;
  white-space: nowrap;
}

.siblings .chips {
  flex: 1;
  min-width: 0;
  list-style: none;
}
</style>
