<script setup lang="ts">
import { computed, onActivated, reactive, ref } from 'vue'
import { journalList, journalUndo, journalUndoSession } from '../api'
import type { JournalEntryView, JournalSession, UndoResult } from '../api/types'
import AppIcon from '../components/AppIcon.vue'
import BusySpinner from '../components/BusySpinner.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import ModalDialog from '../components/ModalDialog.vue'
import PathText from '../components/PathText.vue'
import TagPill from '../components/TagPill.vue'
import { findFeature } from '../state'
import { vAutofocus } from '../utils/dialogs'
import { errorText, formatClock, formatDateTime, formatDay } from '../utils/format'

// 修改日志：每一步修改都记在这里，逐条或整次恢复原状。

const sessions = ref<JournalSession[]>([])
const loaded = ref(false)
const loading = ref(false)
const loadError = ref<string | null>(null)

/** 新的在前（后端已经排好，这里再保险一次；时间读不出来的保持原来的顺序） */
const sorted = computed(() =>
  [...sessions.value].sort((a, b) => (Date.parse(b.startedAt) || 0) - (Date.parse(a.startedAt) || 0)),
)

async function load(): Promise<void> {
  loading.value = true
  try {
    sessions.value = await journalList()
    loadError.value = null
  } catch (e) {
    loadError.value = errorText(e)
  } finally {
    loading.value = false
    loaded.value = true
  }
}

onActivated(load)

// 显不显示「恢复原状」只看后端给的 canUndo（不能撤销的功能、没改成的记录，后端都已经算进去了）

function undoableCount(s: JournalSession): number {
  return s.entries.filter((e) => e.canUndo).length
}

function restoredCount(s: JournalSession): number {
  return s.entries.filter((e) => e.undone && (e.ok || e.pending)).length
}

type EntryTag = { text: string; tone: 'neutral' | 'advice' } | null

function entryTag(e: JournalEntryView): EntryTag {
  if (e.undone && (e.ok || e.pending)) {
    const when = e.undoneAt ? `（${formatDay(e.undoneAt)} ${formatClock(e.undoneAt)}）` : ''
    return { text: `已恢复原状${when}`, tone: 'neutral' }
  }
  if (e.pending) return { text: '状态不确定', tone: 'advice' }
  if (!e.ok) return { text: '没有改成', tone: 'advice' }
  return null
}

/** 没有「恢复原状」按钮时，告诉用户为什么 */
function noUndoReason(e: JournalEntryView): string | null {
  if (e.canUndo || e.undone || !e.ok) return null
  const f = findFeature(e.feature)
  if (f && !f.reversible) return `这一项不能撤销：${f.irreversibleReason ?? '改了就退不回去。'}`
  return '这一项不能撤销。'
}

/** 模板里用：每次会话和每条记录要显示的东西 */
const sessionViews = computed(() =>
  sorted.value.map((s) => ({
    session: s,
    title: sessionTitle(s),
    undoable: undoableCount(s),
    restored: restoredCount(s),
    entries: s.entries.map((e) => ({ e, tag: entryTag(e), noUndo: noUndoReason(e) })),
  })),
)

function sessionTitle(s: JournalSession): string {
  return `${formatDateTime(s.startedAt)} 的修改`
}

// ── 单条恢复 ──

const busyEntry = ref<string | null>(null)
const busySession = ref<string | null>(null)
const busy = computed(() => busyEntry.value !== null || busySession.value !== null)
const notices = reactive<Record<string, { ok: boolean; text: string }>>({})

/** 撤销时发现被改过：问一下用户 */
const drift = ref<{ entry: JournalEntryView; message: string } | null>(null)
const driftBusy = ref(false)

async function undoEntry(entry: JournalEntryView, force: boolean): Promise<void> {
  busyEntry.value = entry.id
  delete notices[entry.id]
  try {
    const r = await journalUndo(entry.id, force)
    if (r.drift && !force) {
      drift.value = { entry, message: r.message }
      return
    }
    notices[entry.id] = r.ok
      ? { ok: true, text: r.message || '已经恢复原状。' }
      : { ok: false, text: [r.message, r.error].filter(Boolean).join(' ') || '没能恢复。' }
    await load()
  } catch (e) {
    notices[entry.id] = { ok: false, text: `没能恢复：${errorText(e)}` }
  } finally {
    busyEntry.value = null
  }
}

async function confirmDrift(): Promise<void> {
  if (!drift.value) return
  driftBusy.value = true
  await undoEntry(drift.value.entry, true)
  driftBusy.value = false
  drift.value = null
}

// ── 整次撤销 ──

const confirmSession = ref<JournalSession | null>(null)
const sessionResult = ref<{ session: JournalSession; results: UndoResult[]; error: string | null } | null>(null)

async function undoSession(): Promise<void> {
  const s = confirmSession.value
  if (!s) return
  busySession.value = s.id
  try {
    const results = await journalUndoSession(s.id)
    sessionResult.value = { session: s, results, error: null }
  } catch (e) {
    sessionResult.value = { session: s, results: [], error: errorText(e) }
  } finally {
    busySession.value = null
    confirmSession.value = null
  }
  await load()
}

function entryOf(session: JournalSession, entryId: string): JournalEntryView | undefined {
  return session.entries.find((e) => e.id === entryId)
}

function resultLabel(r: UndoResult): { text: string; tone: 'ok' | 'advice' | 'manual' } {
  if (r.ok) return { text: '已恢复', tone: 'ok' }
  if (r.drift) return { text: '已跳过', tone: 'advice' }
  return { text: '没能恢复', tone: 'manual' }
}

const driftSkipped = computed(() => sessionResult.value?.results.some((r) => r.drift) ?? false)
</script>

<template>
  <div class="page">
    <header class="page-header">
      <h1 class="page-title">修改日志</h1>
      <p class="page-lead">每一步修改都记在这里，随时可以恢复原状</p>
    </header>

    <p v-if="!loaded" class="loading-line" role="status"><BusySpinner size="small" />正在读取修改日志…</p>

    <div v-else-if="loadError" class="banner banner-error" role="alert">
      <div>
        <p class="banner-title">没能读出修改日志</p>
        <p>{{ loadError }}</p>
        <button type="button" class="btn btn-secondary btn-small retry" :disabled="loading" @click="load">重试</button>
      </div>
    </div>

    <div v-else-if="sorted.length === 0" class="card empty">
      <p>还没有任何修改记录。</p>
      <p class="muted">小药箱只有在你点了「确认执行」以后才会改动电脑，改了什么都会记在这里。</p>
    </div>

    <section
      v-for="{ session: s, title, undoable, restored, entries } in sessionViews"
      :key="s.id"
      class="session card"
      :aria-label="title"
    >
      <header class="session-head">
        <div>
          <h2 class="session-title">{{ title }}</h2>
          <p class="muted small">
            共 {{ s.entries.length }} 项<template v-if="restored">，已恢复 {{ restored }} 项</template>
          </p>
        </div>
        <button type="button" class="btn btn-secondary" :disabled="busy || undoable === 0" @click="confirmSession = s">
          <AppIcon name="undo" :size="18" />撤销这次的全部修改
        </button>
      </header>

      <ol class="entries">
        <li v-for="{ e, tag, noUndo } in entries" :key="e.id" class="entry" :class="{ faded: e.undone || (!e.ok && !e.pending) }">
          <div class="entry-main">
            <div class="entry-head">
              <span class="entry-time muted">{{ formatClock(e.time) }}</span>
              <h3 class="entry-title">{{ e.featureTitle }}</h3>
              <TagPill v-if="tag" :tone="tag.tone">{{ tag.text }}</TagPill>
            </div>

            <dl class="entry-kv">
              <div class="kv-item full">
                <dt>位置</dt>
                <dd><PathText :text="e.target" /></dd>
              </div>
              <div class="kv-item">
                <dt>原始状态</dt>
                <dd>{{ e.before }}</dd>
              </div>
              <div class="kv-item">
                <dt>操作结果</dt>
                <dd>{{ e.after }}</dd>
              </div>
              <div v-if="e.error" class="kv-item full">
                <dt>出错信息</dt>
                <dd>{{ e.error }}</dd>
              </div>
            </dl>

            <p v-if="e.pending && !e.undone" class="small muted">
              小药箱改这一项时被中途关掉了，不确定改没改成。{{ e.canUndo ? '可以按修改前的记录恢复原状。' : '' }}
            </p>
            <p v-else-if="!e.ok && !e.pending && e.undone" class="small muted">没改成的部分已经自动退回原样。</p>
            <p v-if="noUndo" class="small muted">{{ noUndo }}</p>
            <p
              v-if="notices[e.id]"
              class="small"
              :class="notices[e.id]?.ok ? 'success-text' : 'danger-text'"
              role="status"
            >
              {{ notices[e.id]?.text }}
            </p>
          </div>

          <div v-if="e.canUndo" class="entry-side">
            <button type="button" class="btn btn-secondary btn-small" :disabled="busy" @click="undoEntry(e, false)">
              <BusySpinner v-if="busyEntry === e.id" size="small" />
              <AppIcon v-else name="undo" :size="16" />恢复原状
            </button>
          </div>
        </li>
      </ol>
    </section>

    <!-- 被改过，是否仍要恢复 -->
    <ConfirmDialog
      v-if="drift"
      title="这一项后来被改过"
      confirm-text="仍要恢复"
      cancel-text="先不恢复"
      :busy="driftBusy"
      @confirm="confirmDrift"
      @close="drift = null"
    >
      <p class="question">这一项后来被别的程序或你自己改过，仍要恢复吗？</p>
      <p v-if="drift.message" class="muted">{{ drift.message }}</p>
      <dl class="kv">
        <dt>功能</dt>
        <dd>{{ drift.entry.featureTitle }}</dd>
        <dt>位置</dt>
        <dd><PathText :text="drift.entry.target" /></dd>
        <dt>恢复成</dt>
        <dd>{{ drift.entry.before }}</dd>
      </dl>
    </ConfirmDialog>

    <!-- 撤销整次：先确认 -->
    <ConfirmDialog
      v-if="confirmSession"
      title="撤销这次的全部修改？"
      confirm-text="全部恢复原状"
      :busy="busySession !== null"
      @confirm="undoSession"
      @close="confirmSession = null"
    >
      <p>会按从后往前的顺序，把「{{ sessionTitle(confirmSession) }}」里还没恢复的 {{ undoableCount(confirmSession) }} 项逐条恢复原状。</p>
      <p class="muted">后来被别的程序或你自己改过的项目会跳过，不会硬改；不能撤销的项目不在其中。</p>
    </ConfirmDialog>

    <!-- 撤销整次：结果 -->
    <ModalDialog v-if="sessionResult" title="撤销结果" wide @close="sessionResult = null">
      <div v-if="sessionResult.error" class="banner banner-error" role="alert">
        <div>
          <p class="banner-title">没能撤销</p>
          <p>{{ sessionResult.error }}</p>
        </div>
      </div>
      <p v-else-if="sessionResult.results.length === 0">这次的修改都已经恢复过了，没有需要撤销的。</p>
      <template v-else>
        <ol class="results">
          <li v-for="r in sessionResult.results" :key="r.entryId" class="result-row">
            <TagPill :tone="resultLabel(r).tone" dot>{{ resultLabel(r).text }}</TagPill>
            <div class="result-body">
              <p class="result-title">{{ entryOf(sessionResult.session, r.entryId)?.featureTitle ?? '（未知项目）' }}</p>
              <p class="small muted"><PathText :text="entryOf(sessionResult.session, r.entryId)?.target ?? ''" /></p>
              <p v-if="r.message" class="small">{{ r.message }}</p>
              <p v-if="r.error" class="small danger-text">{{ r.error }}</p>
            </div>
          </li>
        </ol>
        <p v-if="driftSkipped" class="muted">
          标着「已跳过」的项目后来被别的程序或你自己改过，小药箱没有硬改。确定要恢复的话，可以在列表里单独点这一项的「恢复原状」。
        </p>
      </template>
      <template #footer>
        <button type="button" class="btn btn-primary" v-autofocus @click="sessionResult = null">知道了</button>
      </template>
    </ModalDialog>
  </div>
</template>

<style scoped>
.empty {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.retry {
  margin-top: 8px;
}

.session {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.session-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--color-border);
}

.session-title {
  font-size: var(--text-large);
}

.entries {
  display: flex;
  flex-direction: column;
  list-style: none;
}

.entry {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 8px 20px;
  align-items: start;
  padding: 12px 0;
  border-bottom: 1px solid var(--color-border);
}

.entry:last-child {
  border-bottom: none;
  padding-bottom: 2px;
}

.entry-main {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.entry.faded .entry-kv {
  opacity: 0.75;
}

.entry-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
}

.entry-time {
  font-variant-numeric: tabular-nums;
}

.entry-title {
  font-size: var(--text-base);
}

.entry-kv {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 4px 20px;
  margin: 0;
  padding: 8px 14px;
  border-radius: var(--radius);
  background: var(--color-surface-2);
  font-size: var(--text-small);
}

.kv-item {
  display: flex;
  gap: 10px;
  min-width: 0;
}

.kv-item.full {
  grid-column: 1 / -1;
}

.kv-item dt {
  flex: none;
  width: 4.5em;
  color: var(--color-text-muted);
}

.kv-item dd {
  margin: 0;
  min-width: 0;
  overflow-wrap: anywhere;
}

.entry-side {
  padding-top: 2px;
}

.question {
  font-size: var(--text-large);
  font-weight: 600;
}

.results {
  display: flex;
  flex-direction: column;
  list-style: none;
  border: 1px solid var(--color-border);
  border-radius: var(--radius);
}

.result-row {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 10px 14px;
  border-bottom: 1px solid var(--color-border);
}

.result-row:last-child {
  border-bottom: none;
}

.result-body {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.result-title {
  font-weight: 600;
}
</style>
