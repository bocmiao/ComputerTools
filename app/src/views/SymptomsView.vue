<script setup lang="ts">
import { computed, nextTick, ref, useTemplateRef, watch } from 'vue'
import { runCheck, symptomDetail } from '../api'
import type { CheckResult, SymptomDetail, SymptomSummary } from '../api/types'
import AppIcon from '../components/AppIcon.vue'
import BusySpinner from '../components/BusySpinner.vue'
import PreviewDialog from '../components/PreviewDialog.vue'
import StatusLamp, { type LampState } from '../components/StatusLamp.vue'
import TagPill from '../components/TagPill.vue'
import { fixerLabel, maturityLabel, riskLabel, riskTone } from '../labels'
import { catalog, nav } from '../state'
import { errorText, normalizeForSearch } from '../utils/format'

// 按症状修：先搜症状，再逐步检查，哪一步有问题就给出对应的修复。

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
  const token = runToken
  try {
    const d = await symptomDetail(id)
    if (token !== runToken) return
    detail.value = d
    runs.value = d.steps.map(() => ({ state: 'pending', result: null, error: null }))
  } catch (e) {
    if (token !== runToken) return
    detailError.value = errorText(e)
  }
  await nextTick()
  detailHeading.value?.focus()
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
  run.state = 'running'
  run.error = null
  try {
    const r = await runCheck(step.check)
    if (token !== runToken) return
    run.result = r
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

/** 这一步查完了、结果不是正常（也不是「不适用」）时，显示它的修复办法 */
function needsFix(run: StepRun | undefined): boolean {
  if (!run) return false
  if (run.state === 'error') return true
  return run.state === 'done' && !!run.result && run.result.status !== 'ok' && run.result.status !== 'na'
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
      showFix: needsFix(run),
    }
  }),
)

const problemCount = computed(() => stepViews.value.filter((v) => v.showFix).length)

// ── 预览修复 ──

const previewId = ref<string | null>(null)
let previewStep: number | null = null

function openPreview(featureId: string, stepIndex: number): void {
  previewId.value = featureId
  previewStep = stepIndex
}

/** 修完以后重新查这一步 */
function onApplied(): void {
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
        <h1 class="page-title">按症状修</h1>
        <p class="page-lead">说说电脑哪里不对劲，比如「没网」「C 盘满了」「打印机」。先检查，找到原因再动手。</p>
      </header>

      <div class="search">
        <label for="symptom-search" class="visually-hidden">搜索症状</label>
        <input
          id="symptom-search"
          ref="searchInput"
          v-model="query"
          type="search"
          class="input search-input"
          placeholder="输入你遇到的情况，比如：没网"
          autocomplete="off"
        />
      </div>

      <p v-if="!catalog" class="loading-line" role="status"><BusySpinner size="small" />正在读取症状列表…</p>
      <p v-else-if="filtered.length === 0" class="empty muted" role="status">
        没找到相关的症状。换个说法试试，比如「上不了网」「C 盘红了」「打印机连不上」。
      </p>
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
            <template v-if="problemCount === 0">
              查过的几项都正常。{{ detail.guide ? '问题还在的话，照着下面的手动步骤试试。' : '' }}
            </template>
            <template v-else>查完了。亮橙灯、红灯或灰灯的那几步，下面有可以试的办法。</template>
          </p>

          <ol class="steps">
            <li v-for="v in stepViews" :key="`${v.step.check}-${v.index}`" class="step">
              <StatusLamp :state="v.lamp" />
              <div class="step-body">
                <p class="step-title">{{ v.step.checkTitle }}</p>
                <p v-if="v.result" class="step-message">{{ v.result.message }}</p>
                <p v-else-if="v.error" class="danger-text small">这一步没能检查：{{ v.error }}</p>
                <p v-else-if="v.skipped" class="muted small">前面已经找到原因，这一步不用查了。</p>

                <div v-if="v.showFix" class="step-fix">
                  <p v-if="v.result?.error" class="muted small">出错信息：{{ v.result.error }}</p>
                  <p v-if="v.result?.fixer" class="small"><span class="muted">谁能修：</span>{{ fixerLabel[v.result.fixer] }}</p>
                  <p v-if="v.result?.next" class="small"><span class="muted">下一步：</span>{{ v.result.next }}</p>

                  <ul v-if="v.step.fixes.length" class="fixes">
                    <li v-for="fix in v.step.fixes" :key="fix.id" class="fix">
                      <div class="fix-main">
                        <p class="fix-title">
                          {{ fix.title }}
                          <TagPill :tone="riskTone[fix.risk]" dot>{{ riskLabel[fix.risk] }}</TagPill>
                        </p>
                        <p class="muted small">{{ fix.description }}</p>
                      </div>
                      <button type="button" class="btn btn-secondary btn-small" @click="openPreview(fix.id, v.index)">
                        预览
                      </button>
                    </li>
                  </ul>
                  <p v-else class="muted small">
                    这一步小药箱没有自动修复的办法{{ detail.guide ? '，可以看看下面的手动步骤' : '' }}。
                  </p>
                </div>
              </div>
            </li>
          </ol>
        </section>

        <section v-if="detail.guide" class="card block" aria-labelledby="guide-title">
          <h2 id="guide-title" class="section-title">手动步骤</h2>
          <p class="pre-text guide">{{ detail.guide }}</p>
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
