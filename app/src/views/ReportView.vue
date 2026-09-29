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
// 左边：写问题、看报告里有什么、生成；右边：看报告、复制。

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
/** 报告是什么时候生成的（报告上方显示） */
const generatedAt = ref<string | null>(null)
/** 最近一次复制的是给谁的；文本框里显示的就是复制出去的内容 */
const copiedFor = ref<'helper' | 'ai'>('helper')
const copyState = ref<'idle' | 'ok' | 'fail'>('idle')
const textarea = useTemplateRef<HTMLTextAreaElement>('textarea')
const preview = useTemplateRef<HTMLElement>('preview')
/** 生成报告那一刻，最近一次体检是什么时候 */
const healthRunAtReport = ref<string | null>(null)

/** 生成报告以后又体检过：报告里的体检结果已经不是最新的 */
const reportOutdated = computed(() => report.value !== null && health.lastRunAt !== healthRunAtReport.value)
/** 生成报告以后又改了问题描述：复制时会先重新生成（见 copy） */
const noteChanged = computed(() => report.value !== null && note.value !== noteAtReport.value)

const aiText = computed(() => (report.value === null ? '' : `${AI_BEFORE}\n\n${report.value.trimEnd()}\n\n${AI_AFTER}`))
const shownText = computed(() => (copiedFor.value === 'ai' ? aiText.value : (report.value ?? '')))

/**
 * 报告里有什么：照后端 Engine::report_generate 写（问题描述另外写在输入框下面）。
 * 后端只收一句问题描述、不能挑内容，所以这里只列出来，不给勾选。
 */
const contents = computed(() => [
  { name: '系统和小药箱的版本', note: '' },
  { name: '体检结果', note: health.lastRunAt ? formatDateTime(health.lastRunAt) : '还没体检' },
  { name: '在「按症状修」里查过的项目', note: '查过才有' },
  { name: '小药箱改过的地方', note: '最近 30 处' },
])

/** 给读屏软件的一句话：正在生成、生成好了（这个区域一直在，内容变了才会读） */
const statusText = computed(() => {
  if (loading.value) return '正在生成报告…'
  return report.value !== null ? '报告已经生成好了，可以复制了。' : ''
})

async function generate(): Promise<boolean> {
  loading.value = true
  error.value = null
  copyState.value = 'idle'
  // 重新生成的报告还没复制过：文本框里先显示报告本身（复制给 AI 时再换）
  copiedFor.value = 'helper'
  const runAt = health.lastRunAt
  const text = note.value
  try {
    report.value = await reportGenerate(text.trim() || undefined)
    noteAtReport.value = text
    healthRunAtReport.value = runAt
    generatedAt.value = new Date().toISOString()
    return true
  } catch (e) {
    error.value = errorText(e)
    return false
  } finally {
    loading.value = false
  }
}

/** 点「生成报告」「重新生成」：窗口窄、报告排在下面看不见时，滚过去让用户看到（焦点留在按钮上） */
async function generateAndShow(): Promise<void> {
  if (!(await generate())) return
  await nextTick()
  const el = preview.value
  if (!el || el.getBoundingClientRect().top < window.innerHeight - 200) return
  const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches
  el.scrollIntoView({ block: 'start', behavior: reduce ? 'auto' : 'smooth' })
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
    <header class="page-head">
      <div class="page-head-main">
        <h1 class="page-title" tabindex="-1">诊断报告</h1>
        <p class="page-lead">生成一份去掉隐私信息的报告，发给懂哥，或者复制给豆包、DeepSeek 这些 AI 问问。</p>
      </div>
    </header>

    <p class="visually-hidden" role="status">{{ statusText }}</p>

    <div class="report-layout">
      <!-- 左边：写问题、看报告里有什么、生成 -->
      <div class="report-side">
        <section class="card note-card">
          <label for="report-note" class="card-title">你遇到了什么问题？<span class="optional">（选填）</span></label>
          <textarea
            id="report-note"
            v-model="note"
            class="input"
            rows="4"
            :maxlength="NOTE_MAX"
            placeholder="比如：昨天开始上不了网，微信能用，网页打不开"
            aria-describedby="report-note-hint"
          ></textarea>
          <p id="report-note-hint" class="muted small">
            会写在报告最前面。别写名字和密码；用户名、电脑名、IP 地址、手机号会自动去掉。
          </p>
        </section>

        <section class="card contents-card" aria-labelledby="report-contents-title">
          <h2 id="report-contents-title" class="card-title">报告里有什么</h2>
          <ul class="contents">
            <li v-for="item in contents" :key="item.name" class="contents-item">
              <AppIcon name="check" :size="16" class="contents-check" />
              <span class="contents-name">{{ item.name }}</span>
              <span v-if="item.note" class="contents-note">{{ item.note }}</span>
            </li>
          </ul>
          <div class="health-source">
            <p class="health-source-text">
              <template v-if="!health.lastRunAt">这次打开小药箱以后还没有体检过，报告里不会有体检结果。</template>
              <template v-else-if="health.stale">
                体检以后你又改过或撤销过设置，这些结果可能已经过期，建议先重新体检。
              </template>
              <template v-else>报告里的体检结果来自最近一次体检。</template>
            </p>
            <button type="button" class="btn btn-secondary btn-small" @click="requestHealthCheck">
              {{ health.lastRunAt ? '重新体检' : '先去体检' }}
            </button>
          </div>
        </section>

        <button type="button" class="btn btn-primary generate" :disabled="loading" @click="generateAndShow">
          <BusySpinner v-if="loading" size="small" />
          <AppIcon v-else :name="report === null ? 'report' : 'refresh'" :size="18" />
          {{ loading ? '正在生成…' : report === null ? '生成报告' : '重新生成' }}
        </button>

        <div v-if="error" class="banner banner-error" role="alert">
          <div>
            <p class="banner-title">报告没能生成</p>
            <p>{{ error }}</p>
          </div>
        </div>

        <p class="privacy">
          <AppIcon name="lock" :size="16" class="privacy-icon" />
          <span>报告里没有用户名、电脑名、IP 地址和序列号。小药箱不连任何 AI，也不会自动上传任何东西。</span>
        </p>
      </div>

      <!-- 右边：看报告、复制 -->
      <section
        ref="preview"
        class="card preview"
        :class="{ 'is-empty': report === null }"
        aria-labelledby="report-preview-title"
      >
        <header class="preview-bar">
          <h2 id="report-preview-title" class="preview-title">报告</h2>
          <span v-if="report !== null && generatedAt" class="preview-time">{{ formatDateTime(generatedAt) }} 生成</span>
          <div v-if="report !== null" class="preview-actions">
            <button type="button" class="btn btn-secondary btn-small" :disabled="loading" @click="copy('helper')">
              <AppIcon name="copy" :size="16" />复制
            </button>
            <button type="button" class="btn btn-primary btn-small" :disabled="loading" @click="copy('ai')">
              <AppIcon name="chat" :size="16" />复制给 AI
            </button>
          </div>
        </header>

        <template v-if="report !== null">
          <p v-if="copyState === 'ok' && copiedFor === 'helper'" class="preview-msg success-text" role="status">
            <AppIcon name="check" :size="16" class="msg-icon" />已经复制好了，可以粘贴到微信里发给懂哥。
          </p>
          <p v-else-if="copyState === 'ok'" class="preview-msg success-text" role="status">
            <AppIcon name="check" :size="16" class="msg-icon" />已经复制好了，可以粘贴到豆包、DeepSeek、元宝这些 AI
            里问。AI 说的不一定都对，动手之前先想一想，拿不准就问懂哥。
          </p>
          <p v-else-if="copyState === 'fail'" class="preview-msg danger-text" role="alert">
            <AppIcon name="warning" :size="16" class="msg-icon" />没能自动复制。文字已经选中了，请按 Ctrl+C
            复制；或者点一下下面的文本框，按 Ctrl+A 全选后再按 Ctrl+C。
          </p>
          <p v-if="reportOutdated" class="preview-msg preview-hint" role="status">
            <AppIcon name="info" :size="16" class="msg-icon" />生成这份报告以后又体检过了，点「重新生成」拿到最新的报告。
          </p>
          <p v-else-if="noteChanged" class="preview-msg preview-hint">
            <AppIcon name="info" :size="16" class="msg-icon" />问题描述改过了，复制时会先重新生成报告。
          </p>

          <label for="report-text" class="visually-hidden">诊断报告内容</label>
          <textarea
            id="report-text"
            ref="textarea"
            class="report-text"
            :value="shownText"
            readonly
            spellcheck="false"
            aria-describedby="report-text-hint"
          ></textarea>
          <p id="report-text-hint" class="preview-foot">
            发出去之前可以先看一遍<template v-if="copiedFor === 'ai'">（上面是刚才复制给 AI 的内容，前后多了几句请它帮忙的话）</template>。报告只在这里显示，不会存到别处，也不会上传。
          </p>
        </template>

        <div v-else class="placeholder">
          <span class="placeholder-icon"><AppIcon name="report" :size="28" /></span>
          <template v-if="loading">
            <p class="loading-line"><BusySpinner size="small" />正在生成报告…</p>
          </template>
          <template v-else>
            <p class="placeholder-title">报告会显示在这里</p>
            <p class="muted small">写一句遇到的问题（也可以不写），再点「生成报告」。发出去之前，可以先在这里看一遍。</p>
          </template>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
/* 左边一栏跟着窗口宽窄在 340–430px 之间；窗口窄了变成一栏（和别的页面的两栏同一个宽度） */
.report-layout {
  display: grid;
  grid-template-columns: clamp(340px, 38%, 430px) minmax(0, 1fr);
  gap: 20px;
}

@media (max-width: 1100px) {
  .report-layout {
    grid-template-columns: minmax(0, 1fr);
  }

  /* 一栏时报告排在下面：还没生成的时候不用占一整屏 */
  .preview.is-empty {
    min-height: 0;
  }
}

.report-side {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
}

.card-title {
  font-size: var(--text-base);
  font-weight: 600;
}

.optional {
  font-weight: 400;
  color: var(--color-text-muted);
}

/* ───── 问题描述 ───── */

.note-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 18px 20px;
}

.note-card textarea {
  resize: vertical;
  line-height: 1.6;
}

/* ───── 报告里有什么 ───── */

.contents-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 18px 20px;
}

.contents {
  display: flex;
  flex-direction: column;
  gap: 8px;
  list-style: none;
}

.contents-item {
  display: flex;
  align-items: center;
  gap: 10px;
}

.contents-check {
  color: var(--tone-ok-text);
}

.contents-name {
  flex: 1;
  min-width: 0;
}

.contents-note {
  color: var(--color-text-muted);
  font-size: var(--text-small);
  white-space: nowrap;
}

.health-source {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px 12px;
  margin-top: 4px;
  padding-top: 12px;
  border-top: 1px solid var(--color-divider);
}

.health-source-text {
  flex: 1 1 180px;
  min-width: 0;
  color: var(--color-text-muted);
  font-size: var(--text-small);
}

/* ───── 生成 ───── */

.generate {
  min-height: 46px;
  font-weight: 600;
}

.privacy {
  display: flex;
  gap: 8px;
  padding: 0 4px;
  color: var(--color-text-muted);
  font-size: var(--text-small);
  line-height: 1.7;
}

.privacy-icon {
  margin-top: 3px;
}

/* ───── 右边：报告 ───── */

/* 和窗口差不多高（减掉页头和上下留白），报告在里面滚；两栏时和左边一栏一样高。
   提示多了，挤的是下面的文本框，不会把页面撑长 */
.preview {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: max(460px, calc(100vh - 170px));
  padding: 0;
  overflow: hidden;
}

.preview-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 10px;
  min-height: 58px;
  padding: 10px 16px 10px 20px;
  border-bottom: 1px solid var(--color-border);
}

.preview-title {
  font-size: var(--text-large);
}

.preview-time {
  color: var(--color-text-muted);
  font-size: var(--text-small);
}

.preview-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-left: auto;
}

.preview-msg {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 10px 20px;
  border-bottom: 1px solid var(--color-divider);
  font-size: var(--text-small);
}

.preview-hint {
  color: var(--color-text-muted);
}

.msg-icon {
  margin-top: 2px;
}

/* 只读的文本框：自动复制不行时能选中、Ctrl+A 只选报告本身；看起来像一块等宽字的纯文本 */
.report-text {
  flex: 1 1 0;
  width: 100%;
  min-height: 240px;
  margin: 0;
  padding: 18px 22px;
  border: none;
  border-radius: 0;
  background: color-mix(in srgb, var(--color-surface) 55%, var(--color-bg));
  color: var(--color-text);
  font-family: var(--font-mono);
  font-size: 13.5px;
  line-height: 1.8;
  resize: none;
}

.report-text:focus-visible {
  outline-offset: -3px;
}

.preview-foot {
  padding: 10px 20px 12px;
  border-top: 1px solid var(--color-divider);
  color: var(--color-text-muted);
  font-size: var(--text-small);
}

/* 还没有报告 */
.placeholder {
  display: flex;
  flex: 1;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 40px 24px;
  text-align: center;
}

.placeholder-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 60px;
  height: 60px;
  margin-bottom: 6px;
  border-radius: 50%;
  background: var(--color-primary-soft);
  color: var(--color-primary-soft-text);
}

.placeholder-title {
  font-size: var(--text-large);
  font-weight: 600;
}

.placeholder .muted {
  max-width: 26em;
}
</style>
