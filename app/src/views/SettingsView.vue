<script setup lang="ts">
import { computed, onActivated, reactive, ref, watch } from 'vue'
import { featureDetect, journalList } from '../api'
import type { ApplyResult, FeatureState, FeatureSummary } from '../api/types'
import BulkApplyDialog from '../components/BulkApplyDialog.vue'
import BusySpinner from '../components/BusySpinner.vue'
import ContextMenuManager from '../components/ContextMenuManager.vue'
import PreviewDialog from '../components/PreviewDialog.vue'
import TagPill from '../components/TagPill.vue'
import {
  featureStateLabel,
  featureStateTone,
  recommendLabel,
  recommendTone,
  rebootLabel,
  settingsCategories,
} from '../labels'
import { catalog, goTo, markHealthStale } from '../state'
import { errorText } from '../utils/format'

// 常用设置：只列资源管理器、桌面、任务栏、开始菜单、推荐和广告、键盘和鼠标、电源这几类。每次进入页面都重新检测一遍当前状态。
// 最下面是右键菜单里软件加的项目（ContextMenuManager，点了才列）。
// 这台电脑用不了的项（系统版本不对等）照样列出来，但只说明原因：不检测、不给执行、不放进「只应用推荐项」。

const categoryIds = new Set<string>(settingsCategories.map((c) => c.id))

const features = computed<FeatureSummary[]>(() => (catalog.value?.features ?? []).filter((f) => categoryIds.has(f.category)))
/** 这台电脑能用的项 */
const usable = computed(() => features.value.filter((f) => f.applicable))

const groups = computed(() =>
  settingsCategories
    .map((c) => ({ ...c, features: features.value.filter((f) => f.category === c.id) }))
    .filter((g) => g.features.length > 0),
)

// ── 当前状态 ──

interface Detect {
  loading: boolean
  state: FeatureState | null
  error: string | null
  /** 只认最后一次检测的结果 */
  seq: number
}

const detects = reactive<Record<string, Detect>>({})

function detectEntry(id: string): Detect {
  let d = detects[id]
  if (!d) {
    detects[id] = { loading: false, state: null, error: null, seq: 0 }
    // 重新读一次，拿到的才是响应式的对象
    d = detects[id]!
  }
  return d
}

async function detectOne(id: string): Promise<void> {
  const d = detectEntry(id)
  const seq = ++d.seq
  d.loading = true
  try {
    const state = await featureDetect(id)
    if (seq !== d.seq) return
    d.state = state
    d.error = null
  } catch (e) {
    if (seq !== d.seq) return
    d.error = errorText(e)
  } finally {
    if (seq === d.seq) d.loading = false
  }
}

function detectAll(): void {
  for (const f of usable.value) void detectOne(f.id)
}

// ── 哪些项是小药箱改的（修改日志里有还能恢复的记录） ──
// 「已经设置好」可能是小药箱改的，也可能是电脑原来就这样；只有前一种能去修改日志里恢复。

/** 修改日志里有还能恢复的记录的功能；null 表示还没读出来（或读不出来），这时只能笼统地说 */
const restorable = ref<Set<string> | null>(null)
let restorableSeq = 0

async function loadRestorable(): Promise<void> {
  const seq = ++restorableSeq
  try {
    const sessions = await journalList()
    if (seq !== restorableSeq) return
    restorable.value = new Set(sessions.flatMap((s) => s.entries.filter((e) => e.canUndo).map((e) => e.feature)))
  } catch {
    if (seq === restorableSeq) restorable.value = null
  }
}

function refresh(): void {
  detectAll()
  void loadRestorable()
}

onActivated(refresh)
// 目录比页面晚加载好时，补一次检测
watch(features, detectAll)

const detecting = computed(() => usable.value.some((f) => detects[f.id]?.loading ?? true))

/** details 是逐个位置的技术说明（注册表路径等），只在没全设置好、查不出来或出错时才有看的必要 */
function showDetails(s: FeatureState): boolean {
  return s.details.length > 0 && (s.state === 'partial' || s.state === 'unknown' || s.error !== null)
}

/** 模板里用：每个分组、每一项和它的检测结果 */
const rows = computed(() =>
  groups.value.map((g) => ({
    ...g,
    items: g.features.map((f) => {
      const d = detects[f.id]
      return { f, loading: d?.loading ?? true, state: d?.state ?? null, error: d?.error ?? null }
    }),
  })),
)

function stateOf(id: string) {
  return detects[id]?.state?.state ?? null
}

/** 「已经设置好」下面那句话，和要不要给「去修改日志」按钮 */
function appliedHint(id: string): { text: string; journal: boolean } {
  const set = restorable.value
  if (set === null) {
    return {
      text: '已经设置好了。如果是小药箱改的，可以到「修改日志」里恢复原状；如果本来就是这样，说明这是电脑原来的设置。',
      journal: true,
    }
  }
  if (set.has(id)) return { text: '已经设置好了，是小药箱改的。想改回去，可以到「修改日志」里恢复原状。', journal: true }
  return { text: '已经设置好了。修改日志里没有小药箱改这一项的记录，这是电脑原来的设置。', journal: false }
}

// ── 只应用推荐项 ──

/** 推荐、这台电脑能用、还没设置好的项目。「谨慎」级的要单独预览确认，不放进来 */
const pendingRecommended = computed(() =>
  usable.value.filter((f) => f.recommend === 'recommended' && f.risk !== 'danger' && stateOf(f.id) !== 'applied'),
)

const bulkOpen = ref(false)

const bulkHint = computed(() => {
  if (!catalog.value) return ''
  if (detecting.value) return '正在检测当前状态…'
  if (pendingRecommended.value.length === 0) return '推荐的设置都已经设置好了。'
  return `有 ${pendingRecommended.value.length} 项推荐设置还没设置好。`
})

function onBulkFinished(changed: boolean): void {
  if (changed) markHealthStale()
  refresh()
}

// ── 单项预览 ──

const previewId = ref<string | null>(null)

function onApplied(r: ApplyResult): void {
  if (r.entryIds.length > 0) markHealthStale()
  if (previewId.value) void detectOne(previewId.value)
  void loadRestorable()
}
</script>

<template>
  <div class="page">
    <header class="page-header">
      <h1 class="page-title" tabindex="-1">常用设置</h1>
      <p class="page-lead">
        一些常见的系统设置开关。小药箱改过的每一项都能在「修改日志」里恢复原状；标着「个人偏好」的看自己习惯，不改也没关系。
      </p>
    </header>

    <div class="toolbar card">
      <button
        type="button"
        class="btn btn-primary"
        :disabled="detecting || pendingRecommended.length === 0"
        @click="bulkOpen = true"
      >
        只应用推荐项
      </button>
      <p class="muted" role="status">{{ bulkHint }}</p>
    </div>

    <p v-if="!catalog" class="loading-line" role="status"><BusySpinner size="small" />正在读取设置列表…</p>

    <section v-for="g in rows" :key="g.id" class="group" :aria-labelledby="`settings-${g.id}`">
      <h2 :id="`settings-${g.id}`" class="section-title">{{ g.title }}</h2>
      <ul class="items">
        <li v-for="{ f, loading, state, error } in g.items" :key="f.id" class="item card">
          <div class="item-main">
            <div class="item-head">
              <h3 class="item-title">{{ f.title }}</h3>
              <TagPill :tone="recommendTone[f.recommend]">{{ recommendLabel[f.recommend] }}</TagPill>
              <TagPill v-if="f.subjective" tone="neutral">个人偏好</TagPill>
            </div>
            <p class="item-desc">{{ f.description }}</p>
            <p v-if="!f.applicable" class="item-state small muted">
              这台电脑用不了：{{ f.notApplicableReason ?? '这台电脑的系统不支持这一项。' }}
            </p>
            <template v-else>
              <p class="item-state small">
                <span class="muted">当前：</span>
                <TagPill v-if="state" :tone="featureStateTone[state.state]" dot>{{ featureStateLabel[state.state] }}</TagPill>
                <span v-else-if="error" class="muted">没检测出来</span>
                <span v-else class="muted">检测中…</span>
                <BusySpinner v-if="loading && state" size="small" />
                <span v-if="f.reboot !== 'none'" class="muted">· 改完{{ rebootLabel[f.reboot] }}</span>
              </p>
              <ul v-if="state && showDetails(state)" class="details small muted">
                <li v-for="(d, i) in state.details" :key="i">{{ d }}</li>
              </ul>
              <p v-if="state?.error" class="small muted">{{ state.error }}</p>
              <p v-if="error" class="small muted">{{ error }}</p>
            </template>
          </div>
          <div v-if="f.applicable" class="item-action">
            <template v-if="state?.state === 'applied'">
              <p class="small muted">{{ appliedHint(f.id).text }}</p>
              <button
                v-if="appliedHint(f.id).journal"
                type="button"
                class="btn btn-secondary btn-small"
                @click="goTo('journal')"
              >
                去修改日志
              </button>
            </template>
            <button v-else type="button" class="btn btn-secondary" @click="previewId = f.id">预览并设置</button>
          </div>
        </li>
      </ul>
    </section>

    <ContextMenuManager v-if="catalog" />

    <PreviewDialog v-if="previewId" :feature-id="previewId" @close="previewId = null" @applied="onApplied" />
    <BulkApplyDialog
      v-if="bulkOpen"
      :features="pendingRecommended"
      @close="bulkOpen = false"
      @finished="onBulkFinished"
    />
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px 16px;
}

.group {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.items {
  display: flex;
  flex-direction: column;
  gap: 10px;
  list-style: none;
}

.item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 20px;
}

.item-main {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.item-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}

.item-title {
  font-size: var(--text-large);
}

.item-state {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  margin-top: 2px;
}

.details {
  padding-left: 1.3em;
}

.item-action {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 4px;
  flex: none;
  max-width: 220px;
  text-align: right;
}
</style>
