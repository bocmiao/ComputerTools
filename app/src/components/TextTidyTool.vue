<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { applyTextAction, textStats, type TextAction } from '../utils/textTools'

// 文本整理：全角半角互换、去掉空行和重复的行、按行排序，顺便数数有多少字。只在这台电脑里处理，不上传。

// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件
defineProps<{ hideTitle?: boolean }>()

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
  <article class="card local-card">
    <h3 :class="hideTitle ? 'visually-hidden' : 'section-title'">文本整理</h3>
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
    <p v-if="copyNotice" class="muted small" role="status">{{ copyNotice }}</p>
  </article>
</template>

<style scoped>
.local-card { display: flex; flex-direction: column; gap: 9px; }
.field-label { font-weight: 600; font-size: var(--text-small); }
.input { width: 100%; min-width: 0; padding: 9px 11px; border: 1px solid var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface); }
.text-box { resize: vertical; line-height: 1.5; }
.local-actions { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
</style>
