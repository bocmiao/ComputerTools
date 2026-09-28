<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { algorithmOf, hashFile, type FileDigests, type HashAlgorithm } from '../utils/fileHash'
import { applyTextAction, textStats, type TextAction } from '../utils/textTools'
import ImagePrivacyTool from './ImagePrivacyTool.vue'
import BatchRenameTool from './BatchRenameTool.vue'
import BatchImageTool from './BatchImageTool.vue'
import ImagesToPdfTool from './ImagesToPdfTool.vue'
import LongImageTool from './LongImageTool.vue'
import FileLockers from './FileLockers.vue'
import PopupOwner from './PopupOwner.vue'
import SpaceFinder from './SpaceFinder.vue'
import HiddenFilesTool from './HiddenFilesTool.vue'
import TextQrTool from './TextQrTool.vue'
import AmountWordsTool from './AmountWordsTool.vue'
import DateCalcTool from './DateCalcTool.vue'
import TextDiffTool from './TextDiffTool.vue'

const selectedFile = ref<File | null>(null)
const digests = ref<FileDigests | null>(null)
const expectedHash = ref('')
const hashBusy = ref(false)
const hashProgress = ref(0)
const hashError = ref('')
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

function selectFile(event: Event): void {
  hashRun++
  selectedFile.value = (event.target as HTMLInputElement).files?.[0] ?? null
  digests.value = null
  hashError.value = ''
  hashProgress.value = 0
  hashBusy.value = false
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

const jsonInput = ref('')
const jsonOutput = ref('')
const jsonError = ref('')
watch(jsonInput, () => { jsonOutput.value = ''; jsonError.value = '' })

function convertJson(pretty: boolean): void {
  jsonOutput.value = ''
  jsonError.value = ''
  try {
    const value: unknown = JSON.parse(jsonInput.value)
    jsonOutput.value = JSON.stringify(value, null, pretty ? 2 : undefined)
  } catch (error) {
    jsonError.value = error instanceof Error ? error.message : 'JSON 格式不正确。'
  }
}

type TextMode = 'url-encode' | 'url-decode' | 'base64-encode' | 'base64-decode'
const textMode = ref<TextMode>('url-encode')
const textInput = ref('')
const textOutput = ref('')
const textError = ref('')
watch([textInput, textMode], () => { textOutput.value = ''; textError.value = '' })

function convertText(): void {
  textOutput.value = ''
  textError.value = ''
  try {
    if (textMode.value === 'url-encode') textOutput.value = encodeURIComponent(textInput.value)
    if (textMode.value === 'url-decode') textOutput.value = decodeURIComponent(textInput.value)
    if (textMode.value === 'base64-encode') {
      const bytes = new TextEncoder().encode(textInput.value)
      let binary = ''
      for (let i = 0; i < bytes.length; i += 8192) {
        binary += String.fromCharCode(...bytes.subarray(i, i + 8192))
      }
      textOutput.value = btoa(binary)
    }
    if (textMode.value === 'base64-decode') {
      const binary = atob(textInput.value.trim())
      const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0))
      textOutput.value = new TextDecoder('utf-8', { fatal: true }).decode(bytes)
    }
  } catch {
    textError.value = '转换失败：请检查输入内容与所选格式是否匹配。'
  }
}

// ── 文本整理 ──
const TEXT_ACTIONS: { id: TextAction; label: string }[] = [
  { id: 'to-half', label: '全角转半角' },
  { id: 'to-full', label: '半角转全角' },
  { id: 'drop-empty', label: '去掉空行' },
  { id: 'dedupe', label: '去掉重复的行' },
  { id: 'trim', label: '去掉每行首尾空格' },
  { id: 'sort', label: '按行排序' },
]
const tidyInput = ref('')
const tidyOutput = ref('')
watch(tidyInput, () => { tidyOutput.value = '' })
const tidyStats = computed(() => textStats(tidyInput.value))

function tidy(action: TextAction): void {
  tidyOutput.value = applyTextAction(action, tidyInput.value)
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
  <section class="group" aria-labelledby="local-tools-title">
    <div class="group-head">
      <h2 id="local-tools-title" class="section-title">文件与文本工具</h2>
      <p class="muted small">文件和文字只在这台电脑里处理，不上传。批量重命名会在确认后修改所选文件的名称；图片批量处理只新建文件，原图不动；「文件删不掉」「找大文件和重复文件」只查不改；「U 盘里的文件不见了」只去掉被藏起来的文件的隐藏属性，能改回去。</p>
    </div>
    <div class="local-list">
      <article class="card local-card">
        <h3 class="section-title">文件校验（MD5、SHA-1、SHA-256）</h3>
        <p class="muted small">选择下载的安装包或镜像，一次算出三种校验值；把官方公布的值粘进下面，自动认出是哪一种并比对。按小块读取，大文件也能处理。</p>
        <label class="field-label" for="hash-file">选择文件</label>
        <input id="hash-file" type="file" @change="selectFile" />
        <div class="local-actions">
          <button type="button" class="btn btn-secondary" :disabled="!selectedFile || hashBusy" @click="calculateHash">
            {{ hashBusy ? `计算中 ${hashProgress}%` : '计算校验值' }}
          </button>
          <button v-if="hashBusy" type="button" class="btn btn-ghost btn-small" @click="cancelHash">取消</button>
          <span v-if="selectedFile" class="muted small">{{ selectedFile.name }} · {{ fileSize(selectedFile.size) }}</span>
        </div>
        <p v-if="hashBusy" role="status" class="muted small">已读取 {{ hashProgress }}%</p>
        <p v-if="hashError" role="alert" class="danger-text small">{{ hashError }}</p>
        <template v-if="digests">
          <div v-for="algorithm in HASH_ORDER" :key="algorithm" class="hash-row">
            <label class="field-label" :for="`hash-${algorithm}`">{{ HASH_LABELS[algorithm] }}</label>
            <div class="result-row">
              <input :id="`hash-${algorithm}`" class="input mono" :value="digests[algorithm]" readonly />
              <button type="button" class="btn btn-secondary btn-small" @click="copy(digests[algorithm])">复制</button>
            </div>
          </div>
          <p class="muted small">MD5 和 SHA-1 只能用来核对文件有没有下载坏；官方给了 SHA-256 的，以 SHA-256 为准。</p>
        </template>
        <label class="field-label" for="hash-expected">官方校验值（可选）</label>
        <input id="hash-expected" v-model="expectedHash" class="input mono" type="text" maxlength="64" spellcheck="false" placeholder="粘贴官方公布的 MD5、SHA-1 或 SHA-256" />
        <p v-if="comparison" :class="comparison.startsWith('一致') ? 'success-text' : 'danger-text'" role="status">{{ comparison }}</p>
      </article>

      <article class="card local-card">
        <h3 class="section-title">文本整理</h3>
        <p class="muted small">把文字粘进来：全角半角互换（「ＡＢＣ１２３」变「ABC123」）、去掉空行和重复的行、按行排序（中文按拼音），顺便数数有多少字。</p>
        <label class="field-label" for="tidy-input">输入文字</label>
        <textarea id="tidy-input" v-model="tidyInput" class="input text-box" rows="5" spellcheck="false"></textarea>
        <p class="muted small" role="status">
          汉字 {{ tidyStats.chinese }} 个 · 英文单词 {{ tidyStats.words }} 个 · 字符 {{ tidyStats.chars }} 个（不算空格）、{{ tidyStats.charsWithSpaces }} 个（算空格） · {{ tidyStats.lines }} 行
        </p>
        <div class="local-actions">
          <button v-for="a in TEXT_ACTIONS" :key="a.id" type="button" class="btn btn-secondary btn-small" :disabled="!tidyInput" @click="tidy(a.id)">
            {{ a.label }}
          </button>
        </div>
        <template v-if="tidyOutput">
          <label class="field-label" for="tidy-output">结果</label>
          <textarea id="tidy-output" class="input text-box" :value="tidyOutput" rows="5" readonly spellcheck="false"></textarea>
          <div class="local-actions">
            <button type="button" class="btn btn-secondary btn-small" @click="copy(tidyOutput)">复制结果</button>
            <button type="button" class="btn btn-ghost btn-small" @click="tidyInput = tidyOutput">接着整理结果</button>
          </div>
        </template>
      </article>

      <article class="card local-card">
        <h3 class="section-title">JSON 格式化与校验</h3>
        <label class="field-label" for="json-input">输入 JSON</label>
        <textarea id="json-input" v-model="jsonInput" class="input mono text-box" rows="5" spellcheck="false" placeholder='例如 {"name":"电脑小药箱"}'></textarea>
        <div class="local-actions">
          <button type="button" class="btn btn-secondary btn-small" @click="convertJson(true)">格式化</button>
          <button type="button" class="btn btn-secondary btn-small" @click="convertJson(false)">压缩</button>
        </div>
        <p v-if="jsonError" class="danger-text small" role="alert">{{ jsonError }}</p>
        <template v-if="jsonOutput">
          <label class="field-label" for="json-output">结果</label>
          <textarea id="json-output" class="input mono text-box" :value="jsonOutput" rows="5" readonly spellcheck="false"></textarea>
          <button type="button" class="btn btn-secondary btn-small self-start" @click="copy(jsonOutput)">复制结果</button>
        </template>
      </article>

      <article class="card local-card">
        <h3 class="section-title">文本编码转换</h3>
        <label class="field-label" for="text-mode">转换方式</label>
        <select id="text-mode" v-model="textMode" class="input">
          <option value="url-encode">网址参数编码</option>
          <option value="url-decode">网址参数解码</option>
          <option value="base64-encode">文本转 Base64</option>
          <option value="base64-decode">Base64 转文本</option>
        </select>
        <label class="field-label" for="text-input">输入文字</label>
        <textarea id="text-input" v-model="textInput" class="input mono text-box" rows="4" spellcheck="false"></textarea>
        <button type="button" class="btn btn-secondary btn-small self-start" @click="convertText">转换</button>
        <p v-if="textError" class="danger-text small" role="alert">{{ textError }}</p>
        <template v-if="textOutput">
          <label class="field-label" for="text-output">结果</label>
          <textarea id="text-output" class="input mono text-box" :value="textOutput" rows="4" readonly spellcheck="false"></textarea>
          <button type="button" class="btn btn-secondary btn-small self-start" @click="copy(textOutput)">复制结果</button>
        </template>
      </article>
      <TextDiffTool />
      <TextQrTool />
      <AmountWordsTool />
      <DateCalcTool />
      <BatchImageTool />
      <ImagesToPdfTool />
      <LongImageTool />
      <ImagePrivacyTool />
      <BatchRenameTool />
      <FileLockers />
      <PopupOwner />
      <SpaceFinder />
      <HiddenFilesTool />
    </div>
    <p v-if="copyNotice" class="muted small" role="status">{{ copyNotice }}</p>
  </section>
</template>

<style scoped>
.group, .group-head, .local-card { display: flex; flex-direction: column; }
.group { gap: 10px; }
.group-head { gap: 2px; }
.local-list { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; align-items: start; }
.local-card { gap: 9px; }
.field-label { font-weight: 600; font-size: var(--text-small); }
.input { width: 100%; min-width: 0; padding: 9px 11px; border: 1px solid var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface); }
.text-box { resize: vertical; line-height: 1.5; }
.local-actions, .result-row { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
.result-row .input { flex: 1; }
.hash-row { display: flex; flex-direction: column; gap: 4px; }
.self-start { align-self: flex-start; }
@media (max-width: 900px) { .local-list { grid-template-columns: 1fr; } }
</style>
