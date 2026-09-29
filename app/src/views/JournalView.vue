<script setup lang="ts">
import { computed, nextTick, onActivated, reactive, ref } from 'vue'
import { journalList, journalUndo, journalUndoSession } from '../api'
import type { JournalEntryView, JournalSession, Reboot, UndoResult } from '../api/types'
import AppIcon from '../components/AppIcon.vue'
import BusySpinner from '../components/BusySpinner.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import ExplorerRestart from '../components/ExplorerRestart.vue'
import ModalDialog from '../components/ModalDialog.vue'
import PathText from '../components/PathText.vue'
import TagPill from '../components/TagPill.vue'
import { rebootLabel, rebootWeight, type Tone } from '../labels'
import { findFeature, markHealthStale } from '../state'
import { rememberFocus, restoreFocus, vAutofocus } from '../utils/dialogs'
import { errorText, formatClock, formatDateTime, formatDay } from '../utils/format'

// 修改日志：每一步修改都记在这里，逐条或整次撤销（恢复原状）。
// 按天分组；每次打开小药箱以后的修改是一个会话，一张卡片，卡片里一行一处修改。
// 撤销以后和执行以后一样，有的要重启资源管理器、注销或重启才看得到变化：照 UndoResult.reboot 在页面上方说一句，
// 不然用户看不到变化，会以为没撤销成。

const sessions = ref<JournalSession[]>([])
const loaded = ref(false)
const loading = ref(false)
const loadError = ref<string | null>(null)

/** 新的在前（后端已经排好，这里再保险一次；时间读不出来的保持原来的顺序） */
const sorted = computed(() =>
  [...sessions.value].sort((a, b) => (Date.parse(b.startedAt) || 0) - (Date.parse(a.startedAt) || 0)),
)

/** 换页回来、撤销完都会重新读，先发出去的请求可能后回来：只认最后一次 */
let loadSeq = 0

async function load(): Promise<void> {
  const seq = ++loadSeq
  loading.value = true
  try {
    const list = await journalList()
    if (seq !== loadSeq) return
    sessions.value = list
    loadError.value = null
  } catch (e) {
    if (seq !== loadSeq) return
    loadError.value = errorText(e)
  } finally {
    if (seq === loadSeq) {
      loading.value = false
      loaded.value = true
    }
  }
}

onActivated(load)

// ── 每一处修改现在是什么情况 ──
// 显不显示「撤销」只看后端给的 canUndo（不能撤销的功能、没改成的记录，后端都已经算进去了）。
// 三种情况互不重叠：已撤销、还在生效、没有改成（没改成，也没留下改动，或者留下的已经自动退回了）。

/** 已经撤销（恢复原状）了 */
function isUndone(e: JournalEntryView): boolean {
  return e.undone && (e.ok || e.pending)
}

/** 还在生效：改成了、状态不确定，或者没改成但留下了一部分改动（还能撤销） */
function isActive(e: JournalEntryView): boolean {
  return !e.undone && (e.ok || e.pending || e.canUndo)
}

function undoableCount(s: JournalSession): number {
  return s.entries.filter((e) => e.canUndo).length
}

function restoredCount(s: JournalSession): number {
  return s.entries.filter(isUndone).length
}

/** 每一行右边的状态标签 */
function entryState(e: JournalEntryView): { text: string; tone: Tone } {
  if (isUndone(e)) return { text: e.undoneAt ? `已撤销 · ${formatDateTime(e.undoneAt)}` : '已撤销', tone: 'unknown' }
  if (e.pending) return { text: '状态不确定', tone: 'advice' }
  if (!e.ok) return { text: '没有改成', tone: 'advice' }
  if (!e.canUndo) return { text: '不能撤销', tone: 'neutral' }
  return { text: '生效中', tone: 'ok' }
}

/** 没有「撤销」按钮时，告诉用户为什么 */
function noUndoReason(e: JournalEntryView): string | null {
  if (e.canUndo || e.undone || !e.ok) return null
  const f = findFeature(e.feature)
  if (f && !f.reversible) return `这一项不能撤销：${f.irreversibleReason ?? '改了就退不回去。'}`
  return '这一项不能撤销。'
}

/** 对话框里、读屏软件读的会话名字：「今天 14:32 的修改」 */
function sessionTitle(s: JournalSession): string {
  return `${formatDateTime(s.startedAt)} 的修改`
}

/** 卡片标题：这次改了哪些功能（同一个功能改了几处只算一次） */
function sessionSummary(s: JournalSession): string {
  const titles = [...new Set(s.entries.map((e) => e.featureTitle))]
  if (titles.length <= 2) return titles.join('、')
  return `${titles.slice(0, 2).join('、')} 等 ${titles.length} 项`
}

// ── 只看哪些 ──

type Filter = 'all' | 'active' | 'undone'

const FILTERS: { id: Filter; label: string }[] = [
  { id: 'all', label: '全部' },
  { id: 'active', label: '还在生效' },
  { id: 'undone', label: '已撤销' },
]

const filter = ref<Filter>('all')

/** 刚在这里撤销的记录：换筛选以前一直留在列表里，免得在「还在生效」里一撤销就不见了（焦点也跟着丢了） */
const keepShown = reactive(new Set<string>())

function setFilter(f: Filter): void {
  filter.value = f
  keepShown.clear()
}

function shown(e: JournalEntryView): boolean {
  if (filter.value === 'all' || keepShown.has(e.id)) return true
  return filter.value === 'active' ? isActive(e) : isUndone(e)
}

/** 分段按钮和右边「一共」里的数字（按处算） */
const counts = computed(() => {
  const all = sessions.value.flatMap((s) => s.entries)
  const active = all.filter(isActive).length
  const undone = all.filter(isUndone).length
  return { all: all.length, active, undone, failed: all.length - active - undone }
})

/** 模板里用：每次会话和每条记录要显示的东西（筛掉以后一条都不剩的会话不显示） */
const sessionViews = computed(() =>
  sorted.value
    .map((s) => ({
      session: s,
      title: sessionTitle(s),
      summary: sessionSummary(s),
      undoable: undoableCount(s),
      restored: restoredCount(s),
      entries: s.entries.filter(shown).map((e) => ({ e, state: entryState(e), noUndo: noUndoReason(e) })),
    }))
    .filter((v) => v.entries.length > 0),
)

type SessionView = (typeof sessionViews.value)[number]

/** 按会话开始那天分组：今天、昨天、9月20日 */
const dayGroups = computed(() => {
  const days: { key: string; label: string; sessions: SessionView[] }[] = []
  for (const v of sessionViews.value) {
    const key = dayKey(v.session.startedAt)
    const last = days[days.length - 1]
    if (last && last.key === key) last.sessions.push(v)
    else days.push({ key, label: formatDay(v.session.startedAt), sessions: [v] })
  }
  return days
})

function dayKey(value: string): string {
  const d = new Date(value)
  return Number.isNaN(d.getTime()) ? value : `${d.getFullYear()}-${d.getMonth() + 1}-${d.getDate()}`
}

// ── 给懂哥看：每一处改的是哪个位置（按会话展开） ──

const detailsOpen = reactive(new Set<string>())

function toggleDetails(id: string): void {
  if (detailsOpen.has(id)) detailsOpen.delete(id)
  else detailsOpen.add(id)
}

function listId(sessionId: string): string {
  return `journal-list-${sessionId}`
}

// ── 撤销以后还要重启资源管理器、注销、重启：页面上方一条提示，用户关掉以前一直在 ──
// （整次撤销的结果对话框里自己会说，也有按钮，不放到这里）

const restartNeeds = ref<{ title: string; reboot: Reboot }[]>([])
/** 每来一个新的要求就换一个新的「现在重启资源管理器」（上一次重启成功以后，按钮就藏起来了） */
const restartKey = ref(0)

function strongestReboot(list: Reboot[]): Reboot {
  let strongest: Reboot = 'none'
  for (const need of list) if (rebootWeight[need] > rebootWeight[strongest]) strongest = need
  return strongest
}

function noteRestart(title: string, reboot: Reboot): void {
  if (reboot === 'none') return
  restartNeeds.value.push({ title, reboot })
  restartKey.value++
}

/** 最「重」的那个要求，只说一次（要注销、重启的话，那时资源管理器会跟着重开） */
const restartReboot = computed(() => strongestReboot(restartNeeds.value.map((n) => n.reboot)))

const restartTitles = computed(() => {
  const titles = [...new Set(restartNeeds.value.map((n) => n.title))]
  const named = titles
    .slice(0, 3)
    .map((t) => `「${t}」`)
    .join('')
  return titles.length > 3 ? `${named}等 ${titles.length} 项` : named
})

async function dismissRestart(): Promise<void> {
  // 关掉按钮跟着提示一起没了：焦点退回到页面标题
  const focus = rememberFocus()
  restartNeeds.value = []
  await nextTick()
  restoreFocus(focus)
}

// ── 单条撤销 ──

const busyEntry = ref<string | null>(null)
const busySession = ref<string | null>(null)
const busy = computed(() => busyEntry.value !== null || busySession.value !== null)
const notices = reactive<Record<string, { ok: boolean; text: string }>>({})

/** 撤销时发现被改过：问一下用户 */
const drift = ref<{ entry: JournalEntryView; message: string } | null>(null)
const driftBusy = ref(false)

async function undoEntry(entry: JournalEntryView, force: boolean): Promise<void> {
  // 撤销成功以后这一条的「撤销」按钮就没了：焦点交给这一条，别丢到页面外面
  const focus = rememberFocus()
  busyEntry.value = entry.id
  delete notices[entry.id]
  keepShown.add(entry.id)
  // 除了「被改过，先问用户」，其余情况（成功、没恢复成、命令出错）都重新读一次日志：
  // 出错时列表也可能已经变了，例如这一项其实已经恢复过了
  let reload = true
  try {
    const r = await journalUndo(entry.id, force)
    if (r.drift && !force) {
      drift.value = { entry, message: r.message }
      reload = false
      return
    }
    if (r.ok) {
      markHealthStale()
      noteRestart(entry.featureTitle, r.reboot ?? 'none')
    }
    notices[entry.id] = r.ok
      ? { ok: true, text: r.message || '已经恢复原状。' }
      : { ok: false, text: [r.message, r.error].filter(Boolean).join(' ') || '没能恢复。' }
  } catch (e) {
    notices[entry.id] = { ok: false, text: `没能恢复：${errorText(e)}` }
  } finally {
    // 读完再放开按钮，免得在旧列表上又点一次
    if (reload) await load()
    busyEntry.value = null
  }
  await nextTick()
  if (!document.activeElement || document.activeElement === document.body) restoreFocus(focus)
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
  for (const e of undoPlan(s)) keepShown.add(e.id)
  try {
    const results = await journalUndoSession(s.id)
    if (results.some((r) => r.ok)) markHealthStale()
    sessionResult.value = { session: s, results, error: null }
  } catch (e) {
    sessionResult.value = { session: s, results: [], error: errorText(e) }
  } finally {
    busySession.value = null
    confirmSession.value = null
  }
  await load()
}

/** 整次撤销会处理哪些项目：还能恢复的，按从后往前的顺序（和后端一样） */
function undoPlan(s: JournalSession): JournalEntryView[] {
  return s.entries.filter((e) => e.canUndo).reverse()
}

function entryOf(session: JournalSession, entryId: string): JournalEntryView | undefined {
  return session.entries.find((e) => e.id === entryId)
}

function resultLabel(r: UndoResult): { text: string; tone: 'ok' | 'advice' | 'manual' } {
  if (r.ok) return { text: '已撤销', tone: 'ok' }
  if (r.drift) return { text: '已跳过', tone: 'advice' }
  return { text: '没能撤销', tone: 'manual' }
}

const driftSkipped = computed(() => sessionResult.value?.results.some((r) => r.drift) ?? false)

/** 整次撤销以后最「重」的要求，只说一次（没恢复成的，后端给的都是 none） */
const sessionReboot = computed<Reboot>(() =>
  strongestReboot((sessionResult.value?.results ?? []).map((r) => (r.ok ? (r.reboot ?? 'none') : 'none'))),
)
</script>

<template>
  <div class="page">
    <header class="page-head">
      <div class="page-head-main">
        <h1 class="page-title" tabindex="-1">修改日志</h1>
        <p class="page-lead">小药箱改过的每一处都记在这里，可以一条条撤销，也可以整次撤销。</p>
      </div>
      <div v-if="loaded && counts.all > 0" class="page-actions">
        <div class="seg" role="group" aria-label="只看">
          <button
            v-for="f in FILTERS"
            :key="f.id"
            type="button"
            class="seg-btn"
            :aria-pressed="filter === f.id"
            @click="setFilter(f.id)"
          >
            {{ f.label }} {{ counts[f.id] }}
          </button>
        </div>
      </div>
    </header>

    <p v-if="!loaded" class="loading-line" role="status"><BusySpinner size="small" />正在读取修改日志…</p>

    <template v-else>
      <div v-if="loadError" class="banner banner-error" role="alert">
        <div>
          <p class="banner-title">没能读出修改日志</p>
          <p>{{ loadError }}</p>
          <button type="button" class="btn btn-secondary btn-small retry" :disabled="loading" @click="load">重试</button>
        </div>
      </div>

      <div v-else-if="sorted.length === 0" class="card empty">
        <span class="empty-icon"><AppIcon name="journal" :size="26" /></span>
        <p class="empty-title">还没有任何修改记录</p>
        <p class="muted">小药箱只有在你确认以后才会改动电脑。改了哪里、改之前是什么样，都会记在这里，随时可以撤销。</p>
      </div>

      <template v-if="sorted.length > 0">
        <!-- 撤销以后还要重启才看得到变化。列表很长时也要看得见，所以贴在页面上方 -->
        <div v-if="restartNeeds.length > 0" class="banner banner-warning restart-bar">
          <AppIcon name="warning" :size="20" class="restart-icon" />
          <div class="restart-body">
            <p role="status">
              刚撤销的{{ restartTitles }}<strong>{{ rebootLabel[restartReboot] }}</strong>，之后才能看到效果。<template
                v-if="restartReboot === 'explorer'"
                >重启时桌面和任务栏会闪一下，打开的文件夹窗口会关掉。</template
              >
            </p>
            <!-- 放在 role=status 外面：按钮和它的结果自己会读 -->
            <ExplorerRestart v-if="restartReboot === 'explorer'" :key="restartKey" />
          </div>
          <button type="button" class="btn btn-ghost btn-small restart-close" aria-label="关掉这条提示" @click="dismissRestart">
            <AppIcon name="close" :size="16" />
          </button>
        </div>

        <div class="two-col">
          <div class="days">
            <div v-if="dayGroups.length === 0 && filter !== 'all'" class="card filter-empty">
              <p>{{ filter === 'active' ? '现在没有还在生效的修改。' : '还没有撤销过任何修改。' }}</p>
              <button type="button" class="btn btn-link" @click="setFilter('all')">看全部记录</button>
            </div>

            <section v-for="(day, i) in dayGroups" :key="day.key" class="day" :aria-labelledby="`journal-day-${i}`">
              <h2 :id="`journal-day-${i}`" class="subhead day-title">{{ day.label }}</h2>

              <section
                v-for="{ session: s, title, summary, undoable, restored, entries } in day.sessions"
                :key="s.id"
                class="card list-card session"
                :aria-label="title"
              >
                <header class="session-head">
                  <span class="session-time">{{ formatClock(s.startedAt) }}</span>
                  <div class="session-heading">
                    <h3 class="session-title">{{ summary }}</h3>
                    <p class="session-meta">
                      共 {{ s.entries.length }} 处<template v-if="restored">，已撤销 {{ restored }} 处</template>
                    </p>
                  </div>
                  <button
                    type="button"
                    class="btn btn-secondary btn-small"
                    :disabled="busy || undoable === 0"
                    :aria-label="`整次撤销：${title}`"
                    @click="confirmSession = s"
                  >
                    <AppIcon name="undo" :size="16" />整次撤销
                  </button>
                </header>

                <ol :id="listId(s.id)" class="entries">
                  <li
                    v-for="{ e, state, noUndo } in entries"
                    :key="e.id"
                    class="list-row entry"
                    :class="{ faded: !isActive(e), 'no-action': !e.canUndo }"
                  >
                    <p class="row-title entry-title">{{ e.featureTitle }}</p>
                    <TagPill class="entry-state" :tone="state.tone">{{ state.text }}</TagPill>

                    <div class="entry-body">
                      <p v-if="e.before || e.after" class="row-sub entry-change">
                        <template v-if="e.after">
                          {{ e.before }}<span class="arrow" aria-hidden="true">→</span
                          ><span class="visually-hidden">，改成了</span>{{ e.after }}
                        </template>
                        <template v-else>原来是 {{ e.before }}</template>
                      </p>
                      <p v-if="detailsOpen.has(s.id)" class="entry-note entry-where">
                        {{ formatClock(e.time) }} 改的 · 位置：<PathText :text="e.target" />
                      </p>

                      <p v-if="e.pending && !e.undone" class="entry-note">
                        小药箱改这一项时被中途关掉了，不确定改没改成。{{
                          e.canUndo ? '可以点「撤销」，按修改前的记录改回去。' : ''
                        }}
                      </p>
                      <p v-else-if="!e.ok && !e.pending && e.undone" class="entry-note">没改成的部分已经自动退回原样。</p>
                      <p v-else-if="!e.ok && !e.pending && e.canUndo" class="entry-note">
                        这一项没改成，可能留下了一部分改动，可以点「撤销」改回原样。
                      </p>
                      <p v-if="noUndo" class="entry-note">{{ noUndo }}</p>
                      <!-- 状态不确定的，上面已经说了；改成了的只会是「后面的步骤失败了，这一步已自动退回」 -->
                      <p v-if="e.error && !e.pending" class="entry-note" :class="{ 'danger-text': !e.ok }">
                        {{ e.ok ? e.error : `出错信息：${e.error}` }}
                      </p>

                      <p
                        v-if="notices[e.id]"
                        class="entry-notice"
                        :class="notices[e.id]?.ok ? 'success-text' : 'danger-text'"
                        role="status"
                      >
                        {{ notices[e.id]?.text }}
                      </p>
                    </div>

                    <div class="entry-action">
                      <button
                        v-if="e.canUndo"
                        type="button"
                        class="btn btn-secondary btn-small"
                        :disabled="busy"
                        :aria-label="`撤销：${e.featureTitle}`"
                        @click="undoEntry(e, false)"
                      >
                        <BusySpinner v-if="busyEntry === e.id" size="small" />
                        <AppIcon v-else name="undo" :size="16" />撤销
                      </button>
                    </div>
                  </li>
                </ol>

                <div class="session-foot">
                  <button
                    type="button"
                    class="btn btn-link btn-small details-toggle"
                    :aria-expanded="detailsOpen.has(s.id)"
                    :aria-controls="listId(s.id)"
                    @click="toggleDetails(s.id)"
                  >
                    <AppIcon name="chevron-down" :size="14" class="chevron" />给懂哥看：每一处改的是哪个位置
                  </button>
                </div>
              </section>
            </section>
          </div>

          <div class="side">
            <section class="card side-card" aria-labelledby="journal-undo-title">
              <h2 id="journal-undo-title" class="side-title">撤销是什么意思</h2>
              <p class="side-text">改之前的样子都记着。点「撤销」就把这一处改回去；「整次撤销」把那一次改的全部改回去。</p>
              <p class="side-text">撤销前会先核对：后来又被别的程序或你自己改过的，会先问你。</p>
              <p class="side-text">撤销不了的（比如重置 Winsock），改之前的确认框里就会醒目地说清楚。</p>
            </section>

            <section class="card side-card" aria-labelledby="journal-sum-title">
              <h2 id="journal-sum-title" class="side-title">一共</h2>
              <dl class="totals">
                <div class="total-row">
                  <dt>改过的地方</dt>
                  <dd>{{ counts.all }} 处</dd>
                </div>
                <div class="total-row">
                  <dt>还在生效</dt>
                  <dd>{{ counts.active }} 处</dd>
                </div>
                <div class="total-row">
                  <dt>已撤销</dt>
                  <dd>{{ counts.undone }} 处</dd>
                </div>
                <div v-if="counts.failed > 0" class="total-row">
                  <dt>没有改成</dt>
                  <dd>{{ counts.failed }} 处</dd>
                </div>
              </dl>
            </section>
          </div>
        </div>
      </template>
    </template>

    <!-- 撤销前核对发现被改过（或者没法核对），是否仍要撤销 -->
    <ConfirmDialog
      v-if="drift"
      title="这一项后来可能被改过"
      confirm-text="仍要撤销"
      cancel-text="先不撤销"
      :busy="driftBusy"
      @confirm="confirmDrift"
      @close="drift = null"
    >
      <p class="question">撤销会把它改回原来的样子，可能会覆盖后来的改动。仍要撤销吗？</p>
      <p v-if="drift.message" class="muted">{{ drift.message }}</p>
      <dl class="kv">
        <dt>功能</dt>
        <dd>{{ drift.entry.featureTitle }}</dd>
        <dt>位置</dt>
        <dd><PathText :text="drift.entry.target" /></dd>
        <dt>改回成</dt>
        <dd>{{ drift.entry.before }}</dd>
      </dl>
    </ConfirmDialog>

    <!-- 整次撤销：先确认 -->
    <ConfirmDialog
      v-if="confirmSession"
      title="撤销这次的全部修改？"
      confirm-text="全部撤销"
      wide
      :busy="busySession !== null"
      @confirm="undoSession"
      @close="confirmSession = null"
    >
      <p>
        会按从后往前的顺序，把「{{ sessionTitle(confirmSession) }}」里下面这 {{ undoPlan(confirmSession).length }}
        处逐条改回原来的样子：
      </p>
      <ol class="plan">
        <li v-for="e in undoPlan(confirmSession)" :key="e.id" class="plan-row">
          <p class="plan-title">
            {{ e.featureTitle }}
            <TagPill v-if="e.pending" tone="advice">状态不确定，会直接撤销</TagPill>
          </p>
          <p class="small muted"><PathText :text="e.target" /></p>
        </li>
      </ol>
      <p class="muted">
        每一处撤销前都会先核对，后来被别的程序或你自己改过的会跳过，并单独告诉你；程序中途退出、状态不确定的记录没法核对，会直接按修改前的记录改回去。不能撤销的项目不在其中。
      </p>
    </ConfirmDialog>

    <!-- 整次撤销：结果 -->
    <ModalDialog v-if="sessionResult" title="撤销结果" wide @close="sessionResult = null">
      <div v-if="sessionResult.error" class="banner banner-error" role="alert">
        <div>
          <p class="banner-title">没能撤销</p>
          <p>{{ sessionResult.error }}</p>
        </div>
      </div>
      <p v-else-if="sessionResult.results.length === 0">这次的修改都已经撤销过了，没有需要撤销的。</p>
      <template v-else>
        <div v-if="sessionReboot !== 'none'" class="session-reboot">
          <p><strong>{{ rebootLabel[sessionReboot] }}</strong>，之后才能看到全部效果。</p>
          <!-- 最重的要求只是重启资源管理器时，直接给按钮（要注销、重启的话，那时资源管理器会跟着重开） -->
          <ExplorerRestart v-if="sessionReboot === 'explorer'" />
        </div>
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
          标着「已跳过」的项目，撤销前核对时发现后来可能被别的程序或你自己改过（或者没法核对），小药箱没有硬改。确定要改回去的话，可以在列表里单独点这一项的「撤销」。
        </p>
      </template>
      <template #footer>
        <button type="button" class="btn btn-primary" v-autofocus @click="sessionResult = null">知道了</button>
      </template>
    </ModalDialog>
  </div>
</template>

<style scoped>
.retry {
  margin-top: 8px;
}

/* ───── 还没有记录 ───── */

.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 40px 24px;
  text-align: center;
}

.empty-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 56px;
  height: 56px;
  margin-bottom: 6px;
  border-radius: 50%;
  background: var(--color-primary-soft);
  color: var(--color-primary-soft-text);
}

.empty-title {
  font-size: var(--text-large);
  font-weight: 600;
}

.empty .muted {
  max-width: 34em;
}

/* ───── 撤销以后还要重启：滚到下面也看得见 ───── */

.restart-bar {
  position: sticky;
  top: 12px;
  z-index: 5;
  /* 第一层用页面底色盖住上方 12px 的空隙，滚过去的文字不会从提示条上面露出来 */
  box-shadow:
    0 -12px 0 4px var(--color-bg),
    var(--shadow-card);
}

.restart-icon {
  margin-top: 2px;
}

.restart-body {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.restart-close {
  min-height: 28px;
  margin: -4px -8px -4px 0;
  padding: 2px 6px;
  color: inherit;
}

/* ───── 左边：按天分组的会话 ───── */

.days {
  display: flex;
  flex-direction: column;
  gap: 18px;
  min-width: 0;
}

.day {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.day-title {
  margin-top: 2px;
}

.filter-empty {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px 14px;
}

/* 卡片够宽时，状态标签和按钮排在右边一列；窄的时候标签放在标题右边、按钮放在第二行右边 */
.session {
  container-type: inline-size;
}

.session-head {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px 12px 20px;
  border-bottom: 1px solid var(--color-divider);
}

.session-time {
  flex: none;
  min-width: 3em;
  color: var(--color-text-muted);
  font-size: var(--text-small);
  font-variant-numeric: tabular-nums;
}

.session-heading {
  display: flex;
  flex: 1;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 0 10px;
  min-width: 0;
}

.session-title {
  font-size: var(--text-base);
  overflow-wrap: anywhere;
}

.session-meta {
  color: var(--color-text-muted);
  font-size: var(--text-small);
}

.entries {
  list-style: none;
}

.entry {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: start;
  gap: 4px 14px;
  padding: 12px 20px;
}

.entry:focus-visible {
  outline-offset: -3px;
}

.entry-title {
  grid-row: 1;
  grid-column: 1;
  overflow-wrap: anywhere;
}

.entry.faded .entry-title {
  color: var(--color-text-muted);
}

.entry-state {
  grid-row: 1;
  grid-column: 2;
  justify-self: end;
}

.entry-body {
  display: flex;
  grid-row: 2;
  grid-column: 1;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}

.entry.no-action .entry-body {
  grid-column: 1 / -1;
}

.entry-action {
  display: flex;
  grid-row: 2;
  grid-column: 2;
  justify-content: flex-end;
  min-width: 80px;
}

.entry.no-action .entry-action {
  display: none;
}

@container (min-width: 600px) {
  .entry {
    grid-template-columns: minmax(0, 1fr) auto auto;
    align-items: center;
  }

  .entry-state {
    grid-row: 1 / span 2;
  }

  .entry-body,
  .entry.no-action .entry-body {
    grid-column: 1;
  }

  /* 没有按钮的行也留出按钮的位置，状态标签才能上下对齐 */
  .entry-action,
  .entry.no-action .entry-action {
    display: flex;
    grid-row: 1 / span 2;
    grid-column: 3;
  }
}

.entry-change {
  overflow-wrap: anywhere;
}

.arrow {
  margin: 0 6px;
}

.entry-note {
  color: var(--color-text-muted);
  font-size: var(--text-small);
  overflow-wrap: anywhere;
}

.entry-note.danger-text {
  color: var(--color-danger-text);
}

.entry-notice {
  font-size: var(--text-small);
  overflow-wrap: anywhere;
}

.session-foot {
  padding: 8px 20px 10px;
  border-top: 1px solid var(--color-divider);
}

.details-toggle {
  gap: 4px;
  color: var(--color-text-muted);
}

.details-toggle:hover:not(:disabled) {
  color: var(--color-text);
}

.chevron {
  transition: transform 0.15s;
}

.details-toggle[aria-expanded='true'] .chevron {
  transform: rotate(180deg);
}

/* ───── 右边：说明和数字 ───── */

.side-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 16px 18px;
}

.side-title {
  font-size: var(--text-base);
}

.side-text {
  font-size: var(--text-small);
  line-height: 1.7;
}

.totals {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 0;
  font-size: var(--text-small);
}

.total-row {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}

.total-row dt {
  color: var(--color-text-muted);
}

.total-row dd {
  margin: 0;
  font-variant-numeric: tabular-nums;
}

/* ───── 对话框 ───── */

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

.session-reboot {
  padding: 10px 14px;
  border-radius: var(--radius);
  background: var(--color-surface-2);
}

/* 整次撤销的确认框：要撤销哪些项 */
.plan {
  display: flex;
  flex-direction: column;
  list-style: none;
  border: 1px solid var(--color-border);
  border-radius: var(--radius);
}

.plan-row {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 14px;
  border-bottom: 1px solid var(--color-border);
}

.plan-row:last-child {
  border-bottom: none;
}

.plan-title {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  font-weight: 600;
}
</style>
