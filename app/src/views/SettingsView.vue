<script setup lang="ts">
import { computed, nextTick, onActivated, reactive, ref, watch } from 'vue'
import { featureDetect, journalList, journalUndo } from '../api'
import type { ApplyResult, FeatureState, FeatureSummary, JournalEntryView, Reboot } from '../api/types'
import AppIcon from '../components/AppIcon.vue'
import BulkApplyDialog from '../components/BulkApplyDialog.vue'
import BusySpinner from '../components/BusySpinner.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import ContextMenuManager from '../components/ContextMenuManager.vue'
import ExplorerRestart from '../components/ExplorerRestart.vue'
import KeyRemapManager from '../components/KeyRemapManager.vue'
import NewMenuManager from '../components/NewMenuManager.vue'
import PreviewDialog from '../components/PreviewDialog.vue'
import ShellPlacesManager from '../components/ShellPlacesManager.vue'
import TagPill from '../components/TagPill.vue'
import { rebootLabel, settingsCategories } from '../labels'
import { catalog, goTo, markHealthStale, nav } from '../state'
import { errorText, normalizeForSearch } from '../utils/format'

// 常用设置：左边分类栏，右边这一类的设置，每项一行：名字、一句话、开关（和 Windows「设置」里的一样，旁边写「开」「关」）。
// - 打开开关：先弹出预览，看清楚会改什么、要不要重启、能不能撤销，确认后才改（PreviewDialog）；
// - 关掉开关：只有小药箱改的才能关（修改日志里有还能恢复的记录），问一下再恢复原状；电脑原来就是这样的，开关不能点，说明原因；
// - 点一行后面的箭头展开：完整说明、改完要做什么、能不能撤销、当前状态的细节。
// 顶上能搜、能只看推荐的或还没设置的；「应用推荐的 N 项」一次把推荐、这台电脑能用、还没设置好的都设置好。
// 分类栏最下面是右键菜单、「新建」菜单、资源管理器里多出来的图标、改键，点了在右边显示。
// 这台电脑用不了的项（系统版本不对等）照样列出来，但只说明原因：不检测、不给执行、不放进「应用推荐的」。

type Filter = 'all' | 'recommended' | 'pending'
type ManagerId = 'context-menu' | 'new-menu' | 'shell-places' | 'key-remap'
type PanelId = string | ManagerId

const MANAGERS: { id: ManagerId; title: string }[] = [
  { id: 'context-menu', title: '右键菜单里的软件' },
  { id: 'new-menu', title: '右键「新建」菜单' },
  { id: 'shell-places', title: '多出来的图标' },
  { id: 'key-remap', title: '改键' },
]

const categoryIds = new Set<string>(settingsCategories.map((c) => c.id))

const features = computed<FeatureSummary[]>(() => (catalog.value?.features ?? []).filter((f) => categoryIds.has(f.category)))
/** 这台电脑能用的项 */
const usable = computed(() => features.value.filter((f) => f.applicable))

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
// 「开」可能是小药箱改的，也可能是电脑原来就这样；只有前一种能关（恢复原状）。

/** 功能 → 还能恢复的记录（新的在前）；null 表示还没读出来（或读不出来） */
const restorable = ref<Map<string, JournalEntryView[]> | null>(null)
let restorableSeq = 0

async function loadRestorable(): Promise<void> {
  const seq = ++restorableSeq
  try {
    const sessions = await journalList()
    if (seq !== restorableSeq) return
    const map = new Map<string, JournalEntryView[]>()
    const entries = sessions.flatMap((s) => s.entries).filter((e) => e.canUndo)
    entries.sort((a, b) => b.time.localeCompare(a.time))
    for (const e of entries) map.set(e.feature, [...(map.get(e.feature) ?? []), e])
    restorable.value = map
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

function stateOf(id: string) {
  return detects[id]?.state?.state ?? null
}

// ── 分类栏、搜索、筛选 ──

const panel = ref<PanelId>(settingsCategories[0].id)
const query = ref('')
const filter = ref<Filter>('all')

const counts = computed(() => ({
  all: features.value.length,
  recommended: features.value.filter((f) => f.recommend === 'recommended').length,
  pending: usable.value.filter((f) => stateOf(f.id) !== 'applied').length,
}))

function categoryCount(id: string): number {
  return features.value.filter((f) => f.category === id).length
}

const isManager = computed(() => MANAGERS.some((m) => m.id === panel.value))
/** 在搜或在筛选：右边列出所有分类里对得上的 */
const narrowing = computed(() => query.value.trim() !== '' || filter.value !== 'all')

/** 每一行右边的小字：改完什么时候生效 */
const rebootWhen: Record<Reboot, string> = {
  none: '',
  explorer: '重启资源管理器后生效',
  logoff: '注销后生效',
  reboot: '重启电脑后生效',
}

function matchesQuery(f: FeatureSummary): boolean {
  const q = normalizeForSearch(query.value)
  if (!q) return true
  return [f.title, f.description].some((t) => normalizeForSearch(t).includes(q))
}

function matchesFilter(f: FeatureSummary): boolean {
  if (filter.value === 'recommended') return f.recommend === 'recommended'
  if (filter.value === 'pending') return f.applicable && stateOf(f.id) !== 'applied'
  return true
}

/** 推荐的排在前面，别的保持目录里的顺序 */
function ordered(list: FeatureSummary[]): FeatureSummary[] {
  return list
    .map((f, i) => ({ f, i }))
    .sort((a, b) => Number(b.f.recommend === 'recommended') - Number(a.f.recommend === 'recommended') || a.i - b.i)
    .map((x) => x.f)
}

const shownFeatures = computed(() => {
  if (narrowing.value) return ordered(features.value.filter((f) => matchesQuery(f) && matchesFilter(f)))
  return ordered(features.value.filter((f) => f.category === panel.value))
})

const panelTitle = computed(() => {
  if (narrowing.value) return query.value.trim() ? `搜到 ${shownFeatures.value.length} 项` : `${filterLabel.value} ${shownFeatures.value.length} 项`
  return settingsCategories.find((c) => c.id === panel.value)?.title ?? ''
})

const filterLabel = computed(() => ({ all: '全部', recommended: '推荐的', pending: '还没设置的' })[filter.value])

const panelNote = computed(() => {
  if (narrowing.value) return '所有分类里对得上的'
  const list = shownFeatures.value
  const done = list.filter((f) => stateOf(f.id) === 'applied').length
  return `${list.length} 项 · 已经设置好 ${done} 项 · 推荐的排在前面`
})

function categoryTitle(id: string): string {
  return settingsCategories.find((c) => c.id === id)?.title ?? ''
}

function choosePanel(id: PanelId): void {
  panel.value = id
  query.value = ''
  filter.value = 'all'
}

// ── 每一行 ──

const expanded = ref<Set<string>>(new Set())

function toggleExpand(id: string): void {
  const next = new Set(expanded.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  expanded.value = next
}

/** 这一行开关的状态和说明 */
function rowInfo(f: FeatureSummary) {
  const d = detects[f.id]
  const state = d?.state?.state ?? null
  const on = state === 'applied'
  const entries = restorable.value?.get(f.id) ?? []
  // 开着、但不是小药箱改的（或者还没读出修改日志）：不能关
  const locked = on && entries.length === 0
  let note = ''
  if (!f.applicable) note = `这台电脑用不了：${f.notApplicableReason ?? '这台电脑的系统不支持这一项。'}`
  else if (d?.error) note = '没检测出来现在是什么样'
  else if (state === 'partial') note = '只设置好了一部分，打开开关补全'
  else if (state === 'unknown') note = '没查出来现在是什么样'
  else if (locked) note = restorable.value === null ? '已经是这样了' : '电脑原来就是这样的，小药箱没改过'
  return { state, on, loading: d?.loading ?? true, locked, note, entries, error: d?.error ?? null, details: d?.state?.details ?? [] }
}

// ── 打开：预览后执行 ──

const previewId = ref<string | null>(null)
/** 这一页里改过「需要重启资源管理器」的设置：顶上提醒一下 */
const needsExplorer = ref(false)

function onApplied(r: ApplyResult): void {
  if (r.entryIds.length > 0) markHealthStale()
  if (r.ok && r.reboot === 'explorer' && r.entryIds.length > 0) needsExplorer.value = true
  if (previewId.value) void detectOne(previewId.value)
  void loadRestorable()
}

// ── 关掉：恢复原状（只有小药箱改的） ──

const undoTarget = ref<FeatureSummary | null>(null)
const undoBusy = ref(false)
const undoNotice = ref<{ ok: boolean; text: string; journal?: boolean } | null>(null)

function clickSwitch(f: FeatureSummary): void {
  const info = rowInfo(f)
  undoNotice.value = null
  if (!f.applicable || info.loading || info.locked) return
  if (info.on) undoTarget.value = f
  else previewId.value = f.id
}

async function confirmUndo(): Promise<void> {
  const f = undoTarget.value
  if (!f) return
  undoBusy.value = true
  let notice: { ok: boolean; text: string; journal?: boolean } = { ok: true, text: `「${f.title}」已经改回原来的样子。` }
  try {
    for (const e of rowInfo(f).entries) {
      const r = await journalUndo(e.id, false)
      if (r.drift) {
        notice = { ok: false, text: `「${f.title}」在小药箱以外又被改过，要恢复的话请到「修改日志」里处理。`, journal: true }
        break
      }
      if (!r.ok) {
        notice = { ok: false, text: [r.message, r.error].filter(Boolean).join(' ') || '没能改回去。', journal: true }
        break
      }
      if (r.reboot === 'explorer') needsExplorer.value = true
    }
    markHealthStale()
  } catch (e) {
    notice = { ok: false, text: `没能改回去：${errorText(e)}`, journal: true }
  }
  undoBusy.value = false
  undoTarget.value = null
  undoNotice.value = notice
  void detectOne(f.id)
  void loadRestorable()
}

// ── 应用推荐的 ──

/** 推荐、这台电脑能用、还没设置好的项目。「谨慎」级的要单独预览确认，不放进来 */
const pendingRecommended = computed(() =>
  usable.value.filter((f) => f.recommend === 'recommended' && f.risk !== 'danger' && stateOf(f.id) !== 'applied'),
)

const bulkOpen = ref(false)

const bulkLabel = computed(() => {
  if (detecting.value) return '正在检测…'
  if (pendingRecommended.value.length === 0) return '推荐的都设置好了'
  return `应用推荐的 ${pendingRecommended.value.length} 项`
})

function onBulkFinished(changed: boolean): void {
  if (changed) markHealthStale()
  refresh()
}

// ── 从导航栏的搜索跳过来：定位到某一项并展开 ──

async function focusFeature(id: string): Promise<void> {
  const f = features.value.find((x) => x.id === id)
  if (!f) return
  query.value = ''
  filter.value = 'all'
  panel.value = f.category
  expanded.value = new Set([id])
  await nextTick()
  const row = document.getElementById(`setting-${id}`)
  row?.scrollIntoView({ block: 'center' })
  row?.focus({ preventScroll: true })
}

watch(
  [() => nav.featureId, features],
  ([id]) => {
    if (!id || features.value.length === 0) return
    nav.featureId = null
    void focusFeature(id)
  },
  { immediate: true },
)
</script>

<template>
  <div class="page">
    <header class="page-head">
      <div class="page-head-main">
        <h1 class="page-title" tabindex="-1">常用设置</h1>
        <p class="page-lead">常见的系统开关。打开前先预览会改什么，改完能在「修改日志」里撤销。</p>
      </div>
      <div class="page-actions">
        <button
          type="button"
          class="btn btn-primary"
          :disabled="detecting || pendingRecommended.length === 0"
          @click="bulkOpen = true"
        >
          <AppIcon name="check" :size="16" />{{ bulkLabel }}
        </button>
      </div>
    </header>

    <div class="toolbar">
      <label class="search-box toolbar-search">
        <AppIcon name="search" :size="16" />
        <span class="visually-hidden">搜索设置</span>
        <input v-model="query" type="search" placeholder="搜索设置，比如：扩展名、右键菜单" autocomplete="off" />
      </label>
      <div class="seg" role="group" aria-label="只看">
        <button type="button" class="seg-btn" :aria-pressed="filter === 'all'" @click="filter = 'all'">全部 {{ counts.all }}</button>
        <button type="button" class="seg-btn" :aria-pressed="filter === 'recommended'" @click="filter = 'recommended'">
          推荐 {{ counts.recommended }}
        </button>
        <button type="button" class="seg-btn" :aria-pressed="filter === 'pending'" @click="filter = 'pending'">
          还没设置 {{ counts.pending }}
        </button>
      </div>
    </div>

    <div v-if="needsExplorer" class="banner banner-warning explorer" role="status">
      <AppIcon name="refresh" :size="20" />
      <div class="explorer-text">
        <p class="banner-title">有改动要重启资源管理器才生效</p>
        <p class="small">重启时桌面和任务栏会闪一下，打开的文件夹窗口会关掉；也可以等下次注销、重启电脑时自动生效。</p>
        <ExplorerRestart />
      </div>
    </div>

    <p v-if="undoNotice" class="banner" :class="undoNotice.ok ? 'banner-ok' : 'banner-warning'" role="status">
      <span class="notice-text">{{ undoNotice.text }}</span>
      <button v-if="undoNotice.journal" type="button" class="btn btn-secondary btn-small" @click="goTo('journal')">去修改日志</button>
    </p>

    <p v-if="!catalog" class="loading-line" role="status"><BusySpinner size="small" />正在读取设置列表…</p>

    <div v-else class="layout">
      <nav class="rail" aria-label="设置分类">
        <button
          v-for="c in settingsCategories"
          :key="c.id"
          type="button"
          class="rail-item"
          :class="{ current: !narrowing && panel === c.id }"
          :aria-current="!narrowing && panel === c.id ? 'true' : undefined"
          @click="choosePanel(c.id)"
        >
          <span>{{ c.title }}</span>
          <span class="rail-count">{{ categoryCount(c.id) }}</span>
        </button>
        <p class="rail-group">管理</p>
        <button
          v-for="m in MANAGERS"
          :key="m.id"
          type="button"
          class="rail-item"
          :class="{ current: !narrowing && panel === m.id }"
          :aria-current="!narrowing && panel === m.id ? 'true' : undefined"
          @click="choosePanel(m.id)"
        >
          <span>{{ m.title }}</span>
        </button>
      </nav>

      <div class="panel">
        <template v-if="!narrowing && isManager">
          <ContextMenuManager v-if="panel === 'context-menu'" />
          <NewMenuManager v-else-if="panel === 'new-menu'" />
          <ShellPlacesManager v-else-if="panel === 'shell-places'" />
          <KeyRemapManager v-else-if="panel === 'key-remap'" />
        </template>

        <section v-else class="card list-card" aria-labelledby="settings-panel-title">
          <div class="list-head">
            <h2 id="settings-panel-title" class="list-title">{{ panelTitle }}</h2>
            <span class="muted small">{{ panelNote }}</span>
          </div>
          <p v-if="shownFeatures.length === 0" class="empty muted" role="status">
            {{ query.trim() ? '没搜到。换个说法试试。' : '这里没有要列的设置。' }}
          </p>
          <ul class="rows">
            <li
              v-for="f in shownFeatures"
              :id="`setting-${f.id}`"
              :key="f.id"
              class="row"
              :class="{ open: expanded.has(f.id) }"
              tabindex="-1"
            >
              <div class="row-line">
                <div class="row-main">
                  <p class="row-title">
                    <span>{{ f.title }}</span>
                    <TagPill v-if="f.recommend === 'recommended'" tone="info">推荐</TagPill>
                    <TagPill v-else-if="f.recommend === 'not-recommended'" tone="advice">不推荐</TagPill>
                    <TagPill v-if="narrowing" tone="neutral">{{ categoryTitle(f.category) }}</TagPill>
                  </p>
                  <p class="row-sub one-line">{{ f.description }}</p>
                  <p v-if="rowInfo(f).note" class="row-sub state-note">{{ rowInfo(f).note }}</p>
                </div>
                <span v-if="f.applicable && f.reboot !== 'none'" class="reboot small muted">{{ rebootWhen[f.reboot] }}</span>
                <template v-if="f.applicable">
                  <BusySpinner v-if="rowInfo(f).loading" size="small" />
                  <span v-else class="state-word">{{ rowInfo(f).on ? '开' : '关' }}</span>
                  <button
                    type="button"
                    class="switch"
                    role="switch"
                    :aria-checked="rowInfo(f).on"
                    :aria-label="f.title"
                    :disabled="rowInfo(f).loading || rowInfo(f).locked || !!rowInfo(f).error"
                    :title="rowInfo(f).locked ? rowInfo(f).note : undefined"
                    @click="clickSwitch(f)"
                  ></button>
                </template>
                <button
                  type="button"
                  class="expand"
                  :aria-expanded="expanded.has(f.id)"
                  :aria-controls="expanded.has(f.id) ? `setting-${f.id}-more` : undefined"
                  :aria-label="`${expanded.has(f.id) ? '收起' : '展开'}：${f.title}`"
                  @click="toggleExpand(f.id)"
                >
                  <AppIcon :name="expanded.has(f.id) ? 'chevron-up' : 'chevron-down'" :size="16" />
                </button>
              </div>
              <div v-if="expanded.has(f.id)" :id="`setting-${f.id}-more`" class="more">
                <p class="more-desc">{{ f.description }}</p>
                <div class="facts">
                  <div class="fact">
                    <span class="fact-k">改完</span>
                    <span>{{ f.reboot === 'none' ? '马上生效' : `${rebootLabel[f.reboot]}，之后生效` }}</span>
                  </div>
                  <div class="fact">
                    <span class="fact-k">能不能撤销</span>
                    <span v-if="f.reversible">能，在「修改日志」里点「撤销」，或者在这里关掉开关</span>
                    <span v-else class="danger-text">不能：{{ f.irreversibleReason ?? '改了就退不回去' }}</span>
                  </div>
                  <div class="fact">
                    <span class="fact-k">这一项</span>
                    <span>{{ f.subjective ? '看个人习惯，不改也没关系' : f.recommend === 'recommended' ? '建议设置' : '可选' }}</span>
                  </div>
                </div>
                <ul v-if="rowInfo(f).details.length && rowInfo(f).state !== 'applied' && rowInfo(f).state !== 'not-applied'" class="details small muted">
                  <li v-for="(d, i) in rowInfo(f).details" :key="i">{{ d }}</li>
                </ul>
                <p v-if="rowInfo(f).error" class="small muted">{{ rowInfo(f).error }}</p>
                <div v-if="f.applicable && !rowInfo(f).on" class="more-actions">
                  <button type="button" class="btn btn-primary btn-small" @click="previewId = f.id">预览并设置</button>
                </div>
              </div>
            </li>
          </ul>
        </section>
      </div>
    </div>

    <PreviewDialog v-if="previewId" :feature-id="previewId" @close="previewId = null" @applied="onApplied" />
    <BulkApplyDialog
      v-if="bulkOpen"
      :features="pendingRecommended"
      @close="bulkOpen = false"
      @finished="onBulkFinished"
    />
    <ConfirmDialog
      v-if="undoTarget"
      :title="`把「${undoTarget.title}」改回原来的样子？`"
      confirm-text="改回去"
      :busy="undoBusy"
      @confirm="confirmUndo"
      @close="undoTarget = null"
    >
      <p>会恢复成小药箱改之前的样子，和在「修改日志」里点「撤销」一样。</p>
      <p v-if="undoTarget.reboot !== 'none'" class="muted">改回去以后{{ rebootLabel[undoTarget.reboot] }}才生效。</p>
    </ConfirmDialog>
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
}

.toolbar-search {
  width: 320px;
  max-width: 100%;
}

.explorer {
  align-items: flex-start;
}

.explorer-text {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 6px;
}

.notice-text {
  flex: 1;
}

.layout {
  display: grid;
  grid-template-columns: 208px minmax(0, 1fr);
  gap: 20px;
  align-items: start;
}

.rail {
  position: sticky;
  top: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.rail-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-height: 38px;
  padding: 6px 12px;
  border: none;
  border-radius: 8px;
  background: none;
  font-size: var(--text-small);
  text-align: left;
  cursor: pointer;
}

.rail-item:hover {
  background: var(--color-surface-2);
}

.rail-item.current {
  background: var(--color-primary-soft);
  color: var(--color-primary-soft-text);
  font-weight: 600;
}

.rail-count {
  color: var(--color-text-muted);
  font-size: 12.5px;
}

.rail-item.current .rail-count {
  color: inherit;
}

.rail-group {
  margin: 12px 12px 4px;
  color: var(--color-text-muted);
  font-size: 12.5px;
  font-weight: 600;
}

.panel {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
}

.empty {
  padding: 16px 20px;
}

.rows {
  list-style: none;
}

.row {
  border-top: 1px solid var(--color-divider);
}

.row:first-child {
  border-top: none;
}

.row:focus {
  outline: none;
}

.row.open {
  background: var(--color-surface-2);
}

.row-line {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px 12px 20px;
}

.one-line {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row.open .one-line {
  display: none;
}

.state-note {
  color: var(--tone-info-text);
}

.reboot {
  flex: none;
  white-space: nowrap;
}

/* 窗口窄时一行放不下：什么时候生效展开以后也写着，这里先不显示，留地方给说明 */
@media (max-width: 1200px) {
  .reboot {
    display: none;
  }
}

.state-word {
  flex: none;
  width: 1.2em;
  color: var(--color-text-muted);
  font-size: var(--text-small);
  text-align: right;
}

.expand {
  display: inline-flex;
  flex: none;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border: none;
  border-radius: var(--radius-sm);
  background: none;
  color: var(--color-text-muted);
  cursor: pointer;
}

.expand:hover {
  background: var(--color-surface);
  color: var(--color-text);
}

.more {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 0 20px 16px;
}

.more-desc {
  font-size: var(--text-small);
  line-height: 1.7;
}

.facts {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
}

@media (max-width: 1100px) {
  .facts {
    grid-template-columns: minmax(0, 1fr);
  }
}

.fact {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 10px 12px;
  border: 1px solid var(--color-border);
  border-radius: var(--radius);
  background: var(--color-surface);
  font-size: var(--text-small);
}

.fact-k {
  color: var(--color-text-muted);
  font-size: 12px;
}

.details {
  padding-left: 1.3em;
}

.more-actions {
  display: flex;
  gap: 10px;
}
</style>
