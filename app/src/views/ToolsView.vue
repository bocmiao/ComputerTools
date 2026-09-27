<script setup lang="ts">
import { computed, nextTick, onActivated, onBeforeUnmount, onDeactivated, reactive, ref, watch } from 'vue'
import { toolRun } from '../api'
import type { ApplyResult, ToolResult, ToolSummary } from '../api/types'
import AppIcon from '../components/AppIcon.vue'
import BusySpinner from '../components/BusySpinner.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import LocalUtilities from '../components/LocalUtilities.vue'
import DeviceTests from '../components/DeviceTests.vue'
import KeepAwake from '../components/KeepAwake.vue'
import PreviewDialog from '../components/PreviewDialog.vue'
import TagPill from '../components/TagPill.vue'
import ToolInfoResult from '../components/ToolInfoResult.vue'
import ToolOutcome from '../components/ToolOutcome.vue'
import { audienceLabel } from '../labels'
import { catalog, catalogTools, markHealthStale, nav } from '../state'
import { rememberFocus, restoreFocus } from '../utils/dialogs'
import { errorText } from '../utils/format'
import { useToolOpen } from '../utils/tools'

// 小工具：看信息、一键处理、打开系统自带的工具。都不改设置，所以不记进修改日志，也不用撤销。
// 从检测结果的 tool: 按钮跳过来时（nav.toolId），滚到那个小工具、把焦点放上去；「看信息」的顺便查一次。

/** 给懂哥用的排在后面，其余保持目录里的顺序 */
const tools = computed(() => {
  const list = catalogTools()
  return [...list.filter((t) => t.audience !== 'helper'), ...list.filter((t) => t.audience === 'helper')]
})

function cardId(id: string): string {
  return `tool-${id}`
}

function titleId(id: string): string {
  return `tool-${id}-title`
}

function descId(id: string): string {
  return `tool-${id}-desc`
}

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

/** 模板里用：每个小工具和它的运行状态 */
function withState(list: ToolSummary[]) {
  return list.map((t) => ({ t, s: runs[t.id] ?? idle }))
}

const infoTools = computed(() => withState(tools.value.filter((t) => t.group === 'info')))
const actionTools = computed(() => withState(tools.value.filter((t) => t.group === 'action')))

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

// ── 打开系统工具、「设置」里的页面 ──

const opener = useToolOpen()

const openGroups = computed(() => {
  const open = tools.value.filter((t) => t.group === 'open')
  const programs = open.filter((t) => t.opens !== 'settings')
  const settings = open.filter((t) => t.opens === 'settings')
  const helperNote = programs.some((t) => t.audience === 'helper')
    ? '标着「给懂哥」的比较专业，不熟悉的话别动里面的东西。'
    : ''
  return [
    {
      id: 'program',
      title: '打开系统工具',
      note: `这些是 Windows 自带的工具，小药箱只负责打开。在里面做的改动不会记进「修改日志」，小药箱也没法帮你撤销。${helperNote}`,
      tools: programs,
    },
    {
      id: 'settings',
      title: '打开「设置」里的页面',
      note: '直接跳到 Windows「设置」里对应的那一页。在「设置」里改的东西，小药箱同样不会记下来。',
      tools: settings,
    },
  ]
    .filter((g) => g.tools.length > 0)
    .map((g) => ({ ...g, items: g.tools.map((t) => ({ t, open: opener.states[t.id] ?? null })) }))
})

// ── 从别的页面跳过来：定位到某个小工具 ──

/** 还没处理的定位请求（页面还没显示出来、目录还没读好时先记着） */
const pendingFocus = ref<string | null>(null)
const highlighted = ref<string | null>(null)
let highlightTimer: ReturnType<typeof setTimeout> | undefined

function highlight(id: string): void {
  clearTimeout(highlightTimer)
  highlighted.value = id
  highlightTimer = setTimeout(() => (highlighted.value = null), 2500)
}

async function consumeFocus(): Promise<void> {
  const id = pendingFocus.value
  if (!id || !active) return
  const t = tools.value.find((x) => x.id === id)
  if (!t) {
    // 目录读好了却没有这个小工具：不用再等
    if (catalog.value) pendingFocus.value = null
    return
  }
  pendingFocus.value = null
  await nextTick()
  const card = document.getElementById(cardId(id))
  if (card) {
    card.scrollIntoView({ block: 'start' })
    card.focus({ preventScroll: true })
  }
  highlight(id)
  if (t.group === 'info') {
    const s = runs[id]
    if (!s?.result && !s?.running) void run(t)
  }
}

watch(
  () => nav.toolId,
  (id) => {
    if (!id) return
    nav.toolId = null
    pendingFocus.value = id
    void consumeFocus()
  },
  { immediate: true },
)
// 目录比页面晚读好时再试一次
watch(tools, () => void consumeFocus())

onActivated(() => {
  active = true
  void consumeFocus()
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
  <div class="page">
    <header class="page-header">
      <h1 class="page-title" tabindex="-1">工具箱</h1>
      <p class="page-lead">处理文件和文字，查看电脑配置，一键解决小毛病，也能打开 Windows 自带工具。</p>
    </header>

    <p v-if="!catalog" class="loading-line" role="status"><BusySpinner size="small" />正在读取小工具列表…</p>
    <p v-else-if="tools.length === 0" class="card muted">这个版本的小药箱还没有小工具。</p>

    <LocalUtilities />
    <DeviceTests />
    <KeepAwake />

    <!-- 看信息 -->
    <section v-if="infoTools.length" class="group" aria-labelledby="tools-info-title">
      <div class="group-head">
        <h2 id="tools-info-title" class="section-title">看信息</h2>
        <p class="muted small">只读取，不会改动电脑。</p>
      </div>
      <ul class="tool-list">
        <li
          v-for="{ t, s } in infoTools"
          :id="cardId(t.id)"
          :key="t.id"
          class="tool card"
          :class="{ highlight: highlighted === t.id }"
          tabindex="-1"
          :aria-labelledby="titleId(t.id)"
        >
          <div class="tool-head">
            <div class="tool-main">
              <h3 :id="titleId(t.id)" class="tool-title">
                {{ t.title }}
                <TagPill v-if="audienceLabel[t.audience]" tone="neutral">{{ audienceLabel[t.audience] }}</TagPill>
              </h3>
              <p class="tool-desc">{{ t.description }}</p>
            </div>
            <button type="button" class="btn btn-secondary tool-btn" :disabled="s.running" @click="run(t)">
              <BusySpinner v-if="s.running" size="small" />
              {{ s.running ? '正在查看…' : s.result ? '重新查看' : '查看' }}
            </button>
          </div>
          <p v-if="s.running && !s.result" class="muted small" role="status">正在读取，一般几秒钟就好…</p>
          <div v-if="s.error" class="banner banner-error" role="alert">
            <div>
              <p class="banner-title">没能查看</p>
              <p>{{ s.error }}</p>
            </div>
          </div>
          <ToolInfoResult
            v-if="s.result"
            :result="s.result"
            :running="s.running"
            @preview="(featureId) => (previewId = featureId)"
          />
        </li>
      </ul>
    </section>

    <!-- 一键处理 -->
    <section v-if="actionTools.length" class="group" aria-labelledby="tools-action-title">
      <div class="group-head">
        <h2 id="tools-action-title" class="section-title">一键处理</h2>
        <p class="muted small">清缓存、重开程序这类一次性的小操作。不改设置，所以不记进「修改日志」，也不用撤销。</p>
      </div>
      <ul class="tool-list">
        <li
          v-for="{ t, s } in actionTools"
          :id="cardId(t.id)"
          :key="t.id"
          class="tool card"
          :class="{ highlight: highlighted === t.id }"
          tabindex="-1"
          :aria-labelledby="titleId(t.id)"
        >
          <div class="tool-head">
            <div class="tool-main">
              <h3 :id="titleId(t.id)" class="tool-title">
                {{ t.title }}
                <TagPill v-if="audienceLabel[t.audience]" tone="neutral">{{ audienceLabel[t.audience] }}</TagPill>
              </h3>
              <p class="tool-desc">{{ t.description }}</p>
            </div>
            <button type="button" class="btn btn-secondary tool-btn" :disabled="s.running" @click="startAction(t)">
              <BusySpinner v-if="s.running" size="small" />
              {{ actionButtonText(s) }}
            </button>
          </div>
          <div v-if="s.error" class="banner banner-error" role="alert">
            <div>
              <p class="banner-title">没能完成</p>
              <p>{{ s.error }}</p>
            </div>
          </div>
          <ToolOutcome v-if="s.result" :result="s.result" @preview="(featureId) => (previewId = featureId)" />
        </li>
      </ul>
    </section>

    <!-- 打开系统工具、「设置」里的页面 -->
    <section v-for="g in openGroups" :key="g.id" class="group" :aria-labelledby="`tools-${g.id}-title`">
      <div class="group-head">
        <h2 :id="`tools-${g.id}-title`" class="section-title">{{ g.title }}</h2>
        <p class="muted small">{{ g.note }}</p>
      </div>
      <ul class="open-grid">
        <li
          v-for="{ t, open } in g.items"
          :id="cardId(t.id)"
          :key="t.id"
          class="open-item card"
          :class="{ highlight: highlighted === t.id }"
          tabindex="-1"
        >
          <button
            type="button"
            class="open-btn"
            :aria-labelledby="titleId(t.id)"
            :aria-describedby="descId(t.id)"
            @click="opener.open(t)"
          >
            <span class="open-head">
              <span :id="titleId(t.id)" class="open-title">
                {{ t.title }}
                <TagPill v-if="audienceLabel[t.audience]" tone="neutral">{{ audienceLabel[t.audience] }}</TagPill>
              </span>
              <BusySpinner v-if="open?.busy" size="small" />
              <AppIcon v-else name="external" :size="18" class="open-icon" />
            </span>
            <span :id="descId(t.id)" class="open-desc">{{ t.description }}</span>
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
.group {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.group-head {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.tool-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  list-style: none;
}

.tool {
  display: flex;
  flex-direction: column;
  gap: 12px;
  scroll-margin-top: 16px;
}

.tool-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 20px;
}

.tool-main {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.tool-title {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  font-size: var(--text-large);
}

.tool-btn {
  flex: none;
}

/* 从别的页面跳过来时，标出是哪一个 */
.card {
  transition:
    border-color 0.2s,
    box-shadow 0.2s;
}

.card.highlight {
  border-color: var(--color-primary);
  box-shadow: 0 0 0 3px var(--color-primary-soft);
}

.open-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 10px;
  list-style: none;
}

.open-item {
  display: flex;
  flex-direction: column;
  padding: 0;
  scroll-margin-top: 16px;
}

.open-item:hover {
  border-color: var(--color-primary);
}

.open-btn {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 4px;
  width: 100%;
  padding: 14px 16px;
  border: none;
  border-radius: var(--radius-lg);
  background: transparent;
  text-align: left;
  cursor: pointer;
}

.open-btn:focus-visible {
  outline-offset: -3px;
}

.open-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 8px;
}

.open-title {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  font-weight: 600;
}

.open-icon {
  margin-top: 3px;
  color: var(--color-text-muted);
}

.open-desc {
  color: var(--color-text-muted);
  font-size: var(--text-small);
}

.open-status {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  padding: 0 16px 12px;
}

.open-status-icon {
  margin-top: 3px;
}
</style>
