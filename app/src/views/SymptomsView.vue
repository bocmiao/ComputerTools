<script setup lang="ts">
import { computed, nextTick, ref, useId, useTemplateRef, watch } from 'vue'
import { runCheck, startupList, startupSet, symptomDetail, toolOpen } from '../api'
import type { ApplyResult, CheckResult, StartupItem, SymptomDetail, SymptomSummary } from '../api/types'
import AppIcon from '../components/AppIcon.vue'
import BusySpinner from '../components/BusySpinner.vue'
import ErrorShotSearch from '../components/ErrorShotSearch.vue'
import ExeCheck from '../components/ExeCheck.vue'
import FileLockers from '../components/FileLockers.vue'
import PopupOwner from '../components/PopupOwner.vue'
import PreviewDialog from '../components/PreviewDialog.vue'
import RecycleBinRepair from '../components/RecycleBinRepair.vue'
import ResultLinks from '../components/ResultLinks.vue'
import StatusLamp, { type LampState } from '../components/StatusLamp.vue'
import TagPill from '../components/TagPill.vue'
import { fixerLabel, maturityLabel, riskLabel, riskTone } from '../labels'
import { catalog, markHealthStale, nav } from '../state'
import { errorText, normalizeForSearch } from '../utils/format'

// 按症状修：先搜症状（也能粘贴报错截图，认出字以后对关键词：ErrorShotSearch），再逐步检查。查出问题（建议处理、需要人工）的那一步给出对应的修复；
// 没查清楚（没查出来、出错）的那一步不急着修，先让用户再查一次。
// 正常的那一步也可能带一句提示和按钮（例如代理软件开着：「网页打不开的话，先把它退出」），照样显示。

// ── 搜索 ──

const query = ref('')
const searchInput = useTemplateRef<HTMLInputElement>('searchInput')

function matches(s: SymptomSummary, q: string): boolean {
  const nq = normalizeForSearch(q)
  if (!nq) return true
  return [s.title, ...s.keywords]
    .map(normalizeForSearch)
    .filter((h) => h.length > 0)
    // 「电脑没网了」里包含关键词「没网」也算
    .some((h) => h.includes(nq) || (h.length >= 2 && nq.includes(h)))
}

const symptoms = computed(() => catalog.value?.symptoms ?? [])
const filtered = computed(() => symptoms.value.filter((s) => matches(s, query.value)))

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

const leadText = computed(() =>
  leadExamples.value.length
    ? `说说电脑哪里不对劲，比如${quoted(leadExamples.value)}。先检查，找到原因再动手。`
    : '说说电脑哪里不对劲。先检查，找到原因再动手。',
)
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
</script>

<template>
  <div class="page">
    <!-- 症状列表 -->
    <template v-if="!selectedId">
      <header class="page-header">
        <h1 class="page-title" tabindex="-1">按症状修</h1>
        <p class="page-lead">{{ leadText }}</p>
      </header>

      <div class="search">
        <label for="symptom-search" class="visually-hidden">搜索症状</label>
        <input
          id="symptom-search"
          ref="searchInput"
          v-model="query"
          type="search"
          class="input search-input"
          :placeholder="placeholder"
          autocomplete="off"
        />
      </div>

      <ErrorShotSearch v-if="catalog" @open="open" />

      <p v-if="!catalog" class="loading-line" role="status"><BusySpinner size="small" />正在读取症状列表…</p>
      <p v-else-if="filtered.length === 0" class="empty muted" role="status">{{ emptyText }}</p>
      <ul v-else class="symptom-list">
        <li v-for="s in filtered" :key="s.id">
          <button type="button" class="symptom card" @click="open(s.id)">
            <span class="symptom-main">
              <span class="symptom-title">{{ s.title }}</span>
              <span v-if="s.summary" class="muted">{{ s.summary }}</span>
            </span>
            <TagPill tone="neutral">{{ maturityLabel[s.maturity] }}</TagPill>
          </button>
        </li>
      </ul>
    </template>

    <!-- 症状详情 -->
    <template v-else>
      <div>
        <button type="button" class="btn btn-ghost back" @click="back">
          <AppIcon name="back" :size="18" />返回症状列表
        </button>
      </div>

      <p v-if="!detail && !detailError" class="loading-line" role="status"><BusySpinner size="small" />正在读取…</p>

      <div v-else-if="detailError" class="banner banner-error" role="alert">
        <div>
          <p class="banner-title">没能打开这个症状</p>
          <p>{{ detailError }}</p>
        </div>
      </div>

      <template v-else-if="detail">
        <header class="page-header">
          <div class="title-row">
            <h1 ref="detailHeading" class="page-title" tabindex="-1">{{ detail.title }}</h1>
            <TagPill tone="info">{{ maturityLabel[detail.maturity] }}</TagPill>
          </div>
          <p v-if="detail.summary" class="page-lead">{{ detail.summary }}</p>
        </header>

        <section v-if="detail.causes.length" class="card block" aria-labelledby="causes-title">
          <h2 id="causes-title" class="section-title">可能的原因</h2>
          <ul class="causes">
            <li v-for="(c, i) in detail.causes" :key="i">{{ c }}</li>
          </ul>
        </section>

        <section v-if="detail.steps.length" class="card block" aria-labelledby="steps-title">
          <div class="steps-head">
            <h2 id="steps-title" class="section-title">检查步骤</h2>
            <button type="button" class="btn btn-primary" :disabled="checking" @click="startCheck">
              <BusySpinner v-if="checking" size="small" />
              {{ checking ? '正在检查…' : checkedOnce ? '重新检查' : '开始检查' }}
            </button>
          </div>

          <p v-if="checkedOnce && !checking" class="muted" role="status">
            <template v-if="fixCount === 0 && unclearCount === 0">{{ allOkText }}</template>
            <template v-else>
              查完了。<template v-if="fixCount">亮橙灯、红灯的那几步，下面有可以试的办法。</template>
              <template v-if="unclearCount">灰灯的那几步没查清楚，可以再查一次。</template>
            </template>
          </p>

          <ol class="steps">
            <li
              v-for="v in stepViews"
              :key="`${v.step.check}-${v.index}`"
              class="step"
              :id="stepId(v.index)"
              tabindex="-1"
            >
              <StatusLamp :state="v.lamp" />
              <div class="step-body">
                <p class="step-title">{{ v.step.checkTitle }}</p>
                <p v-if="v.result" class="step-message">{{ v.result.message }}</p>
                <p v-else-if="v.error" class="danger-text small">这一步没能检查：{{ v.error }}</p>
                <p v-else-if="v.skipped" class="muted small">前面已经找到原因，这一步不用查了。</p>

                <div v-if="v.kind === 'fix'" class="step-fix">
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
                          <TagPill :tone="riskTone[fix.risk]" dot>{{ riskLabel[fix.risk] }}</TagPill>
                        </p>
                        <p class="muted small">{{ fix.description }}</p>
                        <p v-if="!fix.applicable" class="small">
                          这台电脑用不了：{{ fix.notApplicableReason ?? '这台电脑的系统不支持这一项。' }}
                        </p>
                      </div>
                      <button
                        v-if="fix.applicable"
                        type="button"
                        class="btn btn-secondary btn-small"
                        @click="openPreview(fix.id, v.index)"
                      >
                        预览
                      </button>
                    </li>
                  </ul>
                  <p v-else class="muted small">
                    这一步小药箱没有自动修复的办法{{ detail.guide ? '，可以看看下面的手动步骤' : '' }}。
                  </p>
                </div>

                <!-- 正常（或不适用），但结果里有提示：照样显示，不列修复办法 -->
                <div v-else-if="v.result && hasHint(v)" class="step-fix">
                  <p v-if="v.result.next" class="small"><span class="muted">提示：</span>{{ v.result.next }}</p>
                  <ResultLinks :links="hintLinks(v.result)" @preview="(featureId) => openPreview(featureId, v.index)" />
                </div>

                <!-- 没查清楚：不给修复，先再查一次 -->
                <div v-else-if="v.kind === 'unclear'" class="step-fix">
                  <p class="small">
                    这一步没查清楚，先别急着修。可以再查一次；一直查不清楚的话，{{
                      detail.guide ? '照着下面的手动步骤试试' : '可以问问懂哥'
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
        <FileLockers v-if="detail.id === 'file-in-use'" class="block" />
        <ExeCheck v-if="detail.id === 'app-cannot-run'" class="block" />

        <section v-if="showsStartup(detail.id)" class="card block" aria-labelledby="startup-title">
          <div class="steps-head">
            <h2 id="startup-title" class="section-title">管理开机启动项</h2>
            <button type="button" class="btn btn-secondary btn-small" :disabled="startupLoading || !!startupBusy" @click="loadStartup()">刷新列表</button>
          </div>
          <p class="muted small">这里列的是注册表 Run 项和「启动」文件夹里的启动项。开关和任务管理器的「启动应用」是同一个：在这里停用的，任务管理器里也显示为已禁用，在那边改的这里也看得到。只停用、不删除，软件本身还在，想用时照样能打开；每一次改动都记在「修改日志」里。从应用商店装的软件，在 Windows 设置里管理。</p>
          <button type="button" class="btn btn-secondary btn-small" @click="openStartupSettings()">管理其他启动应用</button>
          <p v-if="startupLoading" class="loading-line" role="status"><BusySpinner size="small" />正在读取启动项…</p>
          <p v-if="startupError" class="danger-text small" role="alert">{{ startupError }}</p>
          <p v-if="startupNotice" class="small" role="status">{{ startupNotice }}</p>
          <p v-if="!startupLoading && !startupError && startupItems.length === 0" class="muted small">没有找到开机启动项。</p>
          <ul v-if="startupItems.length" class="fixes">
            <li v-for="item in startupItems" :key="item.id" class="fix">
              <div class="fix-main">
                <p class="fix-title">
                  {{ item.title }}
                  <TagPill :tone="item.enabled ? 'info' : 'neutral'">{{ item.enabled ? '开机自动启动' : '已停用' }}</TagPill>
                  <TagPill v-if="item.advice === 'keep'" tone="advice">建议保留</TagPill>
                </p>
                <p class="muted small">{{ startupPublisher(item) }} · {{ item.location }}</p>
                <p class="small">{{ item.reason }}</p>
                <p v-if="item.path" class="muted small startup-command" :title="item.path">{{ item.path }}</p>
              </div>
              <button type="button" class="btn btn-secondary btn-small" :disabled="!!startupBusy" @click="changeStartup(item)">
                <BusySpinner v-if="startupBusy === item.id" size="small" />{{ item.enabled ? '停用自启' : '恢复自启' }}
              </button>
            </li>
          </ul>
        </section>

        <section v-if="detail.guide" class="card block" aria-labelledby="guide-title">
          <h2 id="guide-title" class="section-title">手动步骤</h2>
          <p class="pre-text guide">{{ detail.guide }}</p>
          <ResultLinks v-if="detail.links.length > 0" :links="detail.links" :kinds="['tool', 'symptom', 'test']" />
        </section>
      </template>
    </template>

    <PreviewDialog v-if="previewId" :feature-id="previewId" @close="previewId = null" @applied="onApplied" />
  </div>
</template>

<style scoped>
.search-input {
  font-size: var(--text-large);
  min-height: 48px;
}

.empty {
  padding: 8px 2px;
}

.symptom-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  list-style: none;
}

.symptom {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  width: 100%;
  text-align: left;
  cursor: pointer;
}

.symptom:hover {
  border-color: var(--color-primary);
}

.symptom-main {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.symptom-title {
  font-size: var(--text-large);
  font-weight: 600;
}

.back {
  padding-left: 8px;
}

.title-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
}

.title-row h1:focus {
  outline: none;
}

.block {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.causes {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-left: 1.3em;
}

.steps-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.steps {
  display: flex;
  flex-direction: column;
  list-style: none;
}

.step {
  display: flex;
  gap: 14px;
  padding: 12px 0;
  border-top: 1px solid var(--color-border);
}

.step:first-child {
  border-top: none;
}

.step-body {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
  flex: 1;
}

.step-title {
  font-weight: 600;
}

.step-fix {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 4px;
}

.fixes {
  display: flex;
  flex-direction: column;
  gap: 8px;
  list-style: none;
  margin-top: 4px;
}

.fix {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 10px 14px;
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

.startup-command {
  overflow-wrap: anywhere;
}

.fix-title {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  font-weight: 600;
}

.guide {
  line-height: 1.8;
}
</style>
