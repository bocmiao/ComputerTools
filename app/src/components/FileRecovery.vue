<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { diskSpeedDrives, isTauri, toolRun } from '../api'
import type { DriveView, ToolResult } from '../api/types'
import {
  FILE_KINDS,
  KIND_LABELS,
  defaultTarget,
  isNtfs,
  planRecovery,
  type FileKind,
} from '../utils/fileRecovery'
import { errorText } from '../utils/format'
import { formatBytes } from '../utils/imageBatch'
import BusySpinner from './BusySpinner.vue'
import ToolOutcome from './ToolOutcome.vue'

// 「误删了文件」：照用户选的拼出微软 Windows File Recovery（winfr）的命令（utils/fileRecovery.ts），再用小工具
// system.file-recovery 打开它（没装的打开商店页面）。小药箱不替用户运行 winfr：命令在用户眼前粘贴、按 y 确认。
// 列盘用「硬盘测速」的那个命令。

const drives = ref<DriveView[]>([])
const loading = ref(false)
const error = ref('')
const source = ref('')
const target = ref('')
const kinds = ref<FileKind[]>(['photos', 'documents'])
const name = ref('')
const thorough = ref(false)
const copied = ref(false)

const opening = ref(false)
const openResult = ref<ToolResult | null>(null)
const openError = ref('')

function driveText(d: DriveView): string {
  const label = d.label ? `（${d.label}）` : d.system ? '（系统盘）' : d.removable ? '（U 盘、存储卡）' : ''
  return `${d.letter} 盘${label} · ${d.fileSystem || '未知格式'} · 剩 ${formatBytes(d.free)}`
}

async function load(): Promise<void> {
  loading.value = true
  error.value = ''
  try {
    drives.value = await diskSpeedDrives()
    if (!drives.value.some((d) => d.letter === source.value)) {
      source.value = (drives.value.find((d) => d.system) ?? drives.value[0])?.letter ?? ''
    }
    if (!drives.value.some((d) => d.letter === target.value) || target.value === source.value) {
      target.value = defaultTarget(drives.value, source.value)
    }
  } catch (e) {
    error.value = errorText(e)
  } finally {
    loading.value = false
  }
}

onMounted(load)

watch(source, (s) => {
  if (target.value === s || !target.value) target.value = defaultTarget(drives.value, s)
})

const sourceNtfs = computed(() => {
  const d = drives.value.find((x) => x.letter === source.value)
  return d ? isNtfs(d.fileSystem) : true
})

const plan = computed(() =>
  planRecovery(drives.value, {
    source: source.value,
    target: target.value,
    kinds: kinds.value,
    name: name.value,
    thorough: thorough.value,
  }),
)

watch(() => plan.value.command, () => (copied.value = false))

async function copy(): Promise<void> {
  try {
    await navigator.clipboard.writeText(plan.value.command)
    copied.value = true
  } catch (e) {
    error.value = `复制不了：${errorText(e)}。可以用鼠标选中命令，按 Ctrl + C 复制。`
  }
}

async function openApp(): Promise<void> {
  if (opening.value) return
  opening.value = true
  openError.value = ''
  openResult.value = null
  try {
    openResult.value = await toolRun('system.file-recovery')
  } catch (e) {
    openError.value = errorText(e)
  } finally {
    opening.value = false
  }
}
</script>

<template>
  <section class="card recovery-card" aria-labelledby="file-recovery-title">
    <div class="head">
      <h2 id="file-recovery-title" class="section-title">拼出恢复命令（用微软的 Windows File Recovery 找）</h2>
      <button type="button" class="btn btn-secondary btn-small" :disabled="loading" @click="load">刷新</button>
    </div>
    <p class="muted small">
      回收站、「以前的版本」里都找不到的，用微软免费的「Windows File Recovery」找。它要在命令行里输入命令，选好下面几项，小药箱帮你把命令拼好，复制过去就行。小药箱自己不去找、不动你的文件。
    </p>
    <p v-if="!isTauri()" class="muted small">浏览器里是演示用的盘；在 Windows 的小药箱里是这台电脑上真实的盘。</p>
    <p v-if="loading" class="loading-line small" role="status"><BusySpinner size="small" />正在列出这台电脑上的盘…</p>
    <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>

    <template v-if="drives.length && !loading">
      <label class="field small">
        <span>文件原来在哪个盘</span>
        <select v-model="source">
          <option v-for="d in drives" :key="d.letter" :value="d.letter">{{ driveText(d) }}</option>
        </select>
      </label>

      <fieldset class="field small">
        <legend>要找哪些文件</legend>
        <label v-for="k in FILE_KINDS" :key="k" class="check">
          <input v-model="kinds" type="checkbox" :value="k" />{{ KIND_LABELS[k] }}
        </label>
      </fieldset>

      <label class="field small">
        <span>文件名里有的字（记得的话填上，找得更准；可以不填）</span>
        <input v-model="name" type="text" maxlength="60" placeholder="例如：合同、年终总结" />
      </label>

      <label class="field small">
        <span>找回来的文件存到哪个盘（不能是原来的盘，最好是 U 盘或者移动硬盘）</span>
        <select v-model="target">
          <option v-for="d in drives.filter((x) => x.letter !== source)" :key="d.letter" :value="d.letter">
            {{ driveText(d) }}
          </option>
        </select>
      </label>

      <label v-if="sourceNtfs" class="check small">
        <input v-model="thorough" type="checkbox" />仔细找（慢很多；删了很久、格式化过、快速找没找到的时候选）
      </label>

      <p v-if="plan.problem" class="small notice" role="status">{{ plan.problem }}</p>
      <template v-else>
        <div class="command-row">
          <code class="command">{{ plan.command }}</code>
          <button type="button" class="btn btn-primary btn-small" @click="copy">{{ copied ? '已复制' : '复制命令' }}</button>
        </div>
        <ul v-if="plan.notes.length" class="notes small">
          <li v-for="n in plan.notes" :key="n">{{ n }}</li>
        </ul>
        <ol class="steps small">
          <li>点下面的「打开 Windows File Recovery」；还没装的会打开 Microsoft Store 里它的页面，点「获取」装好以后再点一次。</li>
          <li>弹出「你要允许此应用对你的设备进行更改吗？」时点「是」。在黑色的窗口里按 Ctrl + V（或者点右键）把命令粘贴进去，按回车。</li>
          <li>问「Continue? (y/n)」时按 y 再回车，等它找完：盘越大越久，别关窗口，也别往原来的盘里存东西。</li>
          <li>找到的文件在 {{ target }} 盘新建的 Recovery_日期_时间 文件夹里；最后问「View recovered files?」时按 y 就会打开这个文件夹。</li>
        </ol>
        <div>
          <button type="button" class="btn btn-secondary btn-small" :disabled="opening" @click="openApp">
            <BusySpinner v-if="opening" size="small" />打开 Windows File Recovery
          </button>
        </div>
        <p v-if="openError" class="danger-text small" role="alert">{{ openError }}</p>
        <ToolOutcome v-if="openResult" :result="openResult" />
      </template>
    </template>
    <p v-else-if="!loading && !error" class="muted small">这台电脑上没列出能用的盘。插上 U 盘以后点「刷新」。</p>
  </section>
</template>

<style scoped>
.recovery-card { display: flex; flex-direction: column; gap: 10px; }
.head { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.field { display: flex; flex-direction: column; gap: 4px; border: 0; padding: 0; margin: 0; }
.field legend { padding: 0; margin-bottom: 4px; }
.field select, .field input[type='text'] { max-width: 100%; }
.check { display: inline-flex; align-items: center; gap: 6px; margin-right: 14px; }
.command-row { display: flex; align-items: flex-start; gap: 10px; flex-wrap: wrap; }
.command {
  flex: 1 1 280px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--color-border);
  background: var(--color-surface-2);
  font-family: var(--font-mono);
  overflow-wrap: anywhere;
  user-select: all;
}
.notes, .steps { display: flex; flex-direction: column; gap: 4px; padding-left: 1.3em; }
.notice { font-weight: 600; }
</style>
