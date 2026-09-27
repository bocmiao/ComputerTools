<script setup lang="ts">
import { computed, onActivated, onDeactivated, onMounted, onUnmounted, ref } from 'vue'
import { shutdownCancel, shutdownGet, shutdownSchedule, type ShutdownPlan } from '../api'
import { errorText } from '../utils/format'
import {
  MAX_MINUTES,
  PRESET_MINUTES,
  cancelText,
  delayAfter,
  delayUntil,
  durationText,
  planText,
  previewText,
} from '../utils/shutdownTimer'

// 定时关机、定时重启：到时间 Windows 自己关（退出小药箱也照样关），到时间以前随时能取消。
// 系统查不到别处安排的关机，这里只显示小药箱安排的那一次；「取消」总是能点。

const restart = ref(false)
const mode = ref<'after' | 'at'>('after')
const minutes = ref(60)
const time = ref('23:00')
const plan = ref<ShutdownPlan | null>(null)
const now = ref(new Date())
const busy = ref(false)
const error = ref('')
const notice = ref('')
let ticker: ReturnType<typeof setInterval> | undefined

const delay = computed(() =>
  mode.value === 'after' ? delayAfter(Number(minutes.value), now.value) : delayUntil(time.value, now.value),
)
const preview = computed(() => previewText(delay.value, restart.value, now.value))
const invalid = computed(() => 'error' in delay.value)
const what = computed(() => (restart.value ? '重启' : '关机'))

async function refresh(): Promise<void> {
  now.value = new Date()
  try {
    plan.value = (await shutdownGet()).plan
  } catch (e) {
    error.value = errorText(e)
  }
}

async function schedule(): Promise<void> {
  const d = delay.value
  if (busy.value || 'error' in d) return
  busy.value = true
  error.value = ''
  notice.value = ''
  try {
    plan.value = (await shutdownSchedule(d.seconds, restart.value)).plan
  } catch (e) {
    error.value = errorText(e)
    await refresh()
  } finally {
    busy.value = false
    now.value = new Date()
  }
}

async function cancel(): Promise<void> {
  if (busy.value) return
  busy.value = true
  error.value = ''
  notice.value = ''
  try {
    notice.value = cancelText((await shutdownCancel()).cancelled)
    plan.value = null
  } catch (e) {
    error.value = errorText(e)
  } finally {
    busy.value = false
  }
}

function pick(value: number): void {
  mode.value = 'after'
  minutes.value = value
}

function start(): void {
  void refresh()
  if (ticker === undefined) ticker = setInterval(() => { now.value = new Date() }, 15_000)
}

function stop(): void {
  if (ticker !== undefined) clearInterval(ticker)
  ticker = undefined
}

onMounted(start)
onActivated(start)
onDeactivated(stop)
onUnmounted(stop)
</script>

<template>
  <section class="group" aria-labelledby="shutdown-timer-title">
    <div class="card timer-card">
      <h2 id="shutdown-timer-title" class="section-title">定时关机、定时重启</h2>
      <p class="muted small">
        下载完、看完电影、晚上忘了关电脑：到时间 Windows 自己关机（或者重启），退出小药箱也照样会关，到时间以前随时能取消。电脑在那之前睡着了就不会关：想让它一直开着等到那时候，把上面的「别让电脑自己睡着」也打开。
      </p>
      <p class="banner banner-warning small" role="note">到时间会直接关掉所有程序，没保存的文档会丢：走开以前先保存好。</p>

      <p v-if="plan" class="success-text small plan" role="status">{{ planText(plan, now) }}</p>

      <fieldset class="choice">
        <legend class="field-label">到时间做什么</legend>
        <label class="check small"><input v-model="restart" type="radio" :value="false" :disabled="busy" /> 关机</label>
        <label class="check small"><input v-model="restart" type="radio" :value="true" :disabled="busy" /> 重启</label>
      </fieldset>

      <fieldset class="choice when">
        <legend class="field-label">什么时候</legend>
        <div class="line">
          <label class="check small"><input v-model="mode" type="radio" value="after" :disabled="busy" /> 多久以后</label>
          <input
            v-model.number="minutes"
            class="input minutes"
            type="number"
            min="1"
            :max="MAX_MINUTES"
            step="1"
            aria-label="多少分钟以后"
            :disabled="busy"
            @focus="mode = 'after'"
          />
          <span class="small">分钟</span>
          <button
            v-for="m in PRESET_MINUTES"
            :key="m"
            type="button"
            class="btn btn-ghost btn-small"
            :disabled="busy"
            @click="pick(m)"
          >
            {{ durationText(m * 60_000) }}
          </button>
        </div>
        <div class="line">
          <label class="check small"><input v-model="mode" type="radio" value="at" :disabled="busy" /> 到几点</label>
          <input v-model="time" class="input clock" type="time" aria-label="几点" :disabled="busy" @focus="mode = 'at'" />
        </div>
      </fieldset>

      <p class="small" :class="invalid ? 'danger-text' : 'muted'">{{ preview }}</p>
      <div class="row">
        <button type="button" class="btn btn-primary btn-small" :disabled="busy || invalid" @click="schedule">
          {{ plan ? '改成这个时间' : `安排${what}` }}
        </button>
        <button v-if="plan" type="button" class="btn btn-secondary btn-small" :disabled="busy" @click="cancel">
          取消{{ plan.restart ? '重启' : '关机' }}
        </button>
        <button v-else type="button" class="btn btn-ghost btn-small" :disabled="busy" @click="cancel">
          取消已经安排的关机或重启
        </button>
      </div>
      <p v-if="notice" class="small" role="status">{{ notice }}</p>
      <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>
    </div>
  </section>
</template>

<style scoped>
.group { display: flex; flex-direction: column; }
.timer-card { display: flex; flex-direction: column; gap: 8px; }
.plan { font-weight: 600; }
.choice { display: flex; flex-wrap: wrap; gap: 6px 16px; align-items: center; border: 0; padding: 0; margin: 0; }
.choice legend { float: left; margin-right: 4px; }
.when { flex-direction: column; align-items: flex-start; }
.line { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
.field-label { font-weight: 600; font-size: var(--text-small); }
.check { display: inline-flex; gap: 6px; align-items: center; }
.input { padding: 6px 9px; border: 1px solid var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface); }
.minutes { width: 6em; }
.clock { width: auto; }
.row { display: flex; flex-wrap: wrap; gap: 10px; align-items: center; }
</style>
