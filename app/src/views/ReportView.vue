<script setup lang="ts">
import { nextTick, ref, useTemplateRef } from 'vue'
import { reportGenerate } from '../api'
import AppIcon from '../components/AppIcon.vue'
import BusySpinner from '../components/BusySpinner.vue'
import { errorText } from '../utils/format'

// 诊断报告：已脱敏的纯文本，复制后发给懂哥。小药箱不会自动上传任何东西。

const report = ref<string | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)
const copyState = ref<'idle' | 'ok' | 'fail'>('idle')
const textarea = useTemplateRef<HTMLTextAreaElement>('textarea')

async function generate(): Promise<void> {
  loading.value = true
  error.value = null
  copyState.value = 'idle'
  try {
    report.value = await reportGenerate()
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
      <h1 class="page-title">诊断报告</h1>
      <p class="page-lead">
        报告已去掉用户名、电脑名、IP、序列号等隐私信息，可以直接发给懂哥；小药箱不会自动上传任何东西。
      </p>
    </header>

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

.report {
  min-height: 360px;
  padding: 14px 16px;
  font-family: var(--font-mono);
  font-size: 14px;
  line-height: 1.7;
  resize: vertical;
}
</style>
