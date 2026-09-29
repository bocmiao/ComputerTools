<script setup lang="ts">
import { computed, ref, useTemplateRef } from 'vue'
import { algorithmOf, hashFile, type FileDigests, type HashAlgorithm } from '../utils/fileHash'
import AppIcon from './AppIcon.vue'

// 文件校验：一次算出 MD5、SHA-1、SHA-256，和官方公布的值比一比。按小块读取，大文件也能处理；只在这台电脑里算，不上传。
// 左边选文件（也能直接拖进来）、填官方的值，右边看结果；窗口窄了变成上下两块。

// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件
defineProps<{ hideTitle?: boolean }>()

const selectedFile = ref<File | null>(null)
const digests = ref<FileDigests | null>(null)
const expectedHash = ref('')
const hashBusy = ref(false)
const hashProgress = ref(0)
const hashError = ref('')
const dragDepth = ref(0)
const picker = useTemplateRef<HTMLInputElement>('picker')
let hashRun = 0

function fileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MB`
  return `${(bytes / 1073741824).toFixed(2)} GB`
}

const HASH_LABELS: Record<HashAlgorithm, string> = { md5: 'MD5', sha1: 'SHA-1', sha256: 'SHA-256' }
const HASH_ORDER: HashAlgorithm[] = ['sha256', 'sha1', 'md5']

// 官方给的是哪一种，按长度认（32 位 MD5、40 位 SHA-1、64 位 SHA-256），和算出来的同一种比
const comparison = computed(() => {
  const expected = expectedHash.value.trim().toLowerCase()
  if (!expected || !digests.value) return ''
  const algorithm = algorithmOf(expected)
  if (!algorithm) return '校验值应是 32 位（MD5）、40 位（SHA-1）或 64 位（SHA-256）的十六进制字符。'
  const name = HASH_LABELS[algorithm]
  return expected === digests.value[algorithm]
    ? `一致：文件和提供的 ${name} 相符。`
    : `不一致：${name} 对不上，请核对来源，不要运行这个文件。`
})

/** 比对的结论放在右边的结果里；粘贴的值格式不对的提示放在输入框下面 */
const comparisonKind = computed(() => {
  if (!comparison.value) return null
  if (comparison.value.startsWith('一致')) return 'match'
  return comparison.value.startsWith('不一致') ? 'mismatch' : 'invalid'
})

/** 和官方的值对上的是哪一种：那一行标「一致」 */
const matched = computed<HashAlgorithm | null>(() => {
  const expected = expectedHash.value.trim().toLowerCase()
  const algorithm = expected && digests.value ? algorithmOf(expected) : null
  return algorithm && digests.value?.[algorithm] === expected ? algorithm : null
})

/** 换了文件：正在算的作废，之前的结果清掉 */
function useFile(file: File | null): void {
  hashRun++
  selectedFile.value = file
  digests.value = null
  hashError.value = ''
  hashProgress.value = 0
  hashBusy.value = false
}

function selectFile(event: Event): void {
  const input = event.target as HTMLInputElement
  useFile(input.files?.[0] ?? null)
  // 清空，下次选同一个文件也算选了
  input.value = ''
}

function onDrop(event: DragEvent): void {
  dragDepth.value = 0
  const file = event.dataTransfer?.files?.[0]
  if (file) useFile(file)
}

async function calculateHash(): Promise<void> {
  const file = selectedFile.value
  if (!file || hashBusy.value) return
  const run = ++hashRun
  hashBusy.value = true
  hashError.value = ''
  digests.value = null
  hashProgress.value = 0
  try {
    const result = await hashFile(file, (percent) => { hashProgress.value = percent }, () => run !== hashRun)
    if (run !== hashRun || result === null) return
    digests.value = result
  } catch (error) {
    if (run === hashRun) hashError.value = error instanceof Error ? error.message : '读取文件失败。'
  } finally {
    if (run === hashRun) hashBusy.value = false
  }
}

function cancelHash(): void {
  hashRun++
  hashBusy.value = false
  hashProgress.value = 0
  hashError.value = '已取消计算。'
}

const copyNotice = ref('')
async function copy(value: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(value)
    copyNotice.value = '已复制。'
  } catch {
    copyNotice.value = '自动复制失败。可以在结果框中按 Ctrl+A、Ctrl+C。'
  }
}
</script>

<template>
  <article class="hash-tool" aria-labelledby="hash-title">
    <h3 id="hash-title" :class="hideTitle ? 'visually-hidden' : 'section-title'">文件校验（MD5、SHA-1、SHA-256）</h3>
    <div class="hash-panels">
      <section class="card panel" aria-labelledby="hash-input-title">
        <h4 id="hash-input-title" class="panel-title">要校验的文件</h4>
        <p class="muted small">选下载的安装包或镜像，一次算出三种校验值。按小块读取，大文件也能处理。</p>
        <div
          class="drop"
          :class="{ over: dragDepth > 0 }"
          @dragenter.prevent="dragDepth++"
          @dragover.prevent
          @dragleave="dragDepth = Math.max(0, dragDepth - 1)"
          @drop.prevent="onDrop"
        >
          <span class="muted small">把文件拖到这里，或者</span>
          <button type="button" class="btn btn-secondary btn-small" @click="picker?.click()">选择文件</button>
          <input ref="picker" class="visually-hidden" type="file" tabindex="-1" aria-hidden="true" @change="selectFile" />
        </div>
        <div v-if="selectedFile" class="file-row">
          <AppIcon name="report" :size="20" class="file-icon" />
          <span class="file-text">
            <span class="file-name">{{ selectedFile.name }}</span>
            <span class="muted small">{{ fileSize(selectedFile.size) }}</span>
          </span>
        </div>

        <label class="field-label" for="hash-expected">官方公布的校验值<span class="optional">（选填）</span></label>
        <input
          id="hash-expected"
          v-model="expectedHash"
          class="input mono"
          type="text"
          maxlength="64"
          spellcheck="false"
          placeholder="粘贴官方公布的 MD5、SHA-1 或 SHA-256"
          aria-describedby="hash-expected-hint"
        />
        <p id="hash-expected-hint" class="muted small">粘贴进来就行，会自动认出是 MD5、SHA-1 还是 SHA-256。</p>
        <p v-if="comparisonKind === 'invalid'" class="danger-text small" role="status">{{ comparison }}</p>

        <div class="local-actions">
          <button type="button" class="btn btn-primary" :disabled="!selectedFile || hashBusy" @click="calculateHash">
            {{ hashBusy ? `计算中 ${hashProgress}%` : digests ? '重新计算' : '计算校验值' }}
          </button>
          <button v-if="hashBusy" type="button" class="btn btn-ghost btn-small" @click="cancelHash">取消</button>
        </div>
        <p v-if="hashBusy" role="status" class="muted small">已读取 {{ hashProgress }}%</p>
        <p v-if="hashError" role="alert" class="danger-text small">{{ hashError }}</p>
      </section>

      <section class="card panel" aria-labelledby="hash-result-title">
        <h4 id="hash-result-title" class="panel-title">结果</h4>
        <template v-if="digests">
          <div
            v-if="comparisonKind === 'match' || comparisonKind === 'mismatch'"
            class="banner"
            :class="comparisonKind === 'match' ? 'banner-ok' : 'banner-error'"
            role="status"
          >
            <AppIcon :name="comparisonKind === 'match' ? 'check' : 'warning'" :size="20" class="banner-icon" />
            <p class="banner-title">{{ comparison }}</p>
          </div>
          <div v-for="algorithm in HASH_ORDER" :key="algorithm" class="hash-row">
            <label class="field-label" :for="`hash-${algorithm}`">
              {{ HASH_LABELS[algorithm] }}<span v-if="matched === algorithm" class="match-tag">一致</span>
            </label>
            <div class="result-row">
              <input :id="`hash-${algorithm}`" class="input mono" :value="digests[algorithm]" readonly />
              <button type="button" class="btn btn-secondary btn-small" @click="copy(digests[algorithm])">复制</button>
            </div>
          </div>
          <p class="muted small">MD5 和 SHA-1 只能用来核对文件有没有下载坏；官方给了 SHA-256 的，以 SHA-256 为准。</p>
          <p v-if="copyNotice" class="muted small" role="status">{{ copyNotice }}</p>
        </template>
        <p v-else class="muted small">选好文件，点「计算校验值」，三种校验值会显示在这里。</p>
      </section>
    </div>
  </article>
</template>

<style scoped>
.hash-tool { display: flex; flex-direction: column; gap: 10px; }
/* 左边窄一点放输入，右边宽一点放结果，两边一样高；窗口窄了上下排 */
.hash-panels { display: grid; grid-template-columns: minmax(0, 5fr) minmax(0, 7fr); gap: 20px; align-items: stretch; }
.panel { display: flex; flex-direction: column; gap: 9px; }
.panel-title { font-size: var(--text-large); }
.field-label { font-weight: 600; font-size: var(--text-small); }
.optional { color: var(--color-text-muted); font-weight: 400; }
.input { width: 100%; min-width: 0; padding: 9px 11px; border: 1px solid var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface); }
.drop {
  display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px;
  min-height: 130px; padding: 16px; border: 2px dashed var(--color-border-strong); border-radius: var(--radius);
  background: var(--color-surface-2); text-align: center;
}
.drop.over { border-color: var(--color-primary); background: var(--color-primary-soft); }
.file-row {
  display: flex; align-items: center; gap: 10px; padding: 10px 12px;
  border: 1px solid var(--color-border); border-radius: var(--radius); background: var(--color-surface-2);
}
.file-icon { color: var(--color-primary); }
.file-text { display: flex; flex-direction: column; min-width: 0; }
.file-name { font-weight: 600; overflow-wrap: anywhere; }
.local-actions, .result-row { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
.result-row .input { flex: 1; }
.hash-row { display: flex; flex-direction: column; gap: 4px; }
.match-tag {
  margin-left: 8px; padding: 1px 8px; border-radius: 999px;
  background: var(--tone-ok-bg); color: var(--tone-ok-text); font-size: 12px; font-weight: 600;
}
.banner { align-items: center; }
.banner-icon { flex: none; }
@media (max-width: 1100px) { .hash-panels { grid-template-columns: minmax(0, 1fr); } }
</style>
