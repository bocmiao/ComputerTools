<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'
import { isTauri, popupFind, popupReveal } from '../api'
import type { WindowOwnerReport } from '../api/types'
import { errorText } from '../utils/format'
import { canReveal, ownerAdvice, ownerLinks, ownerSummary } from '../utils/windowOwner'
import BusySpinner from './BusySpinner.vue'
import ResultLinks from './ResultLinks.vue'

// 弹窗是哪个软件的：点「开始找」，倒数几秒，这段时间里把鼠标移到弹窗上停住，看鼠标指着的窗口是哪个程序的、
// 属于哪个已安装的软件。只查不改：不关窗口、不结束程序。路径里的用户文件夹名后端已经换成 *。

/** 等几秒再看（后端最多等 10 秒） */
const WAIT_SECONDS = 5

const report = ref<WindowOwnerReport | null>(null)
const busy = ref(false)
const left = ref(0)
const error = ref('')
const revealText = ref('')
let timer: ReturnType<typeof setInterval> | undefined

function stopTimer(): void {
  if (timer !== undefined) clearInterval(timer)
  timer = undefined
  left.value = 0
}
onBeforeUnmount(stopTimer)

async function start(): Promise<void> {
  if (busy.value) return
  busy.value = true
  error.value = ''
  revealText.value = ''
  report.value = null
  left.value = WAIT_SECONDS
  timer = setInterval(() => {
    if (left.value > 0) left.value--
  }, 1000)
  try {
    report.value = await popupFind(WAIT_SECONDS)
  } catch (e) {
    error.value = errorText(e)
  } finally {
    stopTimer()
    busy.value = false
  }
}

async function reveal(): Promise<void> {
  const shown = report.value
  revealText.value = ''
  let text: string
  try {
    await popupReveal()
    text = '已经在资源管理器里打开了它所在的文件夹。'
  } catch (e) {
    text = errorText(e)
  }
  // 等着的时候又点了「再找一次」：这句话说的是上一个结果，不显示
  if (report.value === shown && !busy.value) revealText.value = text
}

const advice = computed(() => (report.value ? ownerAdvice(report.value) : []))
const links = computed(() => (report.value ? ownerLinks(report.value) : []))
</script>

<template>
  <article class="card popup-card">
    <h3 class="section-title">弹窗是哪个软件的</h3>
    <p class="muted small">
      右下角、屏幕中间老弹广告、资讯，不知道是哪个软件弹的？等它弹出来，点「开始找」，在倒数完之前把鼠标移到弹窗上停住（不用点它），就能看出是哪个程序弹的、属于哪个已安装的软件。只查不改：不关窗口、不结束程序。
    </p>
    <p v-if="!isTauri()" class="muted small">浏览器里显示的是演示结果；在 Windows 的小药箱里查真实的窗口。</p>
    <div class="row">
      <button type="button" class="btn btn-secondary btn-small" :disabled="busy" @click="start">
        {{ busy ? '正在找…' : report ? '再找一次' : '开始找' }}
      </button>
    </div>
    <p v-if="busy" class="loading-line small" role="status">
      <BusySpinner size="small" />{{
        left > 0 ? `还有 ${left} 秒：现在把鼠标移到弹窗上停住` : '正在看鼠标指着的是哪个程序…'
      }}
    </p>
    <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>

    <template v-if="report && !busy">
      <p class="small summary" role="status">{{ ownerSummary(report) }}</p>
      <dl v-if="report.exe" class="facts small">
        <template v-if="report.description">
          <dt>说明</dt>
          <dd>{{ report.description }}</dd>
        </template>
        <dt>程序文件</dt>
        <dd>{{ report.exe }}</dd>
        <template v-if="report.company">
          <dt>公司</dt>
          <dd>{{ report.company }}</dd>
        </template>
        <template v-if="report.installed">
          <dt>属于</dt>
          <dd>{{ report.installed }}</dd>
        </template>
        <template v-if="report.folder">
          <dt>在哪</dt>
          <dd class="path">{{ report.folder }}</dd>
        </template>
      </dl>
      <ul v-if="advice.length" class="tips small">
        <li v-for="a in advice" :key="a">{{ a }}</li>
      </ul>
      <div v-if="canReveal(report)" class="row">
        <button type="button" class="btn btn-secondary btn-small" @click="reveal">打开所在的文件夹</button>
      </div>
      <p v-if="revealText" class="muted small" role="status">{{ revealText }}</p>
      <ResultLinks :links="links" />
    </template>
  </article>
</template>

<style scoped>
.popup-card { display: flex; flex-direction: column; gap: 9px; }
.row { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
.summary { font-weight: 600; }
.facts {
  display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 3px 12px;
  padding: 9px 12px; border-radius: var(--radius); background: var(--color-surface-2);
}
.facts dt { color: var(--color-text-muted); }
.path { overflow-wrap: anywhere; }
.tips { display: flex; flex-direction: column; gap: 4px; padding-left: 1.2em; }
</style>
