<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, shallowRef, watch } from 'vue'
import { isTauri, longImageReveal, longImageSave } from '../api'
import { errorText } from '../utils/format'
import { MAX_PIXELS, formatBytes, isHeic, HEIC_ADVICE, unsupportedReason } from '../utils/imageBatch'
import { JPEG_QUALITY, MAX_IMAGES, stitchLayout, suggestedName, type StitchLayout, type StitchRules } from '../utils/longImage'
import { byName } from '../utils/pdf'

// 长图拼接：几张截图（聊天记录、网页、订单）上下拼成一张长图，或者左右并排，方便一次发给别人。
// 图片用 WebView2 自带的解码器打开（按照片里记的方向转正），排好位置（utils/longImage.ts）画到一张画布上，
// 存成 JPG 或 PNG；存到哪里由系统的「另存为」对话框选（后端 long_image_save），原图不动。
// 刚截的图可以直接粘贴：点一下拖放框（它能拿到焦点），按 Ctrl + V。
// 预览和缩略图都用画布转成 data: 地址显示（界面的内容安全策略不放行 blob: 图片）。

// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件
defineProps<{ hideTitle?: boolean }>()

interface Item {
  id: number
  file: File
  /** 转正以后的尺寸，还没读出来是 0 */
  width: number
  height: number
  thumb: string
  /** 这张打不开时的原因 */
  note: string
}

interface Built {
  key: string
  canvas: HTMLCanvasElement
  bytes: Uint8Array
  layout: StitchLayout
  preview: string
  format: StitchRules['format']
}

const THUMB_EDGE = 96
/** 预览最宽多少像素、最高多少像素（按比例缩小） */
const PREVIEW_WIDTH = 480
const PREVIEW_HEIGHT = 4000

const items = ref<Item[]>([])
let nextId = 0
let pasted = 0
const rules = reactive<StitchRules>({ direction: 'vertical', size: 'narrowest', gap: false, format: 'jpeg' })
const running = ref(false)
const stopRequested = ref(false)
const current = ref(0)
const error = ref('')
const addProblem = ref('')
const notice = ref('')
const heicRejected = ref(false)
/** 拼好的那张（画布留着，复制到剪贴板时要用） */
const built = shallowRef<Built | null>(null)
const saving = ref(false)
const saved = ref('')
const copyState = ref<'' | 'ok' | 'fail'>('')
const dragDepth = ref(0)
const picker = ref<HTMLInputElement | null>(null)

const totalSize = computed(() => items.value.reduce((sum, i) => sum + i.file.size, 0))
const vertical = computed(() => rules.direction === 'vertical')

function buildKey(): string {
  return JSON.stringify({ rules, items: items.value.map((i) => i.id) })
}

/** 列表或者设置变了，拼好的那张就不对了 */
function release(): void {
  const b = built.value
  if (b) {
    b.canvas.width = 0
    b.canvas.height = 0
  }
  built.value = null
  saved.value = ''
  copyState.value = ''
}
watch(buildKey, (key) => {
  if (built.value && built.value.key !== key) release()
})
onBeforeUnmount(release)

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
    if (items.value.length >= MAX_IMAGES) {
      dropped++
      continue
    }
    known.add(key)
    items.value.push({ id: nextId++, file, width: 0, height: 0, thumb: '', note: '' })
  }
  const problems: string[] = []
  if (rejected.length) {
    const names = rejected.slice(0, 3).map((n) => `「${n}」`).join('、')
    problems.push(`${names}${rejected.length > 3 ? ` 等 ${rejected.length} 个文件` : ''}打不开，没有加进来（HEIC、TIFF、SVG 和不是图片的文件都不行）。`)
  }
  if (dropped) problems.push(`一次最多拼 ${MAX_IMAGES} 张，多出来的 ${dropped} 张没有加进来。`)
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

/** 粘贴进来的截图都叫 image.png：换成「截图 1.png」这样的名字，列表里分得清 */
function onPaste(event: ClipboardEvent): void {
  const files = Array.from(event.clipboardData?.files ?? []).filter((f) => f.type.startsWith('image/'))
  if (!files.length) {
    if (event.clipboardData?.types.length) addProblem.value = '剪贴板里没有图片：先截图（或者在图片上点右键「复制图片」），再按 Ctrl + V。'
    return
  }
  event.preventDefault()
  const now = Date.now()
  addFiles(
    files.map((f, i) => {
      pasted++
      const ext = f.type === 'image/jpeg' ? 'jpg' : (f.type.split('/')[1] ?? 'png')
      return new File([f], `截图 ${pasted}.${ext}`, { type: f.type, lastModified: now + i })
    }),
  )
}

function openImage(file: File): Promise<ImageBitmap> {
  return createImageBitmap(file, { imageOrientation: 'from-image' }).catch(() => createImageBitmap(file))
}

/** 一张一张读尺寸、做缩略图（一次全解码，几十张大图会卡住界面）。正在做的时候再叫它，等的是同一个 */
let thumbTask: Promise<void> | null = null
function makeThumbs(): Promise<void> {
  thumbTask ??= runThumbs().finally(() => {
    thumbTask = null
  })
  return thumbTask
}

async function runThumbs(): Promise<void> {
  const canvas = document.createElement('canvas')
  try {
    for (;;) {
      const item = items.value.find((i) => !i.thumb && !i.note)
      if (!item) break
      try {
        const bitmap = await openImage(item.file)
        try {
          if (bitmap.width * bitmap.height > MAX_PIXELS) {
            item.note = `这张图片有 ${Math.round((bitmap.width * bitmap.height) / 1_000_000)} 百万像素，太大了，拼不了。`
            continue
          }
          item.width = bitmap.width
          item.height = bitmap.height
          const scale = Math.min(1, THUMB_EDGE / Math.max(bitmap.width, bitmap.height))
          canvas.width = Math.max(1, Math.round(bitmap.width * scale))
          canvas.height = Math.max(1, Math.round(bitmap.height * scale))
          const context = canvas.getContext('2d')
          if (!context) throw new Error('no canvas')
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
  }
}

function move(index: number, by: number): void {
  const list = items.value
  const to = index + by
  if (running.value || to < 0 || to >= list.length) return
  const [item] = list.splice(index, 1)
  if (item) list.splice(to, 0, item)
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
  release()
}

function encode(canvas: HTMLCanvasElement, type: string, quality?: number): Promise<Blob> {
  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error('没能生成图片：拼出来太大了，少拼几张再试。'))), type, quality)
  })
}

async function build(): Promise<void> {
  if (running.value || !items.value.length) return
  running.value = true
  stopRequested.value = false
  current.value = 0
  error.value = ''
  notice.value = ''
  release()
  const key = buildKey()
  const canvas = document.createElement('canvas')
  let done = false
  try {
    // 缩略图还没做完的等它做完（尺寸是那时读出来的）
    await makeThumbs()
    const bad = items.value.findIndex((i) => i.note || !i.width)
    if (bad >= 0) {
      const item = items.value[bad]
      error.value = `第 ${bad + 1} 张「${item?.file.name ?? ''}」：${item?.note || '这张图片打不开。'}把它去掉再拼。`
      return
    }
    const layout = stitchLayout(items.value.map((i) => ({ width: i.width, height: i.height })), rules)
    canvas.width = layout.width
    canvas.height = layout.height
    const context = canvas.getContext('2d')
    if (!context) throw new Error('这台电脑上建不了这么大的画布：少拼几张再试。')
    context.imageSmoothingEnabled = true
    context.imageSmoothingQuality = 'high'
    // 缝、留白和透明的地方都是白色（JPG 没有透明，不填会变成黑色）
    context.fillStyle = '#ffffff'
    context.fillRect(0, 0, layout.width, layout.height)
    for (const [index, item] of items.value.entries()) {
      if (stopRequested.value) {
        notice.value = '已经停下，没有拼。'
        return
      }
      current.value = index + 1
      const place = layout.placements[index]
      if (!place) continue
      let bitmap: ImageBitmap
      try {
        bitmap = await openImage(item.file)
      } catch {
        item.note = '这张图片打不开：文件可能坏了，或者不是它的扩展名说的那种格式。'
        error.value = `第 ${index + 1} 张「${item.file.name}」打不开，把它去掉再拼。`
        return
      }
      try {
        context.drawImage(bitmap, place.x, place.y, place.width, place.height)
      } finally {
        bitmap.close()
      }
    }
    const type = rules.format === 'png' ? 'image/png' : 'image/jpeg'
    const blob = await encode(canvas, type, rules.format === 'png' ? undefined : JPEG_QUALITY)
    if (blob.type !== type) throw new Error(`这台电脑存不了 ${rules.format === 'png' ? 'PNG' : 'JPG'} 格式，换另一种试试。`)
    const bytes = new Uint8Array(await blob.arrayBuffer())
    built.value = { key, canvas, bytes, layout, preview: makePreview(canvas), format: rules.format }
    done = true
  } catch (e) {
    error.value = errorText(e)
  } finally {
    if (!done) {
      canvas.width = 0
      canvas.height = 0
    }
    running.value = false
  }
}

/** 缩小的预览（data: 地址） */
function makePreview(source: HTMLCanvasElement): string {
  const scale = Math.min(1, PREVIEW_WIDTH / source.width, PREVIEW_HEIGHT / source.height)
  const small = document.createElement('canvas')
  small.width = Math.max(1, Math.round(source.width * scale))
  small.height = Math.max(1, Math.round(source.height * scale))
  try {
    const context = small.getContext('2d')
    if (!context) return ''
    context.imageSmoothingEnabled = true
    context.imageSmoothingQuality = 'high'
    context.drawImage(source, 0, 0, small.width, small.height)
    return small.toDataURL('image/jpeg', 0.85)
  } finally {
    small.width = 0
    small.height = 0
  }
}

async function save(): Promise<void> {
  const b = built.value
  if (!b || saving.value) return
  saving.value = true
  error.value = ''
  notice.value = ''
  try {
    const path = await longImageSave(suggestedName(new Date(), b.format), b.bytes)
    if (path === null) {
      notice.value = '没有保存（在「另存为」里点了取消）。'
      return
    }
    saved.value = path
  } catch (e) {
    error.value = errorText(e)
  } finally {
    saving.value = false
  }
}

/** 复制到剪贴板（PNG：剪贴板只认这一种图片），在微信、QQ 的聊天框里按 Ctrl + V 就能发 */
async function copy(): Promise<void> {
  const b = built.value
  if (!b) return
  copyState.value = ''
  try {
    // 在点击的这一刻就把剪贴板的内容交出去（图片随后才编码好），免得等编码完「点击」已经过期
    await navigator.clipboard.write([new ClipboardItem({ 'image/png': encode(b.canvas, 'image/png') })])
    copyState.value = 'ok'
  } catch {
    copyState.value = 'fail'
  }
}

async function reveal(): Promise<void> {
  try {
    await longImageReveal()
  } catch (e) {
    error.value = errorText(e)
  }
}
</script>

<template>
  <article class="card long-card">
    <h3 :class="hideTitle ? 'visually-hidden' : 'section-title'">长图拼接</h3>
    <p class="muted small">
      把几张截图（聊天记录、网页、订单）上下拼成一张长图，或者左右并排拼在一起，一次发给别人。原图不动；拼好先看一眼，再存到电脑上，或者复制了直接粘贴到微信、QQ 里。
    </p>
    <p v-if="!isTauri()" class="muted small">浏览器里能真的拼、能复制，只是不会真的保存文件；在 Windows 的小药箱里才会弹出「另存为」。</p>

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
      <span class="muted small paste-tip">刚截了图的（Win + Shift + S、微信、QQ 截图）：点一下这个框，按 Ctrl + V 粘贴进来。</span>
      <span v-if="items.length" class="muted small">已选 {{ items.length }} 张，一共 {{ formatBytes(totalSize) }}</span>
      <button v-if="items.length > 1 && !running" type="button" class="btn btn-ghost btn-small" @click="sortByName">按文件名排</button>
      <button v-if="items.length && !running" type="button" class="btn btn-ghost btn-small" @click="clearList">清空</button>
    </div>
    <p v-if="addProblem" class="danger-text small" role="alert">{{ addProblem }}</p>
    <p v-if="heicRejected" class="hint small">{{ HEIC_ADVICE }}</p>

    <ol v-if="items.length" class="list" aria-label="要拼的图片，按顺序">
      <li v-for="(item, index) in items" :key="item.id" class="entry" :class="{ bad: item.note }">
        <span class="number">{{ index + 1 }}</span>
        <span class="thumb"><img v-if="item.thumb" :src="item.thumb" alt="" /></span>
        <span class="name">
          {{ item.file.name }}<span class="muted small">（<template v-if="item.width">{{ item.width }} × {{ item.height }}，</template>{{ formatBytes(item.file.size) }}）</span>
          <span v-if="item.note" class="danger-text small">{{ item.note }}</span>
        </span>
        <span class="actions">
          <button type="button" class="btn btn-ghost btn-small" :disabled="running || index === 0" :aria-label="`第 ${index + 1} 张往前挪`" @click="move(index, -1)">{{ vertical ? '上移' : '左移' }}</button>
          <button type="button" class="btn btn-ghost btn-small" :disabled="running || index === items.length - 1" :aria-label="`第 ${index + 1} 张往后挪`" @click="move(index, 1)">{{ vertical ? '下移' : '右移' }}</button>
          <button type="button" class="btn btn-ghost btn-small" :disabled="running" :aria-label="`去掉第 ${index + 1} 张`" @click="remove(index)">去掉</button>
        </span>
      </li>
    </ol>

    <fieldset class="group" :disabled="running">
      <span class="field-label">怎么拼</span>
      <div class="choices">
        <label><input v-model="rules.direction" type="radio" value="vertical" /> 上下拼（拼成一张长图）</label>
        <label><input v-model="rules.direction" type="radio" value="horizontal" /> 左右并排</label>
      </div>
      <span class="field-label">大小</span>
      <div class="choices">
        <label><input v-model="rules.size" type="radio" value="narrowest" /> 都缩成一样{{ vertical ? '宽' : '高' }}（按最{{ vertical ? '窄' : '矮' }}的那张）</label>
        <label><input v-model="rules.size" type="radio" value="widest" /> 都放成一样{{ vertical ? '宽' : '高' }}（按最{{ vertical ? '宽' : '高' }}的那张，小图会变糊）</label>
        <label><input v-model="rules.size" type="radio" value="keep" /> 原样不缩放（{{ vertical ? '窄的居中，两边留白' : '矮的居中，上下留白' }}）</label>
      </div>
      <div class="choices">
        <label><input v-model="rules.gap" type="checkbox" /> 每张之间留一道白缝</label>
      </div>
      <span class="field-label">存成</span>
      <div class="choices">
        <label><input v-model="rules.format" type="radio" value="jpeg" /> JPG（文件小，发微信、QQ 用）</label>
        <label><input v-model="rules.format" type="radio" value="png" /> PNG（字最清楚，文件大）</label>
      </div>
    </fieldset>

    <div class="row">
      <button type="button" class="btn btn-primary btn-small" :disabled="running || !items.length" @click="build">
        拼成{{ vertical ? '长图' : '一张' }}{{ items.length ? `（${items.length} 张）` : '' }}
      </button>
      <button v-if="running" type="button" class="btn btn-ghost btn-small" :disabled="stopRequested" @click="stopRequested = true">
        {{ stopRequested ? '画完这一张就停' : '停止' }}
      </button>
    </div>
    <p v-if="running" class="muted small" role="status">正在拼第 {{ current || 1 }} 张（共 {{ items.length }} 张）…</p>
    <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>
    <p v-if="notice" class="muted small" role="status">{{ notice }}</p>

    <template v-if="built && !running">
      <p class="small" role="status">
        拼好了：{{ built.layout.width }} × {{ built.layout.height }} 像素，{{ built.format === 'png' ? 'PNG' : 'JPG' }}，{{ formatBytes(built.bytes.length) }}。<template v-if="built.layout.scale < 1">拼出来太大了，整张按比例缩小到了原来的 {{ Math.round(built.layout.scale * 100) }}%。</template>
      </p>
      <div v-if="built.preview" class="preview"><img :src="built.preview" alt="拼好的图片的预览" /></div>
      <div class="row">
        <button type="button" class="btn btn-primary btn-small" :disabled="saving" @click="save">{{ saving ? '正在保存…' : '保存到电脑' }}</button>
        <button type="button" class="btn btn-secondary btn-small" @click="copy">复制图片</button>
        <button v-if="saved && isTauri()" type="button" class="btn btn-ghost btn-small" @click="reveal">在文件夹里显示</button>
      </div>
      <p v-if="copyState === 'ok'" class="success-text small" role="status">复制好了：在微信、QQ 的聊天框里按 Ctrl + V 就能发出去。</p>
      <p v-else-if="copyState === 'fail'" class="danger-text small" role="alert">没能复制到剪贴板，点「保存到电脑」存下来再发吧。</p>
      <p v-if="saved" class="success-text small path" role="status">存好了：{{ saved }}</p>
    </template>
  </article>
</template>

<style scoped>
.long-card { display: flex; flex-direction: column; gap: 9px; grid-column: 1 / -1; }
.group { display: flex; flex-direction: column; gap: 7px; border: 0; padding: 0; margin: 0; min-width: 0; }
.field-label { font-weight: 600; font-size: var(--text-small); margin-top: 4px; }
.row, .choices { display: flex; flex-wrap: wrap; gap: 8px 14px; align-items: center; }
.choices label { display: inline-flex; gap: 6px; align-items: center; font-size: var(--text-small); }
.path { overflow-wrap: anywhere; }
.drop {
  display: flex; flex-wrap: wrap; gap: 8px 12px; align-items: center;
  padding: 14px; border: 2px dashed var(--color-border-strong); border-radius: var(--radius);
  background: var(--color-surface-2);
}
.drop:focus-visible, .drop:focus { outline: none; border-color: var(--color-primary); }
.drop.over { border-color: var(--color-primary); background: var(--color-primary-soft); }
.paste-tip { flex-basis: 100%; }
.hint { padding: 8px 11px; border-radius: var(--radius); background: var(--tone-info-bg); color: var(--tone-info-text); }
.list {
  display: flex; flex-direction: column; list-style: none; margin: 0; padding: 0;
  max-height: 340px; overflow: auto; border: 1px solid var(--color-border-strong); border-radius: var(--radius);
}
.entry {
  display: grid; grid-template-columns: 2.2em 56px minmax(0, 1fr) auto; gap: 10px; align-items: center;
  padding: 6px 9px; border-bottom: 1px solid var(--color-border-strong); font-size: var(--text-small);
}
.entry:last-child { border-bottom: 0; }
.entry.bad { background: var(--tone-manual-bg); }
.number { color: var(--color-text-muted); text-align: right; }
.thumb { width: 56px; height: 56px; display: flex; align-items: center; justify-content: center; overflow: hidden; background: var(--color-surface-2); border-radius: 4px; }
.thumb img { max-width: 56px; max-height: 56px; object-fit: contain; }
.name { display: flex; flex-direction: column; gap: 2px; overflow-wrap: anywhere; }
.actions { display: flex; flex-wrap: wrap; gap: 4px; justify-content: flex-end; }
.preview {
  align-self: flex-start; max-width: 100%; max-height: 420px; overflow: auto;
  border: 1px solid var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface-2);
}
.preview img { display: block; max-width: min(100%, 480px); height: auto; }
@media (max-width: 720px) {
  .entry { grid-template-columns: 2.2em 56px minmax(0, 1fr); }
  .actions { grid-column: 1 / -1; justify-content: flex-start; }
}
</style>
