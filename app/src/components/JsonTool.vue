<script setup lang="ts">
import { ref, watch } from 'vue'

// JSON 格式化与校验：排整齐或者压成一行，格式不对时显示错在哪里。只在这台电脑里处理，不上传。

// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件
defineProps<{ hideTitle?: boolean }>()

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
    <h3 :class="hideTitle ? 'visually-hidden' : 'section-title'">JSON 格式化与校验</h3>
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
    <p v-if="copyNotice" class="muted small" role="status">{{ copyNotice }}</p>
  </article>
</template>

<style scoped>
.local-card { display: flex; flex-direction: column; gap: 9px; }
.field-label { font-weight: 600; font-size: var(--text-small); }
.input { width: 100%; min-width: 0; padding: 9px 11px; border: 1px solid var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface); }
.text-box { resize: vertical; line-height: 1.5; }
.local-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
.self-start { align-self: flex-start; }
</style>
