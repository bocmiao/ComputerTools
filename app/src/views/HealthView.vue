<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { runCheck, runProfile } from '../api'
import type { CheckResult } from '../api/types'
import BusySpinner from '../components/BusySpinner.vue'
import CheckResultCard from '../components/CheckResultCard.vue'
import PreviewDialog from '../components/PreviewDialog.vue'
import AppIcon from '../components/AppIcon.vue'
import { statusOrder, type ShownStatus } from '../labels'
import { catalog, health } from '../state'
import { errorText } from '../utils/format'

// 体检：只读，不改任何东西。不打分，不说「发现 N 个问题」；没问题就说一切正常。
// 体检以后在别的页面改过或撤销过设置，结果就可能过期了：提示一下，给「重新体检」。

const PROFILE_ID = 'healthcheck'

/** 体检清单一共几项（从目录里读，不写死） */
const checkCount = computed(() => catalog.value?.profiles.find((p) => p.id === PROFILE_ID)?.checkCount ?? null)

type ShownResult = CheckResult & { status: ShownStatus }

const results = ref<CheckResult[] | null>(null)
const running = ref(false)
const error = ref<string | null>(null)
const elapsed = ref(0)
const previewId = ref<string | null>(null)
/** 从哪张卡片打开的预览：修完以后重新查这一项 */
let previewFromCheck: string | null = null
let timer: ReturnType<typeof setInterval> | undefined

function isShown(r: CheckResult): r is ShownResult {
  return r.status !== 'na'
}

/** 不适用的不显示；需要人工、建议处理、没查出来在前，正常在后 */
const shown = computed<ShownResult[]>(() =>
  (results.value ?? [])
    .filter(isShown)
    .map((r, i) => ({ r, i }))
    .sort((a, b) => statusOrder[a.r.status] - statusOrder[b.r.status] || a.i - b.i)
    .map((x) => x.r),
)
const attention = computed(() => shown.value.filter((r) => r.status !== 'ok'))
const normal = computed(() => shown.value.filter((r) => r.status === 'ok'))
const allOk = computed(() => results.value !== null && attention.value.length === 0)

function stopTimer(): void {
  if (timer !== undefined) clearInterval(timer)
  timer = undefined
}

async function start(): Promise<void> {
  if (running.value) return
  running.value = true
  error.value = null
  elapsed.value = 0
  // 开始时就清掉：体检进行中又改了设置的话，会重新标成过期
  health.stale = false
  const startedAt = Date.now()
  timer = setInterval(() => {
    elapsed.value = Math.floor((Date.now() - startedAt) / 1000)
  }, 500)
  try {
    results.value = await runProfile(PROFILE_ID)
    health.lastRunAt = new Date().toISOString()
  } catch (e) {
    error.value = errorText(e)
  } finally {
    stopTimer()
    running.value = false
  }
}

function openPreview(featureId: string, checkId: string): void {
  previewId.value = featureId
  previewFromCheck = checkId
}

/** 修完以后重新查一下这一项，卡片跟着更新 */
async function onApplied(): Promise<void> {
  const checkId = previewFromCheck
  if (!checkId || !results.value) return
  try {
    const fresh = await runCheck(checkId)
    if (results.value) results.value = results.value.map((r) => (r.id === checkId ? fresh : r))
  } catch {
    // 复查失败就保留原来的结果，不打扰用户
  }
}

// 报告页点了「重新体检」
watch(
  () => health.runRequest,
  () => void start(),
)

onBeforeUnmount(stopTimer)
</script>

<template>
  <div class="page">
    <header class="page-header">
      <h1 class="page-title" tabindex="-1">体检</h1>
      <p class="page-lead">只看不改，大约半分钟。没坏的别修，看不懂的就跳过。</p>
    </header>

    <section class="hero card" aria-live="polite">
      <template v-if="running">
        <div class="running">
          <BusySpinner size="large" />
          <div>
            <p class="running-title">正在体检…</p>
            <p class="muted">已经用了 {{ elapsed }} 秒。体检只读取信息，不会改动电脑。</p>
          </div>
        </div>
      </template>

      <template v-else-if="error">
        <div class="banner banner-error" role="alert">
          <div>
            <p class="banner-title">体检没能完成</p>
            <p>{{ error }}</p>
          </div>
        </div>
        <button type="button" class="btn btn-primary btn-big" @click="start">再试一次</button>
      </template>

      <template v-else-if="results === null">
        <p class="hero-text">
          把这台电脑常见的毛病查一遍{{ checkCount ? `（共 ${checkCount} 项）` : '' }}，查完告诉你哪些正常、哪些需要留意。
        </p>
        <button type="button" class="btn btn-primary btn-big" @click="start">
          <AppIcon name="health" :size="24" />开始体检
        </button>
      </template>

      <template v-else-if="allOk">
        <div class="all-ok">
          <span class="all-ok-icon"><AppIcon name="check" :size="30" /></span>
          <div>
            <p class="all-ok-title">一切正常</p>
            <p class="muted">查了 {{ shown.length }} 项，都没有问题，平时照常用就好。</p>
          </div>
        </div>
        <button type="button" class="btn btn-secondary" @click="start">重新体检</button>
      </template>

      <template v-else>
        <div class="summary">
          <p class="summary-title">体检做完了</p>
          <p class="muted">需要留意的排在前面，正常的放在后面。修不修由你决定，看不懂的就跳过。</p>
        </div>
        <button type="button" class="btn btn-secondary" @click="start">重新体检</button>
      </template>
    </section>

    <div v-if="health.stale && results && !running" class="banner banner-warning" role="status">
      <AppIcon name="warning" :size="20" />
      <div class="stale">
        <p class="banner-title">下面的体检结果可能已经过期</p>
        <p>体检以后，你在「按症状修」「常用设置」或「修改日志」里改过或撤销过设置。重新体检一次，看到的才是现在的情况。</p>
        <div>
          <button type="button" class="btn btn-secondary btn-small" @click="start">重新体检</button>
        </div>
      </div>
    </div>

    <template v-if="results && !running">
      <section v-if="attention.length" class="group" aria-label="需要留意的项目">
        <CheckResultCard
          v-for="r in attention"
          :key="r.id"
          :result="r"
          @preview="(featureId) => openPreview(featureId, r.id)"
        />
      </section>

      <section v-if="normal.length" class="group" aria-labelledby="health-normal-title">
        <h2 v-if="attention.length" id="health-normal-title" class="group-title">这些都正常</h2>
        <h2 v-else id="health-normal-title" class="visually-hidden">检查过的项目</h2>
        <CheckResultCard
          v-for="r in normal"
          :key="r.id"
          :result="r"
          @preview="(featureId) => openPreview(featureId, r.id)"
        />
      </section>
    </template>

    <PreviewDialog v-if="previewId" :feature-id="previewId" @close="previewId = null" @applied="onApplied" />
  </div>
</template>

<style scoped>
.hero {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 16px 24px;
  padding: 24px 28px;
}

.hero-text {
  flex: 1 1 280px;
  font-size: var(--text-large);
}

.running {
  display: flex;
  align-items: center;
  gap: 18px;
}

.running-title,
.summary-title {
  font-size: 19px;
  font-weight: 600;
}

.summary {
  flex: 1 1 320px;
}

.all-ok {
  display: flex;
  align-items: center;
  gap: 16px;
}

.all-ok-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 52px;
  height: 52px;
  border-radius: 50%;
  background: var(--tone-ok-bg);
  color: var(--tone-ok-text);
}

.all-ok-title {
  font-size: 24px;
  font-weight: 600;
  color: var(--tone-ok-text);
}

.group {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.group-title {
  margin-top: 8px;
  font-size: var(--text-base);
  color: var(--color-text-muted);
}

.stale {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.stale .btn {
  margin-top: 4px;
}
</style>
