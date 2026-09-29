import type { CatalogSummary, SymptomSummary } from '../api/types'
import { settingsCategories } from '../labels'
import { LOCAL_TOOLS } from './localTools'
import { normalizeForSearch } from './symptomMatch'

// 导航栏的搜索：一个框搜症状、常用设置、工具箱里的工具（本地工具和目录里的小工具）。
// 名字一模一样的排最前，名字开头对上的其次，再是名字里有的，只有关键词、说明对上的再往后；同样的按症状、设置、工具的顺序。

export type SearchKind = 'symptom' | 'setting' | 'local' | 'tool'

export interface SearchHit {
  kind: SearchKind
  id: string
  title: string
}

/** 症状按标题和关键词搜：「电脑没网了」里包含关键词「没网」也算 */
export function symptomMatches(s: SymptomSummary, q: string): boolean {
  const nq = normalizeForSearch(q)
  if (!nq) return true
  return [s.title, ...s.keywords]
    .map(normalizeForSearch)
    .filter((h) => h.length > 0)
    .some((h) => h.includes(nq) || (h.length >= 2 && nq.includes(h)))
}

/** 0：名字一模一样；1：名字开头对上；2：名字里有；3：关键词、说明对上；-1：没对上 */
function score(nq: string, title: string, extra: string[]): number {
  const t = normalizeForSearch(title)
  if (t === nq) return 0
  if (t.startsWith(nq)) return 1
  if (t.includes(nq)) return 2
  const hit = extra
    .map(normalizeForSearch)
    .some((e) => e.length > 0 && (e.includes(nq) || (e.length >= 2 && nq.includes(e))))
  return hit ? 3 : -1
}

const KIND_ORDER: Record<SearchKind, number> = { symptom: 0, setting: 1, local: 2, tool: 3 }

export function searchAll(q: string, catalog: CatalogSummary | null, limit = 8): SearchHit[] {
  const nq = normalizeForSearch(q)
  if (!nq) return []
  const found: { hit: SearchHit; score: number; order: number }[] = []
  const add = (hit: SearchHit, s: number): void => {
    if (s >= 0) found.push({ hit, score: s, order: found.length })
  }
  for (const s of catalog?.symptoms ?? []) {
    add({ kind: 'symptom', id: s.id, title: s.title }, score(nq, s.title, s.keywords))
  }
  const settingIds = new Set<string>(settingsCategories.map((c) => c.id))
  for (const f of catalog?.features ?? []) {
    if (settingIds.has(f.category)) add({ kind: 'setting', id: f.id, title: f.title }, score(nq, f.title, []))
  }
  for (const t of LOCAL_TOOLS) {
    add({ kind: 'local', id: t.id, title: t.title }, score(nq, t.title, [t.short, ...t.keywords]))
  }
  for (const t of catalog?.tools ?? []) {
    add({ kind: 'tool', id: t.id, title: t.title }, score(nq, t.title, []))
  }
  return found
    .sort((a, b) => a.score - b.score || KIND_ORDER[a.hit.kind] - KIND_ORDER[b.hit.kind] || a.order - b.order)
    .slice(0, limit)
    .map((f) => f.hit)
}
