import { computed, reactive, ref } from 'vue'
import { catalogSummary, systemInfo } from './api'
import type { CatalogSummary, FeatureSummary, SymptomSummary, SystemInfo } from './api/types'
import { errorText } from './utils/format'

// 整个界面共用的一点状态。页面不多，用不着路由和状态管理库。

export type PageId = 'health' | 'symptoms' | 'settings' | 'journal' | 'report'

export const nav = reactive<{ page: PageId; symptomId: string | null }>({
  page: 'health',
  symptomId: null,
})

export function goTo(page: PageId): void {
  nav.page = page
}

/** 跳到「按症状修」并打开某个症状（体检结果里的「去修」按钮用） */
export function openSymptom(id: string): void {
  nav.symptomId = id
  nav.page = 'symptoms'
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
}>({ lastRunAt: null, stale: false, runRequest: 0 })

/** 症状页、常用设置、修改日志里改过或撤销过东西以后调用 */
export function markHealthStale(): void {
  health.stale = true
}

/** 跳到体检页并马上开始体检（报告页的「重新体检」用） */
export function requestHealthCheck(): void {
  nav.page = 'health'
  health.runRequest++
}
