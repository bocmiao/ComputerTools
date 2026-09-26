<script setup lang="ts">
import { computed, nextTick, ref, useTemplateRef } from 'vue'
import { reportGenerate } from '../api'
import AppIcon from '../components/AppIcon.vue'
import BusySpinner from '../components/BusySpinner.vue'
import { health, requestHealthCheck } from '../state'
import { errorText, formatDateTime } from '../utils/format'

// 诊断报告：已脱敏的纯文本，复制后发给懂哥。小药箱不会自动上传任何东西。
// 报告里的体检部分来自最近一次体检，这里写清楚是什么时候的，并给出重新体检的入口。

const report = ref<string | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)
const copyState = ref<'idle' | 'ok' | 'fail'>('idle')
const textarea = useTemplateRef<HTMLTextAreaElement>('textarea')
/** 生成报告那一刻，最近一次体检是什么时候 */
const healthRunAtReport = ref<string | null>(null)

/** 生成报告以后又体检过：报告里的体检结果已经不是最新的 */
const reportOutdated = computed(() => report.value !== null && health.lastRunAt !== healthRunAtReport.value)

async function generate(): Promise<void> {
  loading.value = true
  error.value = null
  copyState.value = 'idle'
  const runAt = health.lastRunAt
  try {
    report.value = await reportGenerate()
    healthRunAtReport.value = runAt
  } catch (e) {
    error.value = errorText(e)
  } finally {
    loading.value = false
  }
}

async function copy(): Promise<void> {
  if (report.value === null) return
  try {
    await navigator.clipboard.writeText(report.value)
    copyState.value = 'ok'
  } catch {
    copyState.value = 'fail'
    // 自动复制不行时，帮用户把文字选中，方便按 Ctrl+C
    await nextTick()
    textarea.value?.focus()
    textarea.value?.select()
  }
}
</script>

<template>
  <div class="page">
    <header class="page-header">
      <h1 class="page-title" tabindex="-1">诊断报告</h1>
      <p class="page-lead">
        报告已去掉用户名、电脑名、IP、序列号等隐私信息，可以直接发给懂哥；小药箱不会自动上传任何东西。
      </p>
    </header>

    <div class="health-source card">
      <div class="health-source-text">
        <p>
          报告里的体检结果来自最近一次体检<template v-if="health.lastRunAt">（{{ formatDateTime(health.lastRunAt) }}）</template>。
        </p>
        <p v-if="!health.lastRunAt" class="muted small">这次打开小药箱以后还没有体检过，报告里不会有体检结果。</p>
        <p v-else-if="health.stale" class="muted small">体检以后你又改过或撤销过设置，这些结果可能已经过期，建议先重新体检。</p>
      </div>
      <button type="button" class="btn btn-secondary btn-small" @click="requestHealthCheck">
        {{ health.lastRunAt ? '重新体检' : '先去体检' }}
      </button>
    </div>

    <div class="actions">
      <button type="button" class="btn btn-primary" :disabled="loading" @click="generate">
        <BusySpinner v-if="loading" size="small" />
        <AppIcon v-else name="report" :size="18" />
        {{ loading ? '正在生成…' : report === null ? '生成报告' : '重新生成' }}
      </button>
      <button v-if="report !== null" type="button" class="btn btn-secondary" :disabled="loading" @click="copy">
        <AppIcon name="copy" :size="18" />复制
      </button>
      <p v-if="copyState === 'ok'" class="success-text" role="status">已经复制好了，可以粘贴到微信里发给懂哥。</p>
      <p v-else-if="copyState === 'fail'" class="danger-text" role="alert">
        没能自动复制。文字已经选中了，请按 Ctrl+C 复制；或者点一下文本框，按 Ctrl+A 全选后再按 Ctrl+C。
      </p>
    </div>

    <div v-if="error" class="banner banner-error" role="alert">
      <div>
        <p class="banner-title">报告没能生成</p>
        <p>{{ error }}</p>
      </div>
    </div>

    <template v-if="report !== null">
      <p v-if="reportOutdated" class="muted" role="status">生成这份报告以后又体检过了，点「重新生成」拿到最新的报告。</p>
      <label for="report-text" class="visually-hidden">诊断报告内容</label>
      <textarea
        id="report-text"
        ref="textarea"
        class="report input"
        :value="report"
        readonly
        rows="20"
        spellcheck="false"
      ></textarea>
      <p class="muted small">发出去之前可以先看一遍。报告只在这里显示，不会存到别处，也不会上传。</p>
    </template>
  </div>
</template>

<style scoped>
.actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px 12px;
}

.health-source {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 10px 16px;
}

.health-source-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.report {
  min-height: 360px;
  padding: 14px 16px;
  font-family: var(--font-mono);
  font-size: 14px;
  line-height: 1.7;
  resize: vertical;
}
</style>
