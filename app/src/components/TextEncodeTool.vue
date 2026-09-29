<script setup lang="ts">
import { ref, watch } from 'vue'

// 文本编码转换：网址参数编码、解码，文字和 Base64 互转。只在这台电脑里处理，不上传。

// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件
defineProps<{ hideTitle?: boolean }>()

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
  <article class="card local-card">
    <h3 :class="hideTitle ? 'visually-hidden' : 'section-title'">文本编码转换</h3>
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
    <p v-if="copyNotice" class="muted small" role="status">{{ copyNotice }}</p>
  </article>
</template>

<style scoped>
.local-card { display: flex; flex-direction: column; gap: 9px; }
.field-label { font-weight: 600; font-size: var(--text-small); }
.input { width: 100%; min-width: 0; padding: 9px 11px; border: 1px solid var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface); }
.text-box { resize: vertical; line-height: 1.5; }
.self-start { align-self: flex-start; }
</style>
