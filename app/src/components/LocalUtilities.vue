<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { sha256File } from '../utils/fileHash'
import ImagePrivacyTool from './ImagePrivacyTool.vue'

const selectedFile = ref<File | null>(null)
const fileHash = ref('')
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

const comparison = computed(() => {
  const expected = expectedHash.value.trim().toLowerCase()
  if (!expected || !fileHash.value) return ''
  if (!/^[0-9a-f]{64}$/.test(expected)) return '校验值应是 64 位 SHA-256 十六进制字符。'
  return expected === fileHash.value ? '一致：文件与提供的校验值相符。' : '不一致：请核对来源，不要运行这个文件。'
})

function selectFile(event: Event): void {
  hashRun++
  selectedFile.value = (event.target as HTMLInputElement).files?.[0] ?? null
  fileHash.value = ''
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
  fileHash.value = ''
  hashProgress.value = 0
  try {
    const result = await sha256File(file, (percent) => { hashProgress.value = percent }, () => run !== hashRun)
    if (run !== hashRun || result === null) return
    fileHash.value = result
  } catch (error) {
    if (run === hashRun) hashError.value = error instanceof Error ? error.message : '读取文件失败。'
  } finally {
    if (run === hashRun) hashBusy.value = false
  }
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
      <p class="muted small">文件和文字只在这台电脑里处理，不上传；不会修改原文件。</p>
    </div>
    <div class="local-list">
      <article class="card local-card">
        <h3 class="section-title">文件 SHA-256 校验</h3>
        <p class="muted small">选择下载的安装包或镜像，算出校验值；有官方提供的 SHA-256 时可直接比对。按小块读取，大文件也能处理。</p>
        <label class="field-label" for="hash-file">选择文件</label>
        <input id="hash-file" type="file" @change="selectFile" />
        <div class="local-actions">
          <button type="button" class="btn btn-secondary" :disabled="!selectedFile || hashBusy" @click="calculateHash">
            {{ hashBusy ? `计算中 ${hashProgress}%` : '计算 SHA-256' }}
          </button>
          <span v-if="selectedFile" class="muted small">{{ selectedFile.name }} · {{ fileSize(selectedFile.size) }}</span>
        </div>
        <p v-if="hashBusy" role="status" class="muted small">已读取 {{ hashProgress }}%</p>
        <p v-if="hashError" role="alert" class="danger-text small">{{ hashError }}</p>
        <template v-if="fileHash">
          <label class="field-label" for="hash-result">计算结果</label>
          <div class="result-row">
            <input id="hash-result" class="input mono" :value="fileHash" readonly />
            <button type="button" class="btn btn-secondary btn-small" @click="copy(fileHash)">复制</button>
          </div>
        </template>
        <label class="field-label" for="hash-expected">官方校验值（可选）</label>
        <input id="hash-expected" v-model="expectedHash" class="input mono" type="text" maxlength="64" spellcheck="false" placeholder="粘贴官方公布的 SHA-256" />
        <p v-if="comparison" :class="comparison.startsWith('一致') ? 'success-text' : 'danger-text'" role="status">{{ comparison }}</p>
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
      <ImagePrivacyTool />
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
.self-start { align-self: flex-start; }
@media (max-width: 900px) { .local-list { grid-template-columns: 1fr; } }
</style>
