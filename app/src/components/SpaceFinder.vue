<script setup lang="ts">
import { computed, ref } from 'vue'
import { isTauri, spacePickFolder, spaceRescan, spaceReveal } from '../api'
import type { SpaceFile, SpaceReport } from '../api/types'
import { errorText } from '../utils/format'
import { formatBytes } from '../utils/imageBatch'
import { duplicateSummary, spaceNotes, spaceSummary, whereLine } from '../utils/space'
import BusySpinner from './BusySpinner.vue'
import TagPill from './TagPill.vue'

// 找大文件和重复文件：只查不删。要删的话点「显示」，用户自己在资源管理器里删（先进回收站）。
// 文件夹只能在系统的选择框里选；「显示」只传结果里的编号。

// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件
defineProps<{ hideTitle?: boolean }>()

const report = ref<SpaceReport | null>(null)
const busy = ref(false)
const error = ref('')
const view = ref<'largest' | 'duplicates'>('largest')

async function run(action: () => Promise<SpaceReport | null>): Promise<void> {
  if (busy.value) return
  busy.value = true
  error.value = ''
  try {
    const r = await action()
    if (r) report.value = r
  } catch (e) {
    error.value = errorText(e)
  } finally {
    busy.value = false
  }
}

async function reveal(f: SpaceFile): Promise<void> {
  error.value = ''
  try {
    await spaceReveal(f.id)
  } catch (e) {
    error.value = errorText(e)
  }
}

const notes = computed(() => (report.value ? spaceNotes(report.value) : []))
</script>

<template>
  <article class="card space-card">
    <h3 :class="hideTitle ? 'visually-hidden' : 'section-title'">找大文件和重复文件</h3>
    <p class="muted small">
      选一个文件夹（比如「下载」、整个 D 盘），列出里面最大的文件，和内容完全一样的文件（逐字节比较过，不是只看名字）。这里只查不删：要删的话点「显示」，在打开的资源管理器里删，删掉的先进回收站。
    </p>
    <p v-if="!isTauri()" class="muted small">浏览器里显示的是演示结果；在 Windows 的小药箱里查真实的文件。</p>
    <div class="row">
      <button type="button" class="btn btn-secondary btn-small" :disabled="busy" @click="run(spacePickFolder)">选择文件夹</button>
      <button v-if="report" type="button" class="btn btn-secondary btn-small" :disabled="busy" @click="run(spaceRescan)">删完以后再查一遍</button>
    </div>
    <p v-if="busy" class="loading-line small" role="status"><BusySpinner size="small" />正在数文件、比较内容，文件多的话要一两分钟…</p>
    <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>

    <template v-if="report && !busy">
      <p class="small summary" role="status">{{ spaceSummary(report, formatBytes) }}</p>
      <p v-for="n in notes" :key="n" class="muted small">{{ n }}</p>
      <div class="switch" role="group" aria-label="看哪一种">
        <button type="button" class="btn btn-small" :class="view === 'largest' ? 'btn-primary' : 'btn-secondary'" :aria-pressed="view === 'largest'" @click="view = 'largest'">
          最大的文件
        </button>
        <button type="button" class="btn btn-small" :class="view === 'duplicates' ? 'btn-primary' : 'btn-secondary'" :aria-pressed="view === 'duplicates'" @click="view = 'duplicates'">
          重复的文件（{{ report.duplicateGroups }} 组）
        </button>
      </div>

      <template v-if="view === 'largest'">
        <p v-if="!report.largest.length" class="muted small">这个文件夹里没有文件。</p>
        <ul v-else class="list" aria-label="最大的文件">
          <li v-for="f in report.largest" :key="f.id" class="entry">
            <div class="entry-main">
              <p class="entry-title">
                <span class="name">{{ f.name }}</span>
                <span class="size">{{ formatBytes(f.size) }}</span>
                <TagPill v-if="f.protected" tone="manual">系统或程序的文件，别手动删</TagPill>
              </p>
              <p class="muted small path">{{ whereLine(f) }}</p>
            </div>
            <button type="button" class="btn btn-secondary btn-small" @click="reveal(f)">显示</button>
          </li>
        </ul>
      </template>

      <template v-else>
        <p class="small">{{ duplicateSummary(report, formatBytes) }}</p>
        <p v-if="report.duplicates.length" class="muted small">每组留一个就行：先确认留下的那个在你找得到的地方，再删掉其他的。</p>
        <div v-for="(g, i) in report.duplicates" :key="i" class="dup-group">
          <p class="small group-title">{{ g.count }} 个一样的文件，每个 {{ formatBytes(g.size) }}</p>
          <ul class="list">
            <li v-for="f in g.files" :key="f.id" class="entry">
              <div class="entry-main">
                <p class="entry-title"><span class="name">{{ f.name }}</span></p>
                <p class="muted small path">{{ whereLine(f) }}</p>
              </div>
              <button type="button" class="btn btn-secondary btn-small" @click="reveal(f)">显示</button>
            </li>
          </ul>
          <p v-if="g.count > g.files.length" class="muted small">还有 {{ g.count - g.files.length }} 个没列出来。</p>
        </div>
      </template>
    </template>
  </article>
</template>

<style scoped>
.space-card { display: flex; flex-direction: column; gap: 9px; }
.row, .switch { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
.summary { font-weight: 600; overflow-wrap: anywhere; }
.list { display: flex; flex-direction: column; gap: 6px; list-style: none; max-height: 360px; overflow: auto; }
.entry {
  display: flex; align-items: center; justify-content: space-between; gap: 12px;
  padding: 8px 12px; border-radius: var(--radius); background: var(--color-surface-2);
}
.entry > .btn { flex: none; }
.entry-main { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.entry-title { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
.name { font-weight: 600; overflow-wrap: anywhere; }
.size { font-variant-numeric: tabular-nums; }
.path { overflow-wrap: anywhere; }
.dup-group { display: flex; flex-direction: column; gap: 6px; }
.group-title { font-weight: 600; }
</style>
