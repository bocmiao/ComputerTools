<script setup lang="ts">
import { computed, ref } from 'vue'
import { amountInWords, dateInWords } from '../utils/rmb'

// 金额、日期转中文大写：写支票、发票、收据、借条时用，规则见 utils/rmb.ts（照人民银行的规定）。

const pad = (n: number): string => String(n).padStart(2, '0')
const today = new Date()

const amount = ref('')
const result = computed(() => amountInWords(amount.value))
const date = ref(`${today.getFullYear()}-${pad(today.getMonth() + 1)}-${pad(today.getDate())}`)
const dateWords = computed(() => {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date.value)
  return m ? dateInWords(Number(m[1]), Number(m[2]), Number(m[3])) : ''
})
const notice = ref('')

async function copy(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text)
    notice.value = `已复制：${text}`
  } catch {
    notice.value = '自动复制失败，可以选中上面的字按 Ctrl+C。'
  }
}
</script>

<template>
  <article class="card words-card">
    <h3 class="section-title">金额、日期转大写</h3>
    <p class="muted small">
      写支票、发票、收据、借条时用：金额照人民银行的规定写成「壹万贰仟叁佰肆拾伍元陆角柒分」这样的大写，支票上的日期也要大写。
    </p>
    <label class="field-label" for="words-amount">金额（元）</label>
    <input
      id="words-amount"
      v-model="amount"
      class="input"
      type="text"
      inputmode="decimal"
      maxlength="30"
      autocomplete="off"
      spellcheck="false"
      placeholder="例如 12345.67"
    />
    <template v-if="result.ok">
      <p class="words" aria-live="polite">人民币{{ result.words }}</p>
      <p class="muted small">小写写成 {{ result.figures }}。票据上已经印了「人民币」的，紧接着写后面的字，中间不要留空。</p>
      <button type="button" class="btn btn-secondary btn-small self-start" @click="copy(`人民币${result.words}`)">复制大写金额</button>
    </template>
    <p v-else-if="result.error" class="danger-text small" role="alert">{{ result.error }}</p>

    <label class="field-label" for="words-date">日期</label>
    <input id="words-date" v-model="date" class="input date-input" type="date" />
    <template v-if="dateWords">
      <p class="words" aria-live="polite">{{ dateWords }}</p>
      <p class="muted small">支票的出票日期必须大写：1、2、10 月和 1 到 10 日、20 日、30 日前面加「零」，11 到 19 日前面加「壹」，免得被人改。</p>
      <button type="button" class="btn btn-secondary btn-small self-start" @click="copy(dateWords)">复制大写日期</button>
    </template>
    <p v-if="notice" class="muted small" role="status">{{ notice }}</p>
  </article>
</template>

<style scoped>
.words-card { display: flex; flex-direction: column; gap: 9px; }
.field-label { font-weight: 600; font-size: var(--text-small); margin-top: 4px; }
.input { width: 100%; min-width: 0; padding: 9px 11px; border: 1px solid var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface); color: inherit; }
.date-input { max-width: 14em; }
.words { font-size: 1.15rem; font-weight: 600; letter-spacing: 0.04em; overflow-wrap: anywhere; }
.self-start { align-self: flex-start; }
</style>
