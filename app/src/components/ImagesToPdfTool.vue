<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { isTauri, pdfReveal, pdfSave } from '../api'
import { errorText } from '../utils/format'
import { MAX_PIXELS, formatBytes, isHeic, HEIC_ADVICE, unsupportedReason } from '../utils/imageBatch'
import {
  CLARITIES,
  MAX_PAGES,
  MAX_PDF_BYTES,
  buildPdf,
  byName,
  pageLayout,
  scaledSize,
  suggestedName,
  turnedSize,
  type PdfPage,
  type PdfRules,
} from '../utils/pdf'

// 图片合成 PDF：拍的证件、合同、作业、发票照片按顺序合成一个 PDF（交材料、发邮件时常要 PDF）。
// 图片在这里用 WebView2 自带的解码器打开（按照片里记的方向转正），按要转的角度画到画布上存成 JPEG，
// 再拼成 PDF（utils/pdf.ts，不加新的依赖）。存到哪里由系统的「另存为」对话框选，原图不动。
// 缩略图用画布缩成小图、转成 data: 地址显示（界面的内容安全策略不放行 blob: 图片）。

// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件
defineProps<{ hideTitle?: boolean }>()

interface Item {
  id: number
  file: File
  /** 顺时针转了几个 90° */
  turns: number
  /** 缩略图（data: 地址），还没做好是空的 */
  thumb: string
  /** 这张打不开时的原因 */
  note: string
}

const THUMB_EDGE = 96

const items = ref<Item[]>([])
let nextId = 0
const rules = reactive<PdfRules>({ page: 'a4', orientation: 'auto', margin: 'narrow', clarity: 'standard' })
const running = ref(false)
const stopRequested = ref(false)
const current = ref(0)
const phase = ref<'images' | 'saving'>('images')
const error = ref('')
/** 加图片时没加进来的（放在选择框下面） */
const addProblem = ref('')
const notice = ref('')
const heicRejected = ref(false)
const saved = ref<{ path: string; pages: number; bytes: number } | null>(null)
const dragDepth = ref(0)
const picker = ref<HTMLInputElement | null>(null)
/** 上一次拼好的 PDF：图片、顺序、角度、设置都没变时，「另存为」点了取消再点保存，不用重新处理 */
let built: { key: string; bytes: Uint8Array } | null = null

const totalSize = computed(() => items.value.reduce((sum, i) => sum + i.file.size, 0))

function buildKey(): string {
  return JSON.stringify({ rules, items: items.value.map((i) => [i.id, i.turns % 4]) })
}

function addFiles(list: FileList | File[] | null | undefined): void {
  if (!list || running.value) return
  error.value = ''
  addProblem.value = ''
  notice.value = ''
  const known = new Set(items.value.map((i) => `${i.file.name}|${i.file.size}|${i.file.lastModified}`))
  const rejected: string[] = []
  let dropped = 0
  for (const file of Array.from(list)) {
    const key = `${file.name}|${file.size}|${file.lastModified}`
    if (known.has(key)) continue
    if (unsupportedReason(file.name, file.type)) {
      if (isHeic(file.name, file.type)) heicRejected.value = true
      rejected.push(file.name)
      continue
    }
    if (items.value.length >= MAX_PAGES) {
      dropped++
      continue
    }
    known.add(key)
    items.value.push({ id: nextId++, file, turns: 0, thumb: '', note: '' })
  }
  const problems: string[] = []
  if (rejected.length) {
    const names = rejected.slice(0, 3).map((n) => `「${n}」`).join('、')
    problems.push(`${names}${rejected.length > 3 ? ` 等 ${rejected.length} 个文件` : ''}打不开，没有加进来（HEIC、TIFF、SVG 和不是图片的文件都不行）。`)
  }
  if (dropped) problems.push(`一个 PDF 最多 ${MAX_PAGES} 页，多出来的 ${dropped} 张没有加进来。`)
  addProblem.value = problems.join('')
  void makeThumbs()
}

function onPick(event: Event): void {
  const input = event.target as HTMLInputElement
  addFiles(input.files)
  input.value = ''
}

function onDrop(event: DragEvent): void {
  dragDepth.value = 0
  addFiles(event.dataTransfer?.files)
}

/** 一张一张做缩略图（一次全解码，几十张大照片会卡住界面） */
let thumbing = false
async function makeThumbs(): Promise<void> {
  if (thumbing) return
  thumbing = true
  const canvas = document.createElement('canvas')
  try {
    for (;;) {
      const item = items.value.find((i) => !i.thumb && !i.note)
      if (!item) break
      try {
        const bitmap = await createImageBitmap(item.file, { imageOrientation: 'from-image', resizeWidth: THUMB_EDGE, resizeQuality: 'medium' })
          .catch(() => createImageBitmap(item.file))
        try {
          const scale = Math.min(1, THUMB_EDGE / Math.max(bitmap.width, bitmap.height))
          canvas.width = Math.max(1, Math.round(bitmap.width * scale))
          canvas.height = Math.max(1, Math.round(bitmap.height * scale))
          const context = canvas.getContext('2d')
          if (!context) throw new Error('no canvas')
          // 和 PDF 里一样，透明的地方是白色
          context.fillStyle = '#ffffff'
          context.fillRect(0, 0, canvas.width, canvas.height)
          context.drawImage(bitmap, 0, 0, canvas.width, canvas.height)
          item.thumb = canvas.toDataURL('image/jpeg', 0.8)
        } finally {
          bitmap.close()
        }
      } catch {
        item.note = '这张图片打不开：文件可能坏了，或者不是它的扩展名说的那种格式。'
      }
    }
  } finally {
    canvas.width = 0
    canvas.height = 0
    thumbing = false
  }
}

function move(index: number, by: number): void {
  const list = items.value
  const to = index + by
  if (running.value || to < 0 || to >= list.length) return
  const [item] = list.splice(index, 1)
  if (item) list.splice(to, 0, item)
}

function turn(item: Item): void {
  if (!running.value) item.turns = (item.turns + 1) % 4
}

function remove(index: number): void {
  if (!running.value) items.value.splice(index, 1)
}

function sortByName(): void {
  if (!running.value) items.value.sort((a, b) => byName(a.file.name, b.file.name))
}

function clearList(): void {
  if (running.value) return
  items.value = []
  error.value = ''
  addProblem.value = ''
  notice.value = ''
  heicRejected.value = false
  saved.value = null
  built = null
}

function toJpeg(canvas: HTMLCanvasElement, quality: number): Promise<Blob> {
  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error('没能生成图片（图片可能太大了）。'))), 'image/jpeg', quality)
  })
}

/** 一张图片变成 PDF 的一页：转正、按角度转、缩到清晰度的上限，白底存成 JPEG */
async function renderPage(item: Item, canvas: HTMLCanvasElement): Promise<PdfPage> {
  let bitmap: ImageBitmap
  try {
    bitmap = await createImageBitmap(item.file, { imageOrientation: 'from-image' }).catch(() => createImageBitmap(item.file))
  } catch {
    throw new Error('这张图片打不开：文件可能坏了，或者不是它的扩展名说的那种格式。')
  }
  try {
    if (bitmap.width * bitmap.height > MAX_PIXELS) {
      throw new Error(`这张图片有 ${Math.round((bitmap.width * bitmap.height) / 1_000_000)} 百万像素，太大了，处理不了。`)
    }
    const turns = item.turns % 4
    const turned = turnedSize(bitmap.width, bitmap.height, turns)
    const out = scaledSize(turned.width, turned.height, rules.clarity)
    canvas.width = out.width
    canvas.height = out.height
    const context = canvas.getContext('2d')
    if (!context) throw new Error('这台电脑上建不了处理图片用的画布。')
    context.imageSmoothingEnabled = true
    context.imageSmoothingQuality = 'high'
    // JPEG 没有透明，透明的地方填白色（不填会变成黑色）
    context.fillStyle = '#ffffff'
    context.fillRect(0, 0, out.width, out.height)
    context.save()
    context.translate(out.width / 2, out.height / 2)
    context.rotate((turns * Math.PI) / 2)
    const drawWidth = turns % 2 ? out.height : out.width
    const drawHeight = turns % 2 ? out.width : out.height
    context.drawImage(bitmap, -drawWidth / 2, -drawHeight / 2, drawWidth, drawHeight)
    context.restore()
    const blob = await toJpeg(canvas, CLARITIES[rules.clarity].quality)
    if (blob.type !== 'image/jpeg') throw new Error('这台电脑存不了 JPG 格式，生成不了 PDF。')
    return {
      jpeg: new Uint8Array(await blob.arrayBuffer()),
      pixelWidth: out.width,
      pixelHeight: out.height,
      layout: pageLayout(out.width, out.height, rules),
    }
  } finally {
    bitmap.close()
  }
}

async function makePdf(): Promise<Uint8Array | null> {
  const key = buildKey()
  if (built && built.key === key) return built.bytes
  const pages: PdfPage[] = []
  let size = 0
  const canvas = document.createElement('canvas')
  try {
    for (const [index, item] of items.value.entries()) {
      if (stopRequested.value) {
        notice.value = '已经停下，PDF 没有生成。'
        return null
      }
      current.value = index + 1
      try {
        const page = await renderPage(item, canvas)
        size += page.jpeg.length
        if (size > MAX_PDF_BYTES) throw new Error('合在一起太大了。换小一点的清晰度，或者分成几个 PDF。')
        pages.push(page)
      } catch (e) {
        item.note = errorText(e)
        // 少一页的 PDF 交上去不对，所以停下来，让用户把这张去掉或者换一张
        error.value = `第 ${index + 1} 张「${item.file.name}」：${item.note}把它去掉（或者换一张）再生成。`
        return null
      }
    }
  } finally {
    canvas.width = 0
    canvas.height = 0
  }
  const bytes = buildPdf(pages)
  built = { key, bytes }
  return bytes
}

async function save(): Promise<void> {
  if (running.value || !items.value.length) return
  running.value = true
  stopRequested.value = false
  phase.value = 'images'
  current.value = 0
  error.value = ''
  notice.value = ''
  saved.value = null
  for (const i of items.value) if (i.thumb) i.note = ''
  try {
    const bytes = await makePdf()
    if (!bytes) return
    phase.value = 'saving'
    const path = await pdfSave(suggestedName(new Date()), bytes)
    if (path === null) {
      notice.value = '没有保存（在「另存为」里点了取消）。再点一次保存不用重新处理。'
      return
    }
    saved.value = { path, pages: items.value.length, bytes: bytes.length }
  } catch (e) {
    error.value = errorText(e)
  } finally {
    running.value = false
  }
}

async function reveal(): Promise<void> {
  try {
    await pdfReveal()
  } catch (e) {
    error.value = errorText(e)
  }
}
</script>

<template>
  <article class="card pdf-card">
    <h3 :class="hideTitle ? 'visually-hidden' : 'section-title'">图片合成 PDF</h3>
    <p class="muted small">
      把拍的证件、合同、作业、发票照片按顺序合成一个 PDF 文件（交材料、发邮件时常要 PDF）。每张图片一页，可以调整顺序、转方向。原图不动；存到哪里、叫什么，在「另存为」里自己选。
    </p>
    <p v-if="!isTauri()" class="muted small">浏览器里只演示生成过程，不会真的保存文件；在 Windows 的小药箱里才会弹出「另存为」。</p>

    <div
      class="drop"
      :class="{ over: dragDepth > 0 }"
      @dragenter.prevent="dragDepth++"
      @dragover.prevent
      @dragleave="dragDepth = Math.max(0, dragDepth - 1)"
      @drop.prevent="onDrop"
    >
      <span class="muted small">把图片拖到这里，或者</span>
      <button type="button" class="btn btn-secondary btn-small" :disabled="running" @click="picker?.click()">选择图片</button>
      <input
        ref="picker"
        class="visually-hidden"
        type="file"
        multiple
        accept="image/*"
        tabindex="-1"
        aria-hidden="true"
        @change="onPick"
      />
      <span v-if="items.length" class="muted small">已选 {{ items.length }} 张，一共 {{ formatBytes(totalSize) }}</span>
      <button v-if="items.length > 1 && !running" type="button" class="btn btn-ghost btn-small" @click="sortByName">按文件名排</button>
      <button v-if="items.length && !running" type="button" class="btn btn-ghost btn-small" @click="clearList">清空</button>
    </div>
    <p v-if="addProblem" class="danger-text small" role="alert">{{ addProblem }}</p>
    <p v-if="heicRejected" class="hint small">{{ HEIC_ADVICE }}</p>

    <ol v-if="items.length" class="pages" aria-label="PDF 里的页，按顺序">
      <li v-for="(item, index) in items" :key="item.id" class="page" :class="{ bad: item.note }">
        <span class="number">{{ index + 1 }}</span>
        <span class="thumb">
          <img v-if="item.thumb" :src="item.thumb" alt="" :style="{ transform: `rotate(${item.turns * 90}deg)` }" />
        </span>
        <span class="name">
          {{ item.file.name }}<span class="muted small">（{{ formatBytes(item.file.size) }}）</span>
          <span v-if="item.note" class="danger-text small">{{ item.note }}</span>
        </span>
        <span class="actions">
          <button type="button" class="btn btn-ghost btn-small" :disabled="running || index === 0" :aria-label="`第 ${index + 1} 张往前挪`" @click="move(index, -1)">上移</button>
          <button type="button" class="btn btn-ghost btn-small" :disabled="running || index === items.length - 1" :aria-label="`第 ${index + 1} 张往后挪`" @click="move(index, 1)">下移</button>
          <button type="button" class="btn btn-ghost btn-small" :disabled="running" :aria-label="`第 ${index + 1} 张顺时针转 90 度`" @click="turn(item)">转 90°</button>
          <button type="button" class="btn btn-ghost btn-small" :disabled="running" :aria-label="`去掉第 ${index + 1} 张`" @click="remove(index)">去掉</button>
        </span>
      </li>
    </ol>

    <fieldset class="group" :disabled="running">
      <span class="field-label">页面</span>
      <div class="choices">
        <label><input v-model="rules.page" type="radio" value="a4" /> A4 纸（打印、交材料用）</label>
        <label><input v-model="rules.page" type="radio" value="image" /> 和图片一样的比例，没有白边（长截图用这个）</label>
      </div>
      <template v-if="rules.page === 'a4'">
        <div class="choices">
          <span class="small">方向</span>
          <label><input v-model="rules.orientation" type="radio" value="auto" /> 横着的图片横放</label>
          <label><input v-model="rules.orientation" type="radio" value="portrait" /> 都竖着放</label>
        </div>
        <div class="choices">
          <span class="small">白边</span>
          <label><input v-model="rules.margin" type="radio" value="narrow" /> 窄（1 厘米）</label>
          <label><input v-model="rules.margin" type="radio" value="wide" /> 宽（2 厘米）</label>
          <label><input v-model="rules.margin" type="radio" value="none" /> 不留</label>
        </div>
      </template>
      <span class="field-label">清晰度</span>
      <div class="choices">
        <label><input v-model="rules.clarity" type="radio" value="high" /> 高清（适合打印，文件大）</label>
        <label><input v-model="rules.clarity" type="radio" value="standard" /> 标准</label>
        <label><input v-model="rules.clarity" type="radio" value="small" /> 小文件（方便发微信、邮件）</label>
      </div>
      <p class="muted small">拍歪了、拍横了的，先点那一张的「转 90°」转正。照片比 A4 纸小的会放大到铺满一页。</p>
    </fieldset>

    <div class="row">
      <button type="button" class="btn btn-primary btn-small" :disabled="running || !items.length" @click="save">
        生成 PDF 并保存{{ items.length ? `（${items.length} 页）` : '' }}
      </button>
      <button v-if="running && phase === 'images'" type="button" class="btn btn-ghost btn-small" :disabled="stopRequested" @click="stopRequested = true">
        {{ stopRequested ? '处理完这一张就停' : '停止' }}
      </button>
    </div>
    <p v-if="running" class="muted small" role="status">
      {{ phase === 'images' ? `正在处理第 ${current} 张（共 ${items.length} 张）…` : '已经生成好了，在「另存为」里选存到哪里…' }}
    </p>
    <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>
    <p v-if="notice" class="muted small" role="status">{{ notice }}</p>
    <template v-if="saved && !running">
      <p class="success-text small path" role="status">存好了：{{ saved.path }}（{{ saved.pages }} 页，{{ formatBytes(saved.bytes) }}）</p>
      <button v-if="isTauri()" type="button" class="btn btn-secondary btn-small self-start" @click="reveal">在文件夹里显示</button>
    </template>
  </article>
</template>

<style scoped>
.pdf-card { display: flex; flex-direction: column; gap: 9px; grid-column: 1 / -1; }
.group { display: flex; flex-direction: column; gap: 7px; border: 0; padding: 0; margin: 0; min-width: 0; }
.field-label { font-weight: 600; font-size: var(--text-small); margin-top: 4px; }
.row, .choices { display: flex; flex-wrap: wrap; gap: 8px 14px; align-items: center; }
.choices label { display: inline-flex; gap: 6px; align-items: center; font-size: var(--text-small); }
.self-start { align-self: flex-start; }
.path { overflow-wrap: anywhere; }
.drop {
  display: flex; flex-wrap: wrap; gap: 8px 12px; align-items: center;
  padding: 14px; border: 2px dashed var(--color-border-strong); border-radius: var(--radius);
  background: var(--color-surface-2);
}
.drop.over { border-color: var(--color-primary); background: var(--color-primary-soft); }
.hint { padding: 8px 11px; border-radius: var(--radius); background: var(--tone-info-bg); color: var(--tone-info-text); }
.pages {
  display: flex; flex-direction: column; list-style: none; margin: 0; padding: 0;
  max-height: 340px; overflow: auto; border: 1px solid var(--color-border-strong); border-radius: var(--radius);
}
.page {
  display: grid; grid-template-columns: 2.2em 56px minmax(0, 1fr) auto; gap: 10px; align-items: center;
  padding: 6px 9px; border-bottom: 1px solid var(--color-border-strong); font-size: var(--text-small);
}
.page:last-child { border-bottom: 0; }
.page.bad { background: var(--tone-manual-bg); }
.number { color: var(--color-text-muted); text-align: right; }
.thumb { width: 56px; height: 56px; display: flex; align-items: center; justify-content: center; overflow: hidden; background: var(--color-surface-2); border-radius: 4px; }
.thumb img { max-width: 56px; max-height: 56px; object-fit: contain; transition: transform 0.15s; }
.name { display: flex; flex-direction: column; gap: 2px; overflow-wrap: anywhere; }
.actions { display: flex; flex-wrap: wrap; gap: 4px; justify-content: flex-end; }
@media (max-width: 720px) {
  .page { grid-template-columns: 2.2em 56px minmax(0, 1fr); }
  .actions { grid-column: 1 / -1; justify-content: flex-start; }
}
</style>
