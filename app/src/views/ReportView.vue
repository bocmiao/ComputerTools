<script setup lang="ts">
import { computed, nextTick, ref, useTemplateRef } from 'vue'
import { reportGenerate } from '../api'
import AppIcon from '../components/AppIcon.vue'
import BusySpinner from '../components/BusySpinner.vue'
import { health, requestHealthCheck } from '../state'
import { errorText, formatDateTime } from '../utils/format'

// 诊断报告：已脱敏的纯文本，复制后发给懂哥，或者自己粘给 AI 问。小药箱不连任何 AI，也不会自动上传任何东西。
// 报告里的体检部分来自最近一次体检，这里写清楚是什么时候的，并给出重新体检的入口。
// 用户自己写的问题描述由后端放进报告最前面、和报告一起脱敏（用户名、电脑名、IP、手机号这些）。

/** 问题描述最多写多少字（后端也按这个截断） */
const NOTE_MAX = 1000

/** 复制给 AI 时放在报告前后的话：请它说人话、一步步来，并避开常见的坑 */
const AI_BEFORE =
  '我的电脑遇到了问题，想请你帮忙看看。下面是「电脑小药箱」（一个开源的 Windows 诊断工具）生成的诊断报告，已经去掉了用户名、电脑名、IP 地址这些隐私信息。'
const AI_AFTER = [
  '请你：',
  '1. 用大白话说说最可能是什么原因；',
  '2. 一步一步告诉我怎么办，每一步写清楚在哪里点什么；',
  '3. 哪一步有风险、做之前要先备份什么，提前说清楚；拿不准的地方，告诉我该找什么样的人帮忙。',
  '请不要让我下载来路不明的软件或者单独的 DLL 文件，也不要让我关掉杀毒软件。',
].join('\n')

const report = ref<string | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)
const note = ref('')
/** 生成报告时用的问题描述；之后又改过的话，复制前先重新生成 */
const noteAtReport = ref('')
/** 最近一次复制的是给谁的；文本框里显示的就是复制出去的内容 */
const copiedFor = ref<'helper' | 'ai'>('helper')
const copyState = ref<'idle' | 'ok' | 'fail'>('idle')
const textarea = useTemplateRef<HTMLTextAreaElement>('textarea')
/** 生成报告那一刻，最近一次体检是什么时候 */
const healthRunAtReport = ref<string | null>(null)

/** 生成报告以后又体检过：报告里的体检结果已经不是最新的 */
const reportOutdated = computed(() => report.value !== null && health.lastRunAt !== healthRunAtReport.value)

const aiText = computed(() => (report.value === null ? '' : `${AI_BEFORE}\n\n${report.value.trimEnd()}\n\n${AI_AFTER}`))
const shownText = computed(() => (copiedFor.value === 'ai' ? aiText.value : (report.value ?? '')))

async function generate(): Promise<boolean> {
  loading.value = true
  error.value = null
  copyState.value = 'idle'
  const runAt = health.lastRunAt
  const text = note.value
  try {
    report.value = await reportGenerate(text.trim() || undefined)
    noteAtReport.value = text
    healthRunAtReport.value = runAt
    return true
  } catch (e) {
    error.value = errorText(e)
    return false
  } finally {
    loading.value = false
  }
}

async function copy(target: 'helper' | 'ai'): Promise<void> {
  if (report.value === null) return
  // 生成报告以后又改了问题描述：先重新生成，复制出去的才带着最新的描述
  if (note.value !== noteAtReport.value && !(await generate())) return
  copiedFor.value = target
  const text = target === 'ai' ? aiText.value : (report.value ?? '')
  try {
    await navigator.clipboard.writeText(text)
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
        报告已去掉用户名、电脑名、IP、序列号等隐私信息，可以直接发给懂哥，也可以复制给豆包、DeepSeek 这些 AI 问问。小药箱不连任何
        AI，也不会自动上传任何东西。
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

    <div class="note-box card">
      <label for="report-note" class="note-label">你遇到了什么问题？（选填）</label>
      <textarea
        id="report-note"
        v-model="note"
        class="input"
        rows="3"
        :maxlength="NOTE_MAX"
        placeholder="比如：昨天开始上不了网，微信能用，网页打不开"
      ></textarea>
      <p class="muted small">会写在报告最前面。别写名字和密码；用户名、电脑名、IP 地址、手机号会自动去掉。</p>
    </div>

    <div class="actions">
      <button type="button" class="btn btn-primary" :disabled="loading" @click="generate">
        <BusySpinner v-if="loading" size="small" />
        <AppIcon v-else name="report" :size="18" />
        {{ loading ? '正在生成…' : report === null ? '生成报告' : '重新生成' }}
      </button>
      <template v-if="report !== null">
        <button type="button" class="btn btn-secondary" :disabled="loading" @click="copy('helper')">
          <AppIcon name="copy" :size="18" />复制给懂哥
        </button>
        <button type="button" class="btn btn-secondary" :disabled="loading" @click="copy('ai')">
          <AppIcon name="copy" :size="18" />复制给 AI
        </button>
      </template>
      <p v-if="copyState === 'ok' && copiedFor === 'helper'" class="success-text" role="status">
        已经复制好了，可以粘贴到微信里发给懂哥。
      </p>
      <p v-else-if="copyState === 'ok'" class="success-text" role="status">
        已经复制好了，可以粘贴到豆包、DeepSeek、元宝这些 AI 里问。AI 说的不一定都对，动手之前先想一想，拿不准就问懂哥。
      </p>
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
        :value="shownText"
        readonly
        rows="20"
        spellcheck="false"
      ></textarea>
      <p class="muted small">
        发出去之前可以先看一遍<template v-if="copiedFor === 'ai'">（上面是刚才复制给 AI 的内容，前后多了几句请它帮忙的话）</template>。报告只在这里显示，不会存到别处，也不会上传。
      </p>
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

.note-box {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.note-label {
  font-weight: 600;
}

.note-box textarea {
  resize: vertical;
  line-height: 1.6;
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
