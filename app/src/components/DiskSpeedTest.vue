<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { diskSpeedDrives, diskSpeedRun, isTauri } from '../api'
import type { DriveView, SpeedResult, SpeedVerdict } from '../api/types'
import { errorText } from '../utils/format'
import { formatBytes } from '../utils/imageBatch'
import BusySpinner from './BusySpinner.vue'

// 硬盘测速：顺序写、顺序读、4 KB 随机读（后端 medkit_core::disk_speed：不经过系统缓存，只写一个关掉就删的临时文件）。

// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件
defineProps<{ hideTitle?: boolean }>()

const drives = ref<DriveView[]>([])
const chosen = ref('')
const loading = ref(false)
const running = ref(false)
const error = ref('')
const result = ref<{ letter: string; speed: SpeedResult } | null>(null)

const VERDICTS: Record<SpeedVerdict, string> = {
  nvme: 'NVMe 固态硬盘的水平：很快。',
  'sata-ssd': 'SATA 固态硬盘的水平：日常用足够快。',
  'ssd-slow': '像是比较慢的固态硬盘，或者接在 USB 上的固态硬盘、比较快的 U 盘。',
  hdd: '机械硬盘的水平：开机、打开软件会比较慢。系统装在这种盘上的，换一块固态硬盘是最见效的提速办法。',
  slow: '很慢：像是 U 盘、SD 卡的速度。如果这是电脑自己的硬盘，可能出了问题，看看体检里的「硬盘健康」「硬盘读写错误」。',
}

const chosenDrive = computed(() => drives.value.find((d) => d.letter === chosen.value) ?? null)

function driveName(d: DriveView): string {
  const kind = d.system ? '系统盘' : d.removable ? 'U 盘、移动硬盘' : '本地磁盘'
  return `${d.letter}:${d.label ? ` ${d.label}` : ''}（${kind}）`
}

function speed(value: number): string {
  if (value >= 100) return `${Math.round(value)} MB/s`
  if (value >= 10) return `${value.toFixed(1)} MB/s`
  return `${value.toFixed(2)} MB/s`
}

async function load(): Promise<void> {
  loading.value = true
  error.value = ''
  try {
    drives.value = await diskSpeedDrives()
    if (!drives.value.some((d) => d.letter === chosen.value && d.canTest)) {
      chosen.value = drives.value.find((d) => d.system && d.canTest)?.letter ?? drives.value.find((d) => d.canTest)?.letter ?? ''
    }
  } catch (e) {
    error.value = errorText(e)
  } finally {
    loading.value = false
  }
}

async function run(): Promise<void> {
  if (running.value || !chosen.value) return
  running.value = true
  error.value = ''
  result.value = null
  const letter = chosen.value
  try {
    result.value = { letter, speed: await diskSpeedRun(letter) }
  } catch (e) {
    error.value = errorText(e)
  } finally {
    running.value = false
  }
}

onMounted(load)
</script>

<template>
  <section class="group" aria-labelledby="disk-speed-title">
    <div class="card speed-card">
      <h2 id="disk-speed-title" :class="hideTitle ? 'visually-hidden' : 'section-title'">硬盘测速</h2>
      <p class="muted small">
        看看硬盘、U 盘实际读写有多快：验机、换了固态硬盘以后用，也能看出硬盘是不是慢得不正常。会在这个盘上写一个临时文件（最多 1 GB，测完马上删掉，对硬盘寿命没有影响），大约要 20 秒；测的时候别复制大文件。
      </p>
      <p v-if="!isTauri()" class="muted small">浏览器里是演示：给的是几种盘的典型速度，不会真的测。</p>
      <p v-if="loading" class="muted small" role="status">正在列出这台电脑上的盘…</p>
      <p v-else-if="!drives.length && !error" class="muted small">没有找到能测的盘。</p>
      <fieldset v-if="drives.length" class="drives" :disabled="running">
        <legend class="field-label">测哪个盘</legend>
        <label v-for="d in drives" :key="d.letter" class="drive" :class="{ off: !d.canTest }">
          <input v-model="chosen" type="radio" name="disk-speed-drive" :value="d.letter" :disabled="!d.canTest" />
          <span>
            <span class="name">{{ driveName(d) }}</span>
            <span class="muted small">
              {{ d.fileSystem || '未知文件系统' }} · 共 {{ formatBytes(d.total) }}，剩 {{ formatBytes(d.free) }}<template v-if="!d.canTest">（不到 2 GB，不能测）</template>
            </span>
          </span>
        </label>
      </fieldset>
      <div class="row">
        <button type="button" class="btn btn-primary btn-small" :disabled="running || !chosenDrive?.canTest" @click="run">
          <BusySpinner v-if="running" size="small" />{{ running ? '正在测…' : '开始测速' }}
        </button>
        <button type="button" class="btn btn-ghost btn-small" :disabled="running || loading" @click="load">刷新列表</button>
      </div>
      <p v-if="running" class="muted small" role="status">正在测 {{ chosen }} 盘：先写、再读、再随机读，大约 20 秒，请稍等。</p>
      <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>

      <template v-if="result && !running">
        <dl class="numbers" aria-label="测速结果">
          <div><dt>顺序读</dt><dd>{{ speed(result.speed.seqRead) }}</dd></div>
          <div><dt>顺序写</dt><dd>{{ speed(result.speed.seqWrite) }}</dd></div>
          <div><dt>4K 随机读</dt><dd>{{ speed(result.speed.randomRead) }}<span class="muted small">（每秒 {{ Math.round(result.speed.randomIops) }} 次）</span></dd></div>
        </dl>
        <p class="small" role="status"><strong>{{ result.letter }} 盘：</strong>{{ VERDICTS[result.speed.verdict] }}</p>
        <p v-if="result.speed.testedBytes < 1024 * 1024 * 1024" class="muted small">这个盘比较慢，时间到了就停了，只测了 {{ formatBytes(result.speed.testedBytes) }}。</p>
      </template>

      <details class="ranges">
        <summary class="small">各种盘大概有多快</summary>
        <table class="small">
          <thead><tr><th scope="col"></th><th scope="col">顺序读</th><th scope="col">4K 随机读</th></tr></thead>
          <tbody>
            <tr><th scope="row">NVMe 固态硬盘</th><td>1500–7000 MB/s</td><td>40–90 MB/s</td></tr>
            <tr><th scope="row">SATA 固态硬盘</th><td>400–560 MB/s</td><td>20–50 MB/s</td></tr>
            <tr><th scope="row">机械硬盘</th><td>80–200 MB/s</td><td>0.3–1.5 MB/s</td></tr>
            <tr><th scope="row">U 盘、SD 卡</th><td>10–200 MB/s</td><td>1–10 MB/s</td></tr>
          </tbody>
        </table>
        <p class="muted small">4K 随机读最能看出是不是固态硬盘：开机、打开软件时就是这么一小块一小块地读。笔记本插着电源测会更准。</p>
      </details>
    </div>
  </section>
</template>

<style scoped>
.group { display: flex; flex-direction: column; gap: 10px; }
.speed-card { display: flex; flex-direction: column; gap: 9px; }
.field-label { font-weight: 600; font-size: var(--text-small); }
.drives { display: flex; flex-direction: column; gap: 6px; border: 0; padding: 0; margin: 0; min-width: 0; }
.drive { display: flex; gap: 9px; align-items: flex-start; padding: 7px 11px; border-radius: var(--radius); background: var(--color-surface-2); }
.drive > span { display: flex; flex-direction: column; gap: 2px; }
.drive.off { opacity: 0.6; }
.name { font-weight: 600; }
.row { display: flex; flex-wrap: wrap; gap: 8px 12px; align-items: center; }
.numbers { display: flex; flex-wrap: wrap; gap: 10px 28px; margin: 4px 0 0; }
.numbers div { display: flex; flex-direction: column; gap: 2px; }
.numbers dt { font-size: var(--text-small); color: var(--color-text-muted); }
.numbers dd { margin: 0; font-size: 1.25rem; font-weight: 600; }
.ranges summary { cursor: pointer; }
.ranges table { border-collapse: collapse; margin-top: 6px; }
.ranges th, .ranges td { padding: 4px 12px 4px 0; text-align: left; }
</style>
