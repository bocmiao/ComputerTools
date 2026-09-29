import { computed, reactive, ref } from 'vue'
import { catalogSummary, systemInfo } from './api'
import type { CatalogSummary, FeatureSummary, SymptomSummary, SystemInfo, ToolSummary } from './api/types'
import { errorText } from './utils/format'

// 整个界面共用的一点状态。页面不多，用不着路由和状态管理库。

export type PageId = 'health' | 'symptoms' | 'settings' | 'tools' | 'journal' | 'report'

export const nav = reactive<{
  page: PageId
  symptomId: string | null
  toolId: string | null
  testId: string | null
  /** 常用设置里要定位、展开的那一项（导航栏搜索用） */
  featureId: string | null
  /** 工具箱里要打开的本地工具（utils/localTools.ts 里的 id） */
  localToolId: string | null
  /** 回到某一页的首页（症状详情、单个工具页回到列表），seq 每次加一：见 goHome */
  home: { page: PageId | null; seq: number }
}>({
  page: 'health',
  symptomId: null,
  toolId: null,
  testId: null,
  featureId: null,
  localToolId: null,
  home: { page: null, seq: 0 },
})

export function goTo(page: PageId): void {
  nav.page = page
}

/**
 * 到某一页的首页：在症状详情、单个工具页里又点了导航栏上的这一页（和手机上再点一下当前标签一样），
 * 或者「全部 N 个症状」这类链接。页面自己盯着 nav.home，回到列表
 */
export function goHome(page: PageId): void {
  nav.home = { page, seq: nav.home.seq + 1 }
  nav.page = page
}

/** 跳到「按症状修」并打开某个症状（体检结果里的「去修」按钮用） */
export function openSymptom(id: string): void {
  nav.symptomId = id
  nav.page = 'symptoms'
}

/**
 * 跳到「小工具」并定位到某个小工具（检测结果里 tool: 链接的按钮用）。
 * 小工具页会滚到这张卡片、把焦点放上去；「看信息」的小工具顺便查一次。
 */
export function openTool(id: string): void {
  nav.toolId = id
  nav.page = 'tools'
}

/** 跳到「常用设置」并定位、展开某一项（导航栏搜索用） */
export function openSetting(id: string): void {
  nav.featureId = id
  nav.page = 'settings'
}

/** 跳到「工具箱」并打开某个本地工具（导航栏搜索、最近用过用） */
export function openLocalTool(id: string): void {
  nav.localToolId = id
  nav.page = 'tools'
}

/** 跳到「小工具」页的「屏幕、键盘、鼠标、声音测试」里的一项（症状指引里 test: 链接的按钮用，修完试一试） */
export function openTest(id: string): void {
  nav.testId = id
  nav.page = 'tools'
}

// ── 功能目录 ──

export const catalog = ref<CatalogSummary | null>(null)
export const catalogError = ref<string | null>(null)
let catalogLoading: Promise<void> | null = null

export function loadCatalog(): Promise<void> {
  catalogLoading ??= catalogSummary()
    .then((c) => {
      catalog.value = c
      catalogError.value = null
    })
    .catch((e: unknown) => {
      catalogError.value = errorText(e)
    })
    .finally(() => {
      catalogLoading = null
    })
  return catalogLoading
}

export function findFeature(id: string): FeatureSummary | undefined {
  return catalog.value?.features.find((f) => f.id === id)
}

export function findSymptom(id: string): SymptomSummary | undefined {
  return catalog.value?.symptoms.find((s) => s.id === id)
}

/** 目录里的小工具（后端比界面旧、没有 tools 时当作没有） */
export function catalogTools(): ToolSummary[] {
  return catalog.value?.tools ?? []
}

export function findTool(id: string): ToolSummary | undefined {
  return catalogTools().find((t) => t.id === id)
}

/** 改完「需要重启资源管理器」的设置以后，对话框里直接给这个小工具的按钮 */
export const RESTART_EXPLORER_TOOL = 'system.restart-explorer'

// ── 系统信息 ──

export const system = ref<SystemInfo | null>(null)
export const systemError = ref<string | null>(null)

export async function loadSystemInfo(): Promise<void> {
  try {
    system.value = await systemInfo()
    systemError.value = null
  } catch (e) {
    systemError.value = errorText(e)
  }
}

// ── 引擎没能启动 ──

/**
 * 系统信息或功能目录读不出来：多半是引擎没能启动（后端启动时组装一次引擎，失败后每个命令都返回同一个原因）。
 * 这时各个页面都用不了，界面只显示一个全局错误页；重试没有用，只能重开小药箱或重启电脑。
 */
export const startupError = computed(() => systemError.value ?? catalogError.value)

// ── 体检结果是不是还算数 ──

export const health = reactive<{
  /** 最近一次完整体检的时间（ISO 字符串）；还没体检过为 null */
  lastRunAt: string | null
  /** 体检以后，又在别的页面改过或撤销过设置：体检结果可能已经过期 */
  stale: boolean
  /** 别的页面请求「重新体检」时加一，体检页据此开始体检 */
  runRequest: number
  /** 最近一次体检里要处理的项目有几个（导航栏「体检」旁边的数字）；还没体检过为 null */
  attention: number | null
}>({ lastRunAt: null, stale: false, runRequest: 0, attention: null })

/** 症状页、常用设置、修改日志里改过或撤销过东西以后调用 */
export function markHealthStale(): void {
  health.stale = true
}

/** 跳到体检页并马上开始体检（报告页的「重新体检」用） */
export function requestHealthCheck(): void {
  nav.page = 'health'
  health.runRequest++
}
