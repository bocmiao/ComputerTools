<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { brightnessList, brightnessSet, type MonitorBrightness } from '../api'
import { errorText } from '../utils/format'
import BusySpinner from './BusySpinner.vue'

// 显示器亮度：台式机接的显示器用电脑调亮度（DDC/CI），不用去按显示器上的按钮。拖动滑块松手（或者按方向键）以后才调：
// DDC/CI 很慢，拖的过程中不连续发；正在调的时候又动了滑块，记下最后的值，调完再调一次。调完以显示器读回来的亮度为准。
// 笔记本自带的屏幕、不支持的显示器也列出来，说明用哪个键、哪个按钮调。改的是显示器自己的亮度，不记修改日志。

// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件
defineProps<{ hideTitle?: boolean }>()

const monitors = ref<MonitorBrightness[]>([])
const loading = ref(false)
const loaded = ref(false)
const error = ref('')
/** 拖动、按方向键时显示的值（还没调好的） */
const draft = ref<Record<string, number>>({})
/** 正在调的显示器 */
const busyId = ref<string | null>(null)
/** 调的时候又动了的滑块：显示器 → 最后的值 */
const queued = new Map<string, number>()

async function refresh(): Promise<void> {
  if (loading.value || busyId.value !== null) return
  loading.value = true
  error.value = ''
  try {
    monitors.value = await brightnessList()
    draft.value = {}
  } catch (e) {
    error.value = errorText(e)
  } finally {
    loading.value = false
    loaded.value = true
  }
}

function label(m: MonitorBrightness, index: number): string {
  if (m.name) return `显示器「${m.name}」`
  if (m.internal) return '电脑自带的屏幕'
  return monitors.value.length > 1 ? `第 ${index + 1} 个显示器` : '显示器'
}

function shown(m: MonitorBrightness): number {
  return draft.value[m.id] ?? m.percent ?? 0
}

function onInput(m: MonitorBrightness, e: Event): void {
  draft.value = { ...draft.value, [m.id]: Number((e.target as HTMLInputElement).value) }
}

function clearDraft(id: string): void {
  const rest = { ...draft.value }
  delete rest[id]
  draft.value = rest
}

async function apply(m: MonitorBrightness): Promise<void> {
  const target = draft.value[m.id]
  if (target === undefined) return
  if (busyId.value !== null) {
    queued.set(m.id, target)
    return
  }
  busyId.value = m.id
  error.value = ''
  try {
    m.percent = await brightnessSet(m.id, target)
  } catch (e) {
    error.value = errorText(e)
  } finally {
    busyId.value = null
  }
  if (!queued.has(m.id)) clearDraft(m.id)
  // 调的时候又动了的滑块（这一个或者别的显示器的）：按最后的值再调
  const waiting = [...queued.entries()]
  queued.clear()
  for (const [id, value] of waiting) {
    const other = monitors.value.find((x) => x.id === id)
    if (!other) continue
    draft.value = { ...draft.value, [id]: value }
    await apply(other)
  }
}

onMounted(refresh)
</script>

<template>
  <section class="group" aria-labelledby="brightness-title">
    <div class="card brightness-card">
      <h2 id="brightness-title" :class="hideTitle ? 'visually-hidden' : 'section-title'">显示器亮度</h2>
      <p class="muted small">
        台式机接的显示器可以在这里直接调亮度，不用去按显示器上的按钮（要显示器支持 DDC/CI，大多数显示器都支持，一般默认开着）。改的是显示器自己的亮度，和按显示器上的按钮一样，拖回去就行。笔记本自带的屏幕用键盘上画着太阳的键，或者「设置 → 屏幕」里的亮度条调。
      </p>
      <p v-if="loading && !loaded" class="loading-line" role="status"><BusySpinner size="small" />正在读显示器的亮度…</p>
      <ul v-else-if="monitors.length" class="monitor-list">
        <li v-for="(m, i) in monitors" :key="m.id" class="monitor">
          <span class="monitor-name">{{ label(m, i) }}</span>
          <template v-if="m.percent !== null">
            <input
              type="range"
              min="0"
              max="100"
              step="1"
              :value="shown(m)"
              :aria-label="`${label(m, i)}的亮度`"
              @input="onInput(m, $event)"
              @change="apply(m)"
            />
            <span class="monitor-value">{{ shown(m) }}%</span>
            <BusySpinner v-if="busyId === m.id" size="small" />
          </template>
          <span v-else class="muted small">
            {{ m.internal ? '笔记本自带的屏幕在这里调不了，用键盘上的亮度键。' : '这台显示器不让电脑调：用显示器上的按钮调，或者在显示器的菜单里打开「DDC/CI」以后点「重新读取」。' }}
          </span>
        </li>
      </ul>
      <p v-else-if="loaded && !error" class="muted small">没找到显示器。</p>
      <div class="row">
        <button type="button" class="btn btn-secondary btn-small" :disabled="loading || busyId !== null" @click="refresh">
          {{ loading ? '正在读取…' : '重新读取' }}
        </button>
      </div>
      <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>
    </div>
  </section>
</template>

<style scoped>
.group { display: flex; flex-direction: column; }
.brightness-card { display: flex; flex-direction: column; gap: 8px; }
.monitor-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 10px; }
.monitor { display: flex; flex-wrap: wrap; align-items: center; gap: 10px; }
.monitor-name { min-width: 10em; font-weight: 600; }
.monitor input[type='range'] { flex: 1 1 180px; max-width: 320px; }
.monitor-value { min-width: 3.5em; font-variant-numeric: tabular-nums; }
.row { display: flex; gap: 10px; }
</style>
