<script setup lang="ts">
import { computed, ref } from 'vue'
import { ocrRecognize } from '../api'
import type { OcrView } from '../api/types'
import { catalog, openTool } from '../state'
import { errorText } from '../utils/format'
import { HEIC_ADVICE } from '../utils/imageBatch'
import { imageProblem, pastedFile, toOcrPng } from '../utils/ocrImage'
import { errorCodes, matchSymptoms, type SymptomMatch } from '../utils/symptomMatch'
import BusySpinner from './BusySpinner.vue'

// 看报错截图：软件、系统弹出报错窗口时，截图粘贴进来（或者选图、拖进来），用 Windows 自带的文字识别认出字，
// 再和症状的名字、关键词对一对（utils/symptomMatch.ts），给出最可能的几个症状，点了就打开。认出来的字和错误代码也列出来，
// 能复制去搜、发给懂哥。不联网、不上传。

const INSTALL_TOOL = 'system.install-ocr-chinese'

const emit = defineEmits<{ open: [id: string] }>()

const busy = ref(false)
const problem = ref('')
const heic = ref(false)
const error = ref('')
const result = ref<OcrView | null>(null)
const matches = ref<SymptomMatch[]>([])
const codes = ref<string[]>([])
const copyState = ref<'' | 'ok' | 'fail'>('')
const dragDepth = ref(0)
const picker = ref<HTMLInputElement | null>(null)
let run = 0

const needsChinese = computed(() => {
  const r = result.value
  return !!r && (r.status === 'no-language' || (r.status === 'ok' && !r.chinese))
})

async function recognize(file: File | null | undefined): Promise<void> {
  if (!file || busy.value) return
  const current = ++run
  problem.value = ''
  heic.value = false
  error.value = ''
  result.value = null
  matches.value = []
  codes.value = []
  copyState.value = ''
  const bad = imageProblem(file.name, file.type)
  if (bad.text) {
    problem.value = bad.text
    heic.value = bad.heic
    return
  }
  busy.value = true
  try {
    const png = await toOcrPng(file)
    const r = await ocrRecognize(png.bytes)
    if (current !== run) return
    result.value = r
    if (r.status === 'ok') {
      matches.value = matchSymptoms(r.text, catalog.value?.symptoms ?? [])
      codes.value = errorCodes(r.text)
    }
  } catch (e) {
    if (current === run) error.value = errorText(e)
  } finally {
    if (current === run) busy.value = false
  }
}

function onPick(event: Event): void {
  const input = event.target as HTMLInputElement
  void recognize(input.files?.[0])
  input.value = ''
}

function onDrop(event: DragEvent): void {
  dragDepth.value = 0
  void recognize(event.dataTransfer?.files?.[0])
}

function onPaste(event: ClipboardEvent): void {
  const f = Array.from(event.clipboardData?.files ?? []).find((x) => x.type.startsWith('image/'))
  if (!f) {
    if (event.clipboardData?.types.length) problem.value = '剪贴板里没有图片：先截图（Win + Shift + S，或者微信、QQ 截图），再按 Ctrl + V。'
    return
  }
  event.preventDefault()
  void recognize(pastedFile(f))
}

async function copyText(): Promise<void> {
  try {
    await navigator.clipboard.writeText(result.value?.text ?? '')
    copyState.value = 'ok'
  } catch {
    copyState.value = 'fail'
  }
}
</script>

<template>
  <details class="card shot">
    <summary class="shot-summary">有报错窗口？把截图粘贴进来，帮你认一认</summary>
    <div class="shot-body">
      <p class="muted small">
        软件、系统弹出报错时，截个图（Win + Shift + S），点一下下面的框按 Ctrl + V。小药箱用 Windows 自带的文字识别认出上面的字，再对一对是哪种问题；不联网、不上传。
      </p>
      <div
        class="drop"
        :class="{ over: dragDepth > 0 }"
        tabindex="0"
        role="group"
        aria-label="粘贴报错截图的地方：点一下这里，再按 Ctrl + V"
        @dragenter.prevent="dragDepth++"
        @dragover.prevent
        @dragleave="dragDepth = Math.max(0, dragDepth - 1)"
        @drop.prevent="onDrop"
        @paste="onPaste"
      >
        <span class="muted small">点这里按 Ctrl + V 粘贴截图，或者把图片拖进来，或者</span>
        <button type="button" class="btn btn-secondary btn-small" :disabled="busy" @click="picker?.click()">选择图片</button>
        <input
          ref="picker"
          class="visually-hidden"
          type="file"
          accept="image/*"
          tabindex="-1"
          aria-hidden="true"
          @change="onPick"
        />
      </div>
      <p v-if="busy" class="loading-line" role="status"><BusySpinner size="small" />正在认截图里的字…</p>
      <p v-if="problem" class="danger-text small" role="alert">{{ problem }}</p>
      <p v-if="heic" class="hint small">{{ HEIC_ADVICE }}</p>
      <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>

      <template v-if="result && result.status === 'ok'">
        <p v-if="!result.text" class="small" role="status">截图里没认出字：截得清楚一点、把报错窗口放大一点再试。</p>
        <template v-else>
          <div v-if="matches.length" class="matches" role="status">
            <p class="small">看起来是{{ matches.length > 1 ? '这几种问题之一' : '这个问题' }}，点开照着查：</p>
            <ul class="match-list">
              <li v-for="m in matches" :key="m.id">
                <button type="button" class="btn btn-secondary match" @click="emit('open', m.id)">{{ m.title }}</button>
                <span class="muted small">对上了：{{ m.hits.slice(0, 3).join('、') }}</span>
              </li>
            </ul>
          </div>
          <p v-else class="small" role="status">
            没对上小药箱认得的问题。可以把下面认出来的字复制下来，在上面的搜索框里换个说法搜，或者发给懂哥。
          </p>
          <p v-if="codes.length" class="small">报错里的错误代码：{{ codes.join('、') }}（在网上搜这个代码，比搜整段话准）。</p>
          <details class="text">
            <summary class="small">认出来的字（{{ result.lines }} 行）</summary>
            <pre class="ocr-text">{{ result.text }}</pre>
            <div class="row">
              <button type="button" class="btn btn-ghost btn-small" @click="copyText">复制这些字</button>
              <span v-if="copyState === 'ok'" class="small" role="status">已复制。</span>
              <span v-if="copyState === 'fail'" class="danger-text small" role="alert">自动复制失败，可以用鼠标选中后按 Ctrl + C。</span>
            </div>
          </details>
        </template>
      </template>
      <p v-else-if="result && result.status === 'unsupported'" class="danger-text small" role="alert">
        这台电脑上没有 Windows 的文字识别（很老或者精简过的系统），认不了截图。
      </p>
      <p v-else-if="result && result.status === 'bad-image'" class="danger-text small" role="alert">
        Windows 读不了这张图片：换一张截图试试。
      </p>
      <div v-if="needsChinese" class="banner banner-warning" role="note">
        <div>
          <p class="banner-title">这台电脑没有装中文的文字识别</p>
          <p class="small">中文的报错认不出来。装上中文文字识别以后再粘贴一次（要联网，一般几分钟）。</p>
          <button type="button" class="btn btn-secondary btn-small" @click="openTool(INSTALL_TOOL)">去安装中文文字识别</button>
        </div>
      </div>
    </div>
  </details>
</template>

<style scoped>
.shot { padding: 0; }
.shot-summary { cursor: pointer; padding: 12px 16px; font-weight: 600; }
.shot-body { display: flex; flex-direction: column; gap: 9px; padding: 0 16px 14px; }
.drop {
  display: flex; flex-wrap: wrap; gap: 8px 12px; align-items: center;
  padding: 12px; border: 2px dashed var(--color-border-strong); border-radius: var(--radius);
  background: var(--color-surface-2);
}
.drop:focus-visible, .drop:focus { outline: none; border-color: var(--color-primary); }
.drop.over { border-color: var(--color-primary); background: var(--color-primary-soft); }
.hint { padding: 8px 11px; border-radius: var(--radius); background: var(--tone-info-bg); color: var(--tone-info-text); }
.matches { display: flex; flex-direction: column; gap: 6px; }
.match-list { display: flex; flex-direction: column; gap: 6px; list-style: none; }
.match-list li { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 12px; }
.match { text-align: left; }
.text summary { cursor: pointer; }
.ocr-text {
  margin: 6px 0; padding: 8px 10px; max-height: 14em; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere;
  border-radius: var(--radius); background: var(--color-surface-2); font-family: inherit; font-size: var(--text-small);
}
.row { display: flex; flex-wrap: wrap; gap: 8px 12px; align-items: center; }
.banner .btn { margin-top: 6px; }
</style>
