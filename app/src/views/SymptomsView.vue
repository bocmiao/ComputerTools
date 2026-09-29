<script setup lang="ts">
import { computed, nextTick, ref, useId, useTemplateRef, watch } from 'vue'
import { runCheck, startupList, startupSet, symptomDetail, toolOpen } from '../api'
import type { ApplyResult, CheckResult, StartupItem, SymptomCategory, SymptomDetail } from '../api/types'
import AppIcon from '../components/AppIcon.vue'
import BrightnessControl from '../components/BrightnessControl.vue'
import BusySpinner from '../components/BusySpinner.vue'
import ErrorShotSearch from '../components/ErrorShotSearch.vue'
import ExeCheck from '../components/ExeCheck.vue'
import FileLockers from '../components/FileLockers.vue'
import FileRecovery from '../components/FileRecovery.vue'
import PopupOwner from '../components/PopupOwner.vue'
import PreviewDialog from '../components/PreviewDialog.vue'
import RecycleBinRepair from '../components/RecycleBinRepair.vue'
import WechatCleanup from '../components/WechatCleanup.vue'
import ResultLinks from '../components/ResultLinks.vue'
import StatusLamp, { type LampState } from '../components/StatusLamp.vue'
import TagPill from '../components/TagPill.vue'
import { fixerLabel, riskLabel, riskTone, symptomCategories } from '../labels'
import { catalog, findSymptom, goTo, markHealthStale, nav } from '../state'
import { errorText } from '../utils/format'
import { symptomMatches } from '../utils/search'

// 按症状修：先搜症状（也能粘贴报错截图，认出字以后对关键词：ErrorShotSearch），再逐步检查。查出问题（建议处理、需要人工）的那一步给出对应的修复；
// 没查清楚（没查出来、出错）的那一步不急着修，先让用户再查一次。
// 正常的那一步也可能带一句提示和按钮（例如代理软件开着：「网页打不开的话，先把它退出」），照样显示。

// ── 搜索 ──

const query = ref('')
const searchInput = useTemplateRef<HTMLInputElement>('searchInput')
/** 「看报错截图」展开没有；点搜索框旁边的按钮打开时，直接把光标放进粘贴框 */
const shotOpen = ref(false)
const shotBtn = useTemplateRef<HTMLButtonElement>('shotBtn')
// 在截图框自己的标题上收起来时，那一块整个不见了，光标回到按钮上
watch(shotOpen, async (open) => {
  if (open || document.activeElement === shotBtn.value) return
  await nextTick()
  shotBtn.value?.focus()
})

async function toggleShot(): Promise<void> {
  shotOpen.value = !shotOpen.value
  if (!shotOpen.value) return
  await nextTick()
  const drop = document.getElementById('error-shot-drop')
  drop?.scrollIntoView({ block: 'nearest' })
  drop?.focus({ preventScroll: true })
}

const symptoms = computed(() => catalog.value?.symptoms ?? [])
const searching = computed(() => query.value.trim() !== '')
const filtered = computed(() => symptoms.value.filter((s) => symptomMatches(s, query.value)))

// ── 按类别排好（没在搜的时候） ──

/** 每一类里常见的排前面：这个顺序里的先列，别的按目录里的顺序跟在后面 */
const PRIORITY = [
  'network', 'no-internet-icon', 'network-drops', 'wifi-slow', 'wifi-missing', 'lan-share', 'mobile-hotspot', 'remote-desktop',
  'slow-boot', 'slow-pc', 'bluescreen', 'update-failed', 'black-screen', 'boot-press-f1', 'wakes-up', 'screen-goes-dark',
  'app-missing-dll', 'app-crash', 'popup-ads', 'browser-hijacked', 'app-cannot-run', 'office-broken', 'edge-broken',
  'disk-full', 'deleted-files', 'file-in-use', 'search-broken', 'recycle-bin-corrupted',
  'printer-share', 'printer-offline', 'printer-11b', 'printer-709',
  'screen-display', 'blurry-text', 'screen-colors', 'gpu-not-used',
  'taskbar-broken', 'desktop-icons-missing', 'fullscreen-taskbar',
  'keyboard', 'input-method', 'hotkeys', 'mouse', 'touchpad',
  'no-sound', 'mic-camera', 'bluetooth', 'usb-drive', 'usb-disconnects', 'phone-usb', 'device-error', 'battery',
]

function rank(id: string): number {
  const i = PRIORITY.indexOf(id)
  return i === -1 ? PRIORITY.length : i
}

/** 每类先显示几个，「全部」再展开 */
const PER_CATEGORY = 4
const expandedCats = ref<Set<SymptomCategory>>(new Set())

function toggleCat(id: SymptomCategory): void {
  const next = new Set(expandedCats.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  expandedCats.value = next
}

const groups = computed(() =>
  symptomCategories
    .map((c) => {
      const all = symptoms.value
        .map((s, i) => ({ s, i }))
        .filter((x) => x.s.category === c.id)
        .sort((a, b) => rank(a.s.id) - rank(b.s.id) || a.i - b.i)
        .map((x) => x.s)
      const open = expandedCats.value.has(c.id)
      return { ...c, all, shown: open ? all : all.slice(0, PER_CATEGORY), open }
    })
    .filter((g) => g.all.length > 0),
)

/** 「大家常搜」：直接打开这几个症状（目录里有的才显示） */
const HOT: { id: string; label: string }[] = [
  { id: 'network', label: '上不了网' },
  { id: 'disk-full', label: 'C 盘满了' },
  { id: 'printer-offline', label: '打印机脱机' },
  { id: 'slow-pc', label: '电脑卡' },
  { id: 'no-sound', label: '没声音' },
  { id: 'bluescreen', label: '蓝屏' },
  { id: 'app-missing-dll', label: '缺少 dll' },
  { id: 'printer-11b', label: '0x0000011b' },
]
const hot = computed(() => (catalog.value ? HOT.filter((h) => findSymptom(h.id)) : []))

/** 症状属于哪一类（详情页的面包屑） */
function categoryTitle(id: SymptomCategory | undefined): string {
  return symptomCategories.find((c) => c.id === id)?.title ?? ''
}

/** 示例搜索词：从真实症状的关键词里取（每个症状取一个），保证照着搜一定搜得到 */
function exampleWords(n: number): string[] {
  return symptoms.value
    .map((s) => s.keywords[n] ?? s.keywords[0] ?? s.title)
    .filter((k) => k.trim().length > 0)
    .slice(0, 3)
}
const leadExamples = computed(() => exampleWords(0))
/** 没搜到时换一批说法 */
const emptyExamples = computed(() => exampleWords(1))

function quoted(words: string[]): string {
  return words.map((w) => `「${w}」`).join('')
}

const placeholder = computed(() =>
  leadExamples.value[0] ? `输入你遇到的情况，比如：${leadExamples.value[0]}` : '输入你遇到的情况',
)
const emptyText = computed(() =>
  emptyExamples.value.length
    ? `没找到相关的症状。换个说法试试，比如${quoted(emptyExamples.value)}。`
    : '没找到相关的症状。换个说法试试。',
)

// ── 症状详情 ──

const selectedId = ref<string | null>(null)
const detail = ref<SymptomDetail | null>(null)
const detailError = ref<string | null>(null)
const detailHeading = useTemplateRef<HTMLHeadingElement>('detailHeading')

interface StepRun {
  /** skipped：前面某一步已经找到原因，这一步不用查了 */
  state: 'pending' | 'running' | 'done' | 'error' | 'skipped'
  result: CheckResult | null
  error: string | null
}

const runs = ref<StepRun[]>([])
const checking = ref(false)
const checkedOnce = ref(false)
const startupItems = ref<StartupItem[]>([])
const startupLoading = ref(false)
const startupError = ref<string | null>(null)
const startupNotice = ref<string | null>(null)
const startupBusy = ref<string | null>(null)
/** 切换症状或返回列表时作废正在进行的检查 */
let runToken = 0

async function open(id: string): Promise<void> {
  runToken++
  selectedId.value = id
  detail.value = null
  detailError.value = null
  runs.value = []
  checking.value = false
  checkedOnce.value = false
  startupItems.value = []
  startupNotice.value = null
  const token = runToken
  try {
    const d = await symptomDetail(id)
    if (token !== runToken) return
    detail.value = d
    runs.value = d.steps.map(() => ({ state: 'pending', result: null, error: null }))
    if (showsStartup(id)) void loadStartup()
  } catch (e) {
    if (token !== runToken) return
    detailError.value = errorText(e)
  }
  await nextTick()
  detailHeading.value?.focus()
}

/** 这几个症状的页面里直接列出开机启动项（开机慢、老弹广告：很多弹窗是开机自己启动的软件弹的） */
const STARTUP_SYMPTOMS: ReadonlySet<string> = new Set(['slow-boot', 'popup-ads'])
function showsStartup(id: string | null | undefined): boolean {
  return !!id && STARTUP_SYMPTOMS.has(id)
}

async function loadStartup(): Promise<void> {
  startupLoading.value = true
  startupError.value = null
  try {
    const items = await startupList()
    if (showsStartup(selectedId.value)) startupItems.value = items
  } catch (e) {
    if (showsStartup(selectedId.value)) startupError.value = errorText(e)
  } finally {
    if (showsStartup(selectedId.value)) startupLoading.value = false
  }
}

async function changeStartup(item: StartupItem): Promise<void> {
  if (startupBusy.value) return
  startupBusy.value = item.id
  startupError.value = null
  startupNotice.value = null
  try {
    const result = await startupSet(item.id, !item.enabled)
    if (!result.ok) throw new Error(result.message + (result.error ? ` ${result.error}` : ''))
    startupNotice.value = `「${item.title}」${result.message}`
    markHealthStale()
    await loadStartup()
  } catch (e) {
    startupError.value = errorText(e)
  } finally {
    startupBusy.value = null
  }
}

/** 发布者和签名：有签名写发布者；没签名、签名无效的写明，免得被冒名 */
function startupPublisher(item: StartupItem): string {
  const who = item.publisher ?? '发布者不明'
  switch (item.signature) {
    case 'valid':
      return item.publisher ?? '有数字签名'
    case 'unsigned':
      return `${who}（没有数字签名）`
    case 'invalid':
      return `${who}（数字签名无效）`
    default:
      return who
  }
}

async function openStartupSettings(): Promise<void> {
  startupError.value = null
  try {
    await toolOpen('settings.startup-apps')
  } catch (e) {
    startupError.value = errorText(e)
  }
}

async function back(): Promise<void> {
  runToken++
  selectedId.value = null
  detail.value = null
  checking.value = false
  await nextTick()
  searchInput.value?.focus()
}

async function runStep(index: number, token: number): Promise<void> {
  const step = detail.value?.steps[index]
  const run = runs.value[index]
  if (!step || !run) return
  // 上一次的结果先留着，查完再换：重新检查时说明和按钮不会一下子消失
  run.state = 'running'
  try {
    const r = await runCheck(step.check)
    if (token !== runToken) return
    run.result = r
    run.error = null
    run.state = 'done'
  } catch (e) {
    if (token !== runToken) return
    run.result = null
    run.error = errorText(e)
    run.state = 'error'
  }
}

/** 按顺序一步一步查，每查完一步亮一盏灯 */
async function startCheck(): Promise<void> {
  if (!detail.value || checking.value) return
  const token = ++runToken
  checking.value = true
  runs.value = detail.value.steps.map(() => ({ state: 'pending', result: null, error: null }))
  const steps = detail.value.steps
  for (let i = 0; i < steps.length; i++) {
    await runStep(i, token)
    if (token !== runToken) return
    const status = runs.value[i]?.result?.status
    if (status && steps[i]?.stopOn.includes(status)) {
      for (const later of runs.value.slice(i + 1)) later.state = 'skipped'
      break
    }
  }
  checking.value = false
  checkedOnce.value = true
}

function lampOf(run: StepRun | undefined): LampState {
  if (!run) return 'pending'
  if (run.state === 'done' && run.result) return run.result.status
  if (run.state === 'error') return 'error'
  return run.state === 'running' ? 'running' : 'pending'
}

/**
 * 这一步查完以后怎么办：
 * - fix：查出了问题（建议处理、需要人工），列出修复办法
 * - unclear：没查清楚（没查出来，或者这一步出错了），先别急着修，给「再查一次」
 * - null：正常、不适用，或者还没查
 * 重新检查这一步时（running）按上一次的结论显示，按钮不会一下子消失。
 */
function outcomeOf(run: StepRun | undefined): 'fix' | 'unclear' | null {
  if (!run || run.state === 'pending' || run.state === 'skipped') return null
  if (run.result) {
    const s = run.result.status
    if (s === 'advice' || s === 'manual') return 'fix'
    return s === 'unknown' ? 'unclear' : null
  }
  return run.error !== null ? 'unclear' : null
}

/** 每一步和它的检查结果放在一起，模板里用起来方便 */
const stepViews = computed(() =>
  (detail.value?.steps ?? []).map((step, index) => {
    const run = runs.value[index]
    return {
      step,
      index,
      lamp: lampOf(run),
      result: run?.result ?? null,
      error: run?.error ?? null,
      skipped: run?.state === 'skipped',
      running: run?.state === 'running',
      kind: outcomeOf(run),
    }
  }),
)

const fixCount = computed(() => stepViews.value.filter((v) => v.kind === 'fix').length)
const unclearCount = computed(() => stepViews.value.filter((v) => v.kind === 'unclear').length)

/** 正常（或不适用）的那一步带的提示：结果里的 next 和按钮 */
function hasHint(v: { kind: string | null; result: CheckResult | null }): boolean {
  return v.kind === null && v.result !== null && (v.result.next !== null || hintLinks(v.result).length > 0)
}

/** 正常那一步的按钮：去掉指回这个症状自己的「去修」 */
function hintLinks(r: CheckResult): string[] {
  const self = detail.value ? `symptom:${detail.value.id}` : null
  return r.links.filter((l) => l !== self)
}

/** 都正常时的那句话：只查了一步时不说「几项」；有提示时让用户看看提示 */
const allOkText = computed(() => {
  const checked = stepViews.value.filter((v) => v.result !== null)
  const hints = stepViews.value.some(hasHint)
  const first = checked.length === 1 ? '查过了，这一项正常。' : '查过的几项都正常。'
  const guide = detail.value?.guide ? '照着下面的手动步骤试试' : ''
  if (hints) return `${first}问题还在的话，先看看下面的提示${guide ? `，再${guide}` : ''}。`
  return guide ? `${first}问题还在的话，${guide}。` : first
})

/** 每一步的 id：再查一次以后按钮没了，焦点交给这一步 */
const stepIdBase = useId()
function stepId(index: number): string {
  return `${stepIdBase}-step-${index}`
}

/** 没查清楚的那一步单独再查一次 */
async function recheck(index: number): Promise<void> {
  if (checking.value) return
  await runStep(index, runToken)
  // 查清楚以后「再查一次」按钮就没了，焦点别丢到页面外面去：交给这一步
  await nextTick()
  const active = document.activeElement
  if (!active || active === document.body) document.getElementById(stepId(index))?.focus()
}

// ── 预览修复 ──

const previewId = ref<string | null>(null)
let previewStep: number | null = null

function openPreview(featureId: string, stepIndex: number): void {
  previewId.value = featureId
  previewStep = stepIndex
}

/** 修完以后重新查这一步；真改了东西的话，体检结果可能已经过期 */
function onApplied(r: ApplyResult): void {
  if (r.entryIds.length > 0) markHealthStale()
  if (previewStep === null || checking.value) return
  void runStep(previewStep, runToken)
}

/** 页头右边的那句话：查到第几步发现问题、都正常 */
const checkStatus = computed<{ text: string; tone: 'advice' | 'unknown' | 'ok' | 'info' } | null>(() => {
  if (checking.value) return { text: '正在检查…', tone: 'info' }
  if (!checkedOnce.value) return null
  const firstFix = stepViews.value.find((v) => v.kind === 'fix')
  if (firstFix) return { text: `第 ${firstFix.index + 1} 步发现问题`, tone: 'advice' }
  if (unclearCount.value) return { text: '有几步没查清楚', tone: 'unknown' }
  return { text: '查过的都正常', tone: 'ok' }
})

// 从体检页点「去修」跳过来时，直接打开对应的症状
watch(
  () => nav.symptomId,
  (id) => {
    if (!id) return
    nav.symptomId = null
    void open(id)
  },
  { immediate: true },
)

// 在症状详情里又点了导航栏上的「按症状修」，或者体检页的「全部 N 个症状」：回到症状首页
watch(
  () => nav.home.seq,
  () => {
    if (nav.home.page === 'symptoms' && selectedId.value) void back()
  },
)
</script>

<template>
  <div class="page">
    <!-- 症状首页：搜索 + 按类别排好 -->
    <template v-if="!selectedId">
      <header class="page-head">
        <div class="page-head-main">
          <h1 class="page-title" tabindex="-1">按症状修</h1>
          <p class="page-lead">说说电脑哪里不对劲，一步步查原因；能修的给修复按钮，修不了的说清楚下一步找谁。</p>
        </div>
        <div class="page-actions">
          <button type="button" class="btn btn-link" @click="goTo('report')">没找到？生成诊断报告发给懂哥</button>
        </div>
      </header>

      <section class="card search-card" aria-label="搜索症状">
        <div class="search-row">
          <label class="search-box big-search">
            <AppIcon name="search" :size="20" />
            <span class="visually-hidden">搜索症状</span>
            <input
              id="symptom-search"
              ref="searchInput"
              v-model="query"
              type="search"
              :placeholder="placeholder"
              autocomplete="off"
            />
          </label>
          <button
            v-if="catalog"
            type="button"
            class="btn btn-secondary shot-btn"
            ref="shotBtn"
            :aria-expanded="shotOpen"
            :aria-controls="shotOpen ? 'error-shot' : undefined"
            @click="toggleShot"
          >
            <AppIcon name="image" :size="18" />看报错截图
          </button>
        </div>
        <div v-if="hot.length" class="hot">
          <span class="muted small">大家常搜</span>
          <button v-for="h in hot" :key="h.id" type="button" class="chip" @click="open(h.id)">{{ h.label }}</button>
        </div>
      </section>

      <!-- 点了搜索框旁边的「看报错截图」才显示；收起来就不占地方 -->
      <ErrorShotSearch v-if="catalog && shotOpen" v-model:expanded="shotOpen" @open="open" />

      <p v-if="!catalog" class="loading-line" role="status"><BusySpinner size="small" />正在读取症状列表…</p>

      <!-- 在搜：列出搜到的 -->
      <template v-else-if="searching">
        <p v-if="filtered.length === 0" class="empty muted" role="status">{{ emptyText }}</p>
        <section v-else class="card list-card" aria-labelledby="symptom-results-title">
          <div class="list-head">
            <h2 id="symptom-results-title" class="list-title">找到 {{ filtered.length }} 个</h2>
            <button type="button" class="btn btn-ghost btn-small" @click="query = ''">清空搜索</button>
          </div>
          <ul class="results">
            <li v-for="s in filtered" :key="s.id">
              <button type="button" class="result" @click="open(s.id)">
                <span class="row-main">
                  <span class="row-title">{{ s.title }}</span>
                  <span v-if="s.summary" class="row-sub">{{ s.summary }}</span>
                </span>
                <AppIcon name="chevron-right" :size="18" class="go" />
              </button>
            </li>
          </ul>
        </section>
      </template>

      <!-- 没在搜：按类别 -->
      <div v-else class="cat-grid">
        <section v-for="g in groups" :key="g.id" class="card cat" :aria-labelledby="`cat-${g.id}`">
          <div class="cat-head">
            <span class="cat-icon"><AppIcon :name="g.icon" :size="18" /></span>
            <h2 :id="`cat-${g.id}`" class="cat-title">{{ g.title }}</h2>
            <span class="muted small">{{ g.all.length }} 个</span>
          </div>
          <ul class="cat-list">
            <li v-for="s in g.shown" :key="s.id">
              <button type="button" class="cat-item" @click="open(s.id)">
                <span class="cat-item-title">{{ s.title }}</span>
                <AppIcon name="chevron-right" :size="14" class="go" />
              </button>
            </li>
          </ul>
          <button
            v-if="g.all.length > PER_CATEGORY"
            type="button"
            class="btn btn-link small more"
            :aria-expanded="g.open"
            @click="toggleCat(g.id)"
          >
            {{ g.open ? '收起' : `全部 ${g.all.length} 个` }}
          </button>
        </section>
      </div>
    </template>

    <!-- 症状详情 -->
    <template v-else>
      <nav class="crumbs" aria-label="当前位置">
        <button type="button" class="crumb-link" @click="back">
          <AppIcon name="back" :size="14" />按症状修
        </button>
        <template v-if="detail">
          <span aria-hidden="true">/</span>
          <span>{{ categoryTitle(detail.category) }}</span>
        </template>
      </nav>

      <p v-if="!detail && !detailError" class="loading-line" role="status"><BusySpinner size="small" />正在读取…</p>

      <div v-else-if="detailError" class="banner banner-error" role="alert">
        <div>
          <p class="banner-title">没能打开这个症状</p>
          <p>{{ detailError }}</p>
        </div>
      </div>

      <template v-else-if="detail">
        <header class="page-head">
          <div class="page-head-main">
            <h1 ref="detailHeading" class="page-title" tabindex="-1">{{ detail.title }}</h1>
            <p v-if="detail.summary" class="page-lead">{{ detail.summary }}</p>
          </div>
          <div v-if="detail.steps.length" class="page-actions">
            <span role="status"><TagPill v-if="checkStatus" :tone="checkStatus.tone">{{ checkStatus.text }}</TagPill></span>
            <button type="button" class="btn btn-primary" :disabled="checking" @click="startCheck">
              <BusySpinner v-if="checking" size="small" />
              <AppIcon v-else :name="checkedOnce ? 'refresh' : 'play'" :size="16" />
              {{ checking ? '正在检查…' : checkedOnce ? '重新检查' : '开始检查' }}
            </button>
          </div>
        </header>

        <div class="two-col">
          <div class="main-col">
            <section v-if="detail.steps.length" class="card list-card" aria-labelledby="steps-title">
              <div class="list-head">
                <h2 id="steps-title" class="list-title">检查步骤（{{ detail.steps.length }} 步）</h2>
                <span class="muted small">按顺序查，查到问题就停下来先修</span>
              </div>

              <p v-if="!checkedOnce && !checking" class="steps-hint muted small">
                点右上角的「开始检查」，一步一步查。查的时候只读取信息，不会改动电脑。
              </p>
              <p v-else-if="checkedOnce && !checking" class="steps-hint small" role="status">
                <template v-if="fixCount === 0 && unclearCount === 0">{{ allOkText }}</template>
                <template v-else>
                  查完了。<template v-if="fixCount">亮橙灯、红灯的那几步，下面有可以试的办法。</template>
                  <template v-if="unclearCount">灰灯的那几步没查清楚，可以再查一次。</template>
                </template>
              </p>

              <ol class="steps">
                <li
                  v-for="v in stepViews"
                  :id="stepId(v.index)"
                  :key="`${v.step.check}-${v.index}`"
                  class="step"
                  :class="{ 'step-fix-row': v.kind === 'fix' }"
                  tabindex="-1"
                >
                  <StatusLamp :state="v.lamp" />
                  <div class="step-body">
                    <p class="step-title">
                      <span class="step-no">第 {{ v.index + 1 }} 步</span>{{ v.step.checkTitle }}
                    </p>
                    <p v-if="v.result" class="step-message">{{ v.result.message }}</p>
                    <p v-else-if="v.error" class="danger-text small">这一步没能检查：{{ v.error }}</p>
                    <p v-else-if="v.skipped" class="muted small">前面已经找到原因，这一步不用查了。</p>

                    <div v-if="v.kind === 'fix'" class="step-extra">
                      <p v-if="v.result?.error" class="muted small">出错信息：{{ v.result.error }}</p>
                      <p v-if="v.result?.fixer" class="small"><span class="muted">谁能修：</span>{{ fixerLabel[v.result.fixer] }}</p>
                      <p v-if="v.result?.next" class="small"><span class="muted">下一步：</span>{{ v.result.next }}</p>
                      <!-- 这一步的修复办法列在下面；检测结果里只再加上小工具的按钮（去修、预览修复在这里都是重复的） -->
                      <ResultLinks v-if="v.result" :links="v.result.links" :kinds="['tool']" />

                      <ul v-if="v.step.fixes.length" class="fixes">
                        <li v-for="fix in v.step.fixes" :key="fix.id" class="fix">
                          <div class="fix-main">
                            <p class="fix-title">
                              {{ fix.title }}
                              <TagPill v-if="fix.risk !== 'safe'" :tone="riskTone[fix.risk]" dot>{{ riskLabel[fix.risk] }}</TagPill>
                            </p>
                            <p class="muted small">{{ fix.description }}</p>
                            <p v-if="!fix.applicable" class="small">
                              这台电脑用不了：{{ fix.notApplicableReason ?? '这台电脑的系统不支持这一项。' }}
                            </p>
                          </div>
                          <button
                            v-if="fix.applicable"
                            type="button"
                            class="btn btn-primary btn-small"
                            @click="openPreview(fix.id, v.index)"
                          >
                            看看会改什么
                          </button>
                        </li>
                      </ul>
                      <p v-else class="muted small">
                        这一步小药箱没有自动修复的办法{{ detail.guide ? '，可以看看下面「自己动手试试」' : '' }}。
                      </p>
                    </div>

                    <!-- 正常（或不适用），但结果里有提示：照样显示，不列修复办法 -->
                    <div v-else-if="v.result && hasHint(v)" class="step-extra">
                      <p v-if="v.result.next" class="small"><span class="muted">提示：</span>{{ v.result.next }}</p>
                      <ResultLinks :links="hintLinks(v.result)" @preview="(featureId) => openPreview(featureId, v.index)" />
                    </div>

                    <!-- 没查清楚：不给修复，先再查一次 -->
                    <div v-else-if="v.kind === 'unclear'" class="step-extra">
                      <p class="small">
                        这一步没查清楚，先别急着修。可以再查一次；一直查不清楚的话，{{
                          detail.guide ? '照着「自己动手试试」做' : '可以问问懂哥'
                        }}。
                      </p>
                      <p v-if="v.result?.error" class="muted small">出错信息：{{ v.result.error }}</p>
                      <p v-if="v.result?.next" class="small"><span class="muted">下一步：</span>{{ v.result.next }}</p>
                      <ResultLinks v-if="v.result" :links="v.result.links" :kinds="['tool']" />
                      <div>
                        <button
                          type="button"
                          class="btn btn-secondary btn-small"
                          :disabled="checking || v.running"
                          @click="recheck(v.index)"
                        >
                          <BusySpinner v-if="v.running" size="small" />{{ v.running ? '正在查…' : '再查一次' }}
                        </button>
                      </div>
                    </div>
                  </div>
                </li>
              </ol>
            </section>

            <PopupOwner v-if="detail.id === 'popup-ads'" class="block" />
            <RecycleBinRepair v-if="detail.id === 'recycle-bin-corrupted'" class="block" />
            <WechatCleanup v-if="detail.id === 'disk-full'" class="block" />
            <FileLockers v-if="detail.id === 'file-in-use'" class="block" />
            <ExeCheck v-if="detail.id === 'app-cannot-run'" class="block" />
            <BrightnessControl v-if="detail.id === 'screen-display'" class="block" />

            <section v-if="showsStartup(detail.id)" class="card list-card" aria-labelledby="startup-title">
              <div class="list-head">
                <h2 id="startup-title" class="list-title">管理开机启动项</h2>
                <div class="row-actions">
                  <button type="button" class="btn btn-secondary btn-small" @click="openStartupSettings()">管理其他启动应用</button>
                  <button type="button" class="btn btn-secondary btn-small" :disabled="startupLoading || !!startupBusy" @click="loadStartup()">
                    <AppIcon name="refresh" :size="14" />刷新
                  </button>
                </div>
              </div>
              <p class="steps-hint muted small">
                列的是开机自己启动的软件。只停用、不删除，软件本身还在；开关和任务管理器的「启动应用」是同一个，每次改动都记在「修改日志」里。从应用商店装的软件在 Windows 设置里管理。
              </p>
              <p v-if="startupLoading" class="loading-line steps-hint" role="status"><BusySpinner size="small" />正在读取启动项…</p>
              <p v-if="startupError" class="danger-text small steps-hint" role="alert">{{ startupError }}</p>
              <p v-if="startupNotice" class="small steps-hint" role="status">{{ startupNotice }}</p>
              <p v-if="!startupLoading && !startupError && startupItems.length === 0" class="muted small steps-hint">没有找到开机启动项。</p>
              <ul v-if="startupItems.length" class="startup">
                <li v-for="item in startupItems" :key="item.id" class="list-row">
                  <div class="row-main">
                    <p class="row-title">
                      {{ item.title }}
                      <TagPill :tone="item.enabled ? 'info' : 'neutral'">{{ item.enabled ? '开机自动启动' : '已停用' }}</TagPill>
                      <TagPill v-if="item.advice === 'keep'" tone="advice">建议保留</TagPill>
                    </p>
                    <p class="row-sub">{{ startupPublisher(item) }} · {{ item.location }}</p>
                    <p class="small">{{ item.reason }}</p>
                    <p v-if="item.path" class="row-sub startup-command" :title="item.path">{{ item.path }}</p>
                  </div>
                  <button type="button" class="btn btn-secondary btn-small" :disabled="!!startupBusy" @click="changeStartup(item)">
                    <BusySpinner v-if="startupBusy === item.id" size="small" />{{ item.enabled ? '停用自启' : '恢复自启' }}
                  </button>
                </li>
              </ul>
            </section>

            <section v-if="detail.guide" class="card block" aria-labelledby="guide-title">
              <h2 id="guide-title" class="list-title">自己动手试试</h2>
              <p class="pre-text guide">{{ detail.guide }}</p>
              <ResultLinks v-if="detail.links.length > 0" :links="detail.links" :kinds="['tool', 'symptom', 'test']" />
            </section>

            <!-- 误删的文件先照手动步骤去回收站这些地方找，都没有才用恢复工具：放在手动步骤后面 -->
            <FileRecovery v-if="detail.id === 'deleted-files'" class="block" />
          </div>

          <aside class="side" aria-label="常见原因和帮助">
            <section v-if="detail.causes.length" class="card side-card" aria-labelledby="causes-title">
              <h2 id="causes-title" class="list-title">常见原因</h2>
              <ul class="causes">
                <li v-for="(c, i) in detail.causes" :key="i">{{ c }}</li>
              </ul>
            </section>
            <section class="card side-card" aria-labelledby="help-title">
              <h2 id="help-title" class="list-title">还是不行？</h2>
              <p class="muted small">生成一份去掉隐私信息的诊断报告，发给懂哥，或者复制给 AI 问问。</p>
              <div>
                <button type="button" class="btn btn-secondary btn-small" @click="goTo('report')">
                  <AppIcon name="report" :size="16" />生成诊断报告
                </button>
              </div>
            </section>
          </aside>
        </div>
      </template>
    </template>

    <PreviewDialog v-if="previewId" :feature-id="previewId" @close="previewId = null" @applied="onApplied" />
  </div>
</template>

<style scoped>
.search-card {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 20px 22px;
}

.search-row {
  display: flex;
  gap: 12px;
}

.big-search {
  flex: 1;
  min-width: 0;
  min-height: 52px;
  padding: 0 16px;
  border-width: 2px;
  border-color: var(--color-primary);
}

.big-search input {
  min-height: 48px;
  font-size: var(--text-large);
}

.shot-btn {
  flex: none;
  min-height: 52px;
}

.hot {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}

.hot .muted {
  margin-right: 4px;
}

.empty {
  padding: 8px 2px;
}

.results {
  list-style: none;
}

.result {
  display: flex;
  align-items: center;
  gap: 14px;
  width: 100%;
  padding: 13px 20px;
  border: none;
  border-top: 1px solid var(--color-divider);
  background: none;
  text-align: left;
  cursor: pointer;
}

.results li:first-child .result {
  border-top: none;
}

.result:hover,
.cat-item:hover {
  background: var(--color-surface-2);
}

.go {
  color: var(--color-text-muted);
}

.cat-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 16px;
}

@media (max-width: 1280px) {
  .cat-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

.cat {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 16px 18px;
}

.cat-head {
  display: flex;
  align-items: center;
  gap: 10px;
}

.cat-icon {
  display: inline-flex;
  flex: none;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  border-radius: 9px;
  background: var(--color-primary-soft);
  color: var(--color-primary-soft-text);
}

.cat-title {
  flex: 1;
  font-size: var(--text-base);
}

.cat-list {
  display: flex;
  flex-direction: column;
  list-style: none;
}

.cat-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  width: 100%;
  padding: 7px 6px;
  border: none;
  border-bottom: 1px solid var(--color-divider);
  border-radius: 0;
  background: none;
  font-size: var(--text-small);
  text-align: left;
  cursor: pointer;
}

.cat-item-title {
  min-width: 0;
}

.more {
  align-self: flex-start;
  margin-top: 2px;
}

.main-col {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
}

.block {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.steps-hint {
  padding: 12px 20px 0;
}

.steps {
  display: flex;
  flex-direction: column;
  padding: 6px 0 4px;
  list-style: none;
}

.step {
  display: flex;
  gap: 14px;
  padding: 13px 20px;
  border-top: 1px solid var(--color-divider);
}

.step:first-child {
  border-top: none;
}

.step:focus {
  outline: none;
}

.step-body {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.step-title {
  font-weight: 600;
}

.step-no {
  margin-right: 8px;
  color: var(--color-text-muted);
  font-size: var(--text-small);
  font-weight: 400;
}

.step-message {
  color: var(--color-text-muted);
  font-size: var(--text-small);
  line-height: 1.7;
}

.step-extra {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 4px;
}

.fixes {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 4px;
  list-style: none;
}

.fix {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 12px 14px;
  border: 1px solid var(--color-primary-soft);
  border-radius: var(--radius);
  background: var(--color-surface-2);
}

.fix > .btn {
  flex: none;
}

.fix-main {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.fix-title {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  font-weight: 600;
}

.startup {
  list-style: none;
  margin-top: 8px;
}

.startup-command {
  overflow-wrap: anywhere;
}

.guide {
  line-height: 1.8;
}

.side-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 18px 20px;
}

.causes {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding-left: 1.2em;
  font-size: var(--text-small);
  line-height: 1.7;
}
</style>
