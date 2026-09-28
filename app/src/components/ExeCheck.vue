<script setup lang="ts">
import { computed, ref } from 'vue'
import { exeCheckPick, isTauri } from '../api'
import type { ExeCheckView } from '../api/types'
import { exeAdvice } from '../utils/exeCheck'
import { errorText } from '../utils/format'
import { formatBytes } from '../utils/imageBatch'
import BusySpinner from './BusySpinner.vue'

// 「此应用无法在你的电脑上运行」：用系统的选择框选那个程序文件，后端只读文件开头，看它是给哪种电脑的、有没有下载完整
// （medkit_core::exe_info），不运行它。结果里只有文件名。

const result = ref<ExeCheckView | null>(null)
const busy = ref(false)
const error = ref('')

async function pick(): Promise<void> {
  if (busy.value) return
  busy.value = true
  error.value = ''
  try {
    const r = await exeCheckPick()
    if (r) result.value = r
  } catch (e) {
    error.value = errorText(e)
  } finally {
    busy.value = false
  }
}

const advice = computed(() => (result.value ? exeAdvice(result.value) : null))
</script>

<template>
  <section class="card exe-card" aria-labelledby="exe-check-title">
    <h2 id="exe-check-title" class="section-title">看看这个程序能不能在这台电脑上运行</h2>
    <p class="muted small">
      选一下双击时提示「此应用无法在你的电脑上运行」的那个程序文件（.exe）。小药箱只读文件开头，看它是给哪种电脑的、有没有下载完整，不会运行它。
    </p>
    <p v-if="!isTauri()" class="muted small">浏览器里显示的是演示结果；在 Windows 的小药箱里看真实的文件。</p>
    <div>
      <button type="button" class="btn btn-secondary btn-small" :disabled="busy" @click="pick">
        {{ result ? '换一个文件' : '选择程序文件' }}
      </button>
    </div>
    <p v-if="busy" class="loading-line small" role="status"><BusySpinner size="small" />正在看…</p>
    <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>

    <template v-if="result && advice && !busy">
      <p class="small muted">{{ result.name }}（{{ formatBytes(result.size) }}）</p>
      <p class="small verdict" :class="advice.ok ? 'success-text' : 'danger-text'" role="status">{{ advice.title }}</p>
      <ul class="tips small">
        <li v-for="t in advice.tips" :key="t">{{ t }}</li>
      </ul>
    </template>
  </section>
</template>

<style scoped>
.exe-card { display: flex; flex-direction: column; gap: 9px; }
.verdict { font-weight: 600; overflow-wrap: anywhere; }
.tips { display: flex; flex-direction: column; gap: 4px; padding-left: 1.2em; }
</style>
