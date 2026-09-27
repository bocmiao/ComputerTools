<script setup lang="ts">
import { computed, ref } from 'vue'
import { isTauri, lockersPickFiles, lockersPickFolder, lockersRefresh } from '../api'
import type { FileLockReport } from '../api/types'
import { errorText } from '../utils/format'
import { filesLine, KIND_LABELS, lockAdvice, lockNotes, lockSummary, NOTHING_FOUND_TIPS } from '../utils/fileLocks'
import BusySpinner from './BusySpinner.vue'
import ExplorerRestart from './ExplorerRestart.vue'
import TagPill from './TagPill.vue'

// 文件删不掉（「操作无法完成，因为文件已在另一程序中打开」）：查是哪些程序在用它，说清楚怎么让它放手。
// 只查不改：不关程序、不动文件。要查的文件只能在系统的选择框里选，界面拿不到完整路径。

const report = ref<FileLockReport | null>(null)
const busy = ref(false)
const error = ref('')

async function run(action: () => Promise<FileLockReport | null>): Promise<void> {
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

const notes = computed(() => (report.value ? lockNotes(report.value) : []))
const hasExplorer = computed(() => report.value?.users.some((u) => u.kind === 'explorer' && !u.otherSession) ?? false)
</script>

<template>
  <article class="card locker-card">
    <h3 class="section-title">文件删不掉：是谁占着</h3>
    <p class="muted small">
      删除、移动文件时提示「文件已在另一程序中打开」？选中它（或者整个文件夹），看看是哪个程序在用，再按说明关掉那个程序。这里只查不改：不关程序、不动文件。
    </p>
    <p v-if="!isTauri()" class="muted small">浏览器里显示的是演示结果；在 Windows 的小药箱里查真实的文件。</p>
    <div class="row">
      <button type="button" class="btn btn-secondary btn-small" :disabled="busy" @click="run(lockersPickFiles)">选择文件</button>
      <button type="button" class="btn btn-secondary btn-small" :disabled="busy" @click="run(lockersPickFolder)">选择文件夹</button>
      <button v-if="report" type="button" class="btn btn-secondary btn-small" :disabled="busy" @click="run(lockersRefresh)">关掉以后再查一次</button>
    </div>
    <p v-if="busy" class="loading-line small" role="status"><BusySpinner size="small" />正在查…</p>
    <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>

    <template v-if="report && !busy">
      <p class="small summary" role="status">{{ lockSummary(report) }}</p>
      <ul v-if="report.users.length" class="list">
        <li v-for="u in report.users" :key="u.pid" class="entry">
          <p class="entry-title">
            {{ u.name }}
            <TagPill tone="neutral">{{ KIND_LABELS[u.kind] }}</TagPill>
            <span v-if="u.program && u.program !== u.name" class="muted small">{{ u.program }}</span>
          </p>
          <p v-if="filesLine(report, u)" class="muted small path">{{ filesLine(report, u) }}</p>
          <p class="small">{{ lockAdvice(u) }}</p>
        </li>
      </ul>
      <ExplorerRestart v-if="hasExplorer" />
      <ul v-if="!report.users.length && report.checked > 0" class="tips small">
        <li v-for="tip in NOTHING_FOUND_TIPS" :key="tip">{{ tip }}</li>
      </ul>
      <p v-for="n in notes" :key="n" class="muted small">{{ n }}</p>
    </template>
  </article>
</template>

<style scoped>
.locker-card { display: flex; flex-direction: column; gap: 9px; }
.row { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
.summary { font-weight: 600; }
.list { display: flex; flex-direction: column; gap: 8px; list-style: none; }
.entry { display: flex; flex-direction: column; gap: 3px; padding: 9px 12px; border-radius: var(--radius); background: var(--color-surface-2); }
.entry-title { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; font-weight: 600; }
.entry-title .muted { font-weight: 400; }
.path { overflow-wrap: anywhere; }
.tips { display: flex; flex-direction: column; gap: 4px; padding-left: 1.2em; }
</style>
