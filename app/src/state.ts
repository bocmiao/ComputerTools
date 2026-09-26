import { reactive, ref } from 'vue'
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
