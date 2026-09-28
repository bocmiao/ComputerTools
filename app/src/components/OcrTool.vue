<script setup lang="ts">
import { computed, ref } from 'vue'
import { ocrRecognize } from '../api'
import type { OcrView } from '../api/types'
import { openTool } from '../state'
import { errorText } from '../utils/format'
import { HEIC_ADVICE, MAX_PIXELS, formatBytes, isHeic } from '../utils/imageBatch'
import BusySpinner from './BusySpinner.vue'

// 图片转文字：用 Windows 自带的文字识别（后端 ocr_recognize → scripts/ocr/recognize.ps1），不联网、不上传。
// 图片先在这里转正（照片里记的方向）、画到白底的画布上存成 PNG（透明的截图也认得出；Windows 只读它认得的格式），
// 再交给后端；后端存成临时文件，认完就删。认出来的字能改、能复制。
// 没有装中文识别时，给「安装中文文字识别」的按钮（小工具 system.install-ocr-chinese）。
// 预览用画布转成 data: 地址显示（界面的内容安全策略不放行 blob: 图片）。

const INSTALL_TOOL = 'system.install-ocr-chinese'
/** 画布每边最多多少像素（WebView2 画不了更大的） */
const MAX_EDGE = 32767
const PREVIEW_WIDTH = 560
const PREVIEW_HEIGHT = 320

const file = ref<File | null>(null)
const preview = ref('')
const size = ref<{ width: number; height: number } | null>(null)
const pickProblem = ref('')
const heicRejected = ref(false)
const busy = ref(false)
const error = ref('')
const result = ref<OcrView | null>(null)
const text = ref('')
/** 图片太大，缩小了再认的 */
const shrunk = ref(false)
const copyState = ref<'' | 'ok' | 'fail'>('')
const dragDepth = ref(0)
const picker = ref<HTMLInputElement | null>(null)
let run = 0

const needsChinese = computed(() => {
  const r = result.value
  return !!r && (r.status === 'no-language' || (r.status === 'ok' && !r.chinese))
})

function openImage(f: File): Promise<ImageBitmap> {
  return createImageBitmap(f, { imageOrientation: 'from-image' }).catch(() => createImageBitmap(f))
}

/** 这张打不开的原因；能认的返回空字符串 */
function problemOf(f: File): string {
  const name = f.name.toLowerCase()
  if (isHeic(f.name, f.type)) {
    heicRejected.value = true
    return `「${f.name}」是 HEIC（苹果手机的照片格式），这里打不开，办法见下面。`
  }
  if (name.endsWith('.tif') || name.endsWith('.tiff') || f.type === 'image/tiff') {
    return `「${f.name}」是 TIFF 格式，这里打不开：先用「画图」打开，另存为 PNG 再来。`
  }
  if (name.endsWith('.svg') || f.type === 'image/svg+xml') return `「${f.name}」是 SVG 矢量图，这里打不开。`
  if (!f.type.startsWith('image/') && !/\.(jpe?g|png|webp|gif|bmp|ico|avif)$/.test(name)) {
    return `「${f.name}」不是图片文件。`
  }
  return ''
}

async function setFile(f: File | null | undefined): Promise<void> {
  if (!f || busy.value) return
  run++
  pickProblem.value = ''
  heicRejected.value = false
  error.value = ''
  result.value = null
  text.value = ''
  copyState.value = ''
  const problem = problemOf(f)
  if (problem) {
    pickProblem.value = problem
    return
  }
  const current = run
  file.value = f
  preview.value = ''
  size.value = null
  try {
    const bitmap = await openImage(f)
    try {
      if (current !== run) return
      size.value = { width: bitmap.width, height: bitmap.height }
      const scale = Math.min(1, PREVIEW_WIDTH / bitmap.width, PREVIEW_HEIGHT / bitmap.height)
      const canvas = document.createElement('canvas')
      canvas.width = Math.max(1, Math.round(bitmap.width * scale))
      canvas.height = Math.max(1, Math.round(bitmap.height * scale))
      const ctx = canvas.getContext('2d')
      if (ctx) {
        ctx.fillStyle = '#fff'
        ctx.fillRect(0, 0, canvas.width, canvas.height)
        ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height)
        preview.value = canvas.toDataURL('image/jpeg', 0.85)
      }
    } finally {
      bitmap.close()
    }
  } catch {
    if (current === run) {
      file.value = null
      pickProblem.value = `「${f.name}」打不开，可能不是图片，或者文件坏了。`
    }
  }
}

function onPick(event: Event): void {
  const input = event.target as HTMLInputElement
  void setFile(input.files?.[0])
  input.value = ''
}

function onDrop(event: DragEvent): void {
  dragDepth.value = 0
  void setFile(event.dataTransfer?.files?.[0])
}

function onPaste(event: ClipboardEvent): void {
  const f = Array.from(event.clipboardData?.files ?? []).find((x) => x.type.startsWith('image/'))
  if (!f) {
    if (event.clipboardData?.types.length) pickProblem.value = '剪贴板里没有图片：先截图（或者在图片上点右键「复制图片」），再按 Ctrl + V。'
    return
  }
  event.preventDefault()
  const ext = f.type === 'image/jpeg' ? 'jpg' : (f.type.split('/')[1] ?? 'png')
  void setFile(new File([f], `截图.${ext}`, { type: f.type, lastModified: Date.now() }))
}

/** 转正、画到白底上，存成 PNG；太大的按比例缩小 */
async function toPng(f: File): Promise<Uint8Array> {
  const bitmap = await openImage(f)
  try {
    const { width, height } = bitmap
    const scale = Math.min(1, MAX_EDGE / width, MAX_EDGE / height, Math.sqrt(MAX_PIXELS / (width * height)))
    shrunk.value = scale < 1
    const canvas = document.createElement('canvas')
    canvas.width = Math.max(1, Math.floor(width * scale))
    canvas.height = Math.max(1, Math.floor(height * scale))
    const ctx = canvas.getContext('2d')
    if (!ctx) throw new Error('没能打开这张图片。')
    ctx.fillStyle = '#fff'
    ctx.fillRect(0, 0, canvas.width, canvas.height)
    ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height)
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'))
    if (!blob) throw new Error('没能把图片转成 PNG，可能是图片太大了。')
    return new Uint8Array(await blob.arrayBuffer())
  } finally {
    bitmap.close()
  }
}

async function recognize(): Promise<void> {
  const f = file.value
  if (!f || busy.value) return
  const current = ++run
  busy.value = true
  error.value = ''
  result.value = null
  text.value = ''
  copyState.value = ''
  try {
    const bytes = await toPng(f)
    const r = await ocrRecognize(bytes)
    if (current !== run) return
    result.value = r
    text.value = r.text
  } catch (e) {
    if (current === run) error.value = errorText(e)
  } finally {
    if (current === run) busy.value = false
  }
}

async function copyText(): Promise<void> {
  try {
    await navigator.clipboard.writeText(text.value)
    copyState.value = 'ok'
  } catch {
    copyState.value = 'fail'
  }
}

function clear(): void {
  run++
  file.value = null
  preview.value = ''
  size.value = null
  result.value = null
  text.value = ''
  error.value = ''
  pickProblem.value = ''
  copyState.value = ''
  busy.value = false
}
</script>

<template>
  <article class="card ocr-card">
    <h3 class="section-title">图片转文字</h3>
    <p class="muted small">
      截图、拍的文件、表格照片里的字认出来，变成能复制、能改的文字：用的是 Windows 自带的文字识别，不联网、不上传。印刷体认得准；手写的字、艺术字、很小很糊的字认不准，认完对一遍再用。
    </p>

    <div
      class="drop"
      :class="{ over: dragDepth > 0 }"
      tabindex="0"
      role="group"
      aria-label="拖放或粘贴图片的地方：点一下这里，再按 Ctrl + V 粘贴截图"
      @dragenter.prevent="dragDepth++"
      @dragover.prevent
      @dragleave="dragDepth = Math.max(0, dragDepth - 1)"
      @drop.prevent="onDrop"
      @paste="onPaste"
    >
      <span class="muted small">把一张图片拖到这里，或者</span>
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
      <span class="muted small paste-tip">刚截了图的（Win + Shift + S、微信、QQ 截图）：点一下这个框，按 Ctrl + V 粘贴进来。</span>
    </div>
    <p v-if="pickProblem" class="danger-text small" role="alert">{{ pickProblem }}</p>
    <p v-if="heicRejected" class="hint small">{{ HEIC_ADVICE }}</p>

    <template v-if="file">
      <div class="picked">
        <img v-if="preview" class="preview" :src="preview" alt="要认字的图片" />
        <div class="picked-info">
          <p class="small">{{ file.name }}</p>
          <p class="muted small">
            <template v-if="size">{{ size.width }} × {{ size.height }} 像素 · </template>{{ formatBytes(file.size) }}
          </p>
          <div class="row">
            <button type="button" class="btn btn-primary" :disabled="busy" @click="recognize">
              <BusySpinner v-if="busy" size="small" />{{ busy ? '正在认字…' : '认出文字' }}
            </button>
            <button type="button" class="btn btn-ghost btn-small" :disabled="busy" @click="clear">换一张</button>
          </div>
        </div>
      </div>
    </template>
    <p v-if="busy" class="muted small" role="status">长截图要分成几段来认，要等一会儿。</p>
    <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>

    <template v-if="result">
      <template v-if="result.status === 'ok'">
        <p v-if="result.lines === 0" class="small" role="status">
          没有认出字。把图片截得清楚一点、字大一点再试；手写的字、艺术字认不出来。
        </p>
        <template v-else>
          <label class="field-label" for="ocr-text">
            认出来的文字（{{ result.lines }} 行<template v-if="result.language">，按{{ result.language }}认的</template>）
          </label>
          <textarea id="ocr-text" v-model="text" class="input text-box" rows="10" spellcheck="false"></textarea>
          <div class="row">
            <button type="button" class="btn btn-secondary btn-small" :disabled="!text" @click="copyText">复制全部</button>
            <span v-if="copyState === 'ok'" class="small" role="status">已复制，可以粘贴到微信、Word 里了。</span>
            <span v-if="copyState === 'fail'" class="danger-text small" role="alert">自动复制失败：在上面的框里按 Ctrl + A、Ctrl + C。</span>
          </div>
          <p class="muted small">认得不对的地方直接在框里改。表格认出来是一行一行的字，不会还原成表格。</p>
        </template>
        <p v-if="result.truncated" class="hint small">图片太长，只认了前面一大段：把长图截成几段，分别认。</p>
        <p v-if="shrunk" class="hint small">图片太大，缩小了一些再认的，字小的地方可能认不准：可以截成几段分别认。</p>
      </template>
      <p v-else-if="result.status === 'unsupported'" class="danger-text small" role="alert">
        这台电脑上没有 Windows 的文字识别（很老或者精简过的系统），用不了这个功能。
      </p>
      <p v-else-if="result.status === 'bad-image'" class="danger-text small" role="alert">
        Windows 读不了这张图片：换一张试试，或者先用「画图」打开、另存为 PNG 再来。
      </p>
      <p v-if="result.detail && result.status !== 'ok'" class="muted small">Windows 的说法：{{ result.detail }}</p>
      <div v-if="needsChinese" class="banner banner-warning" role="note">
        <div>
          <p class="banner-title">这台电脑没有装中文的文字识别</p>
          <p class="small">
            <template v-if="result.status === 'ok' && result.language">刚才是按{{ result.language }}认的，中文认不出来。</template>
            <template v-else>一种文字识别都没有装。</template>
            装上中文文字识别以后再认一次（要联网，一般几分钟；中文版 Windows 一般自带，精简过的系统和英文版 Windows 才要装）。
          </p>
          <button type="button" class="btn btn-secondary btn-small" @click="openTool(INSTALL_TOOL)">去安装中文文字识别</button>
        </div>
      </div>
    </template>
  </article>
</template>

<style scoped>
.ocr-card { display: flex; flex-direction: column; gap: 9px; grid-column: 1 / -1; }
.field-label { font-weight: 600; font-size: var(--text-small); margin-top: 4px; }
.row { display: flex; flex-wrap: wrap; gap: 8px 14px; align-items: center; }
.drop {
  display: flex; flex-wrap: wrap; gap: 8px 12px; align-items: center;
  padding: 14px; border: 2px dashed var(--color-border-strong); border-radius: var(--radius);
  background: var(--color-surface-2);
}
.drop:focus-visible, .drop:focus { outline: none; border-color: var(--color-primary); }
.drop.over { border-color: var(--color-primary); background: var(--color-primary-soft); }
.paste-tip { flex-basis: 100%; }
.hint { padding: 8px 11px; border-radius: var(--radius); background: var(--tone-info-bg); color: var(--tone-info-text); }
.picked { display: flex; flex-wrap: wrap; gap: 12px 16px; align-items: flex-start; }
.preview { max-width: min(100%, 560px); max-height: 320px; border: 1px solid var(--color-border); border-radius: var(--radius); background: #fff; }
.picked-info { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
.text-box { min-height: 12em; resize: vertical; font-family: inherit; line-height: 1.6; }
.banner .btn { margin-top: 6px; }
</style>
