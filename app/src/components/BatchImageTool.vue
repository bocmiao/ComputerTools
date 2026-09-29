<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { imageOpenFolder, imageSave, imageSelectFolder, isTauri } from '../api'
import { errorText } from '../utils/format'
import {
  HEIC_ADVICE,
  ID_PHOTO_SIZES,
  MAX_FILES,
  MAX_PIXELS,
  drawPlan,
  fitToSize,
  formatBytes,
  formatLabel,
  isHeic,
  keepsSize,
  matchesFormat,
  outputMime,
  outputName,
  sameFormat,
  suffixProblem,
  unsupportedReason,
  watermarkPlan,
  WATERMARK_MAX,
  WATERMARK_OPACITIES,
  type Encoder,
  type Fitted,
  type ImageRules,
  type OutputMime,
} from '../utils/imageBatch'

// 图片批量压缩、转格式、改尺寸、加水印。图片在这里用 WebView2 自带的解码器打开、画到画布上、重新编码；
// 保存交给后端：只能存进用系统对话框选的文件夹，只新建、不覆盖，所以原图不会被改动。
// 重新编码以后，拍摄时间、地点、设备这些信息都不会带过去（「存原图」的那几张除外）。
// 水印：文字斜着铺满整张图（交身份证复印件时写上用途）；加了水印就不存原图（原图上没有水印）。

// 工具箱的单个工具页页头已经写着名字：hideTitle 时标题只留给读屏软件
defineProps<{ hideTitle?: boolean }>()

type ItemStatus = 'waiting' | 'working' | 'done' | 'skipped' | 'failed'
interface Item {
  id: number
  file: File
  status: ItemStatus
  savedAs: string
  after: number
  note: string
}

const items = ref<Item[]>([])
let nextId = 0
const folder = ref('')
const rules = reactive<ImageRules>({
  format: 'keep',
  quality: 80,
  resize: 'none',
  longEdge: 1920,
  width: ID_PHOTO_SIZES[0].width,
  height: ID_PHOTO_SIZES[0].height,
  maxKb: 200,
  suffix: '',
  keepSmaller: true,
  watermark: '',
  watermarkOpacity: WATERMARK_OPACITIES[1].value,
})
const watermarkText = computed(() => rules.watermark.trim())
const limitSize = ref(false)
/** 固定尺寸选的是哪一个：证件照尺寸的序号，或者 custom */
const fixedPreset = ref<string>('0')
const busy = ref(false)
const running = ref(false)
const stopRequested = ref(false)
const current = ref(0)
const error = ref('')
const notice = ref('')
/** 拖着文件经过框里的子元素时也会收到 dragleave，所以数进出的次数 */
const dragDepth = ref(0)
const finished = ref(false)
const picker = ref<HTMLInputElement | null>(null)

const LONG_EDGES = [3840, 2560, 1920, 1280, 1080, 800] as const

const totalBefore = computed(() => items.value.reduce((sum, i) => sum + i.file.size, 0))
const done = computed(() => items.value.filter((i) => i.status === 'done'))
const notDone = computed(() => items.value.filter((i) => i.status === 'skipped' || i.status === 'failed'))
/** 有 HEIC 照片没处理 */
const heicSkipped = computed(() => items.value.some((i) => i.status === 'skipped' && isHeic(i.file.name, i.file.type)))
const doneBefore = computed(() => done.value.reduce((sum, i) => sum + i.file.size, 0))
const doneAfter = computed(() => done.value.reduce((sum, i) => sum + i.after, 0))
const change = computed(() => {
  if (!doneBefore.value) return ''
  const pct = Math.round((1 - doneAfter.value / doneBefore.value) * 100)
  return pct > 0 ? `小了 ${pct}%` : pct < 0 ? `大了 ${-pct}%` : '差不多'
})
/** 有没有可能存成 JPG 或 WebP（质量滑块才有用） */
const mayBeLossy = computed(() => rules.format !== 'png')
const problem = computed(() => {
  const suffix = suffixProblem(rules.suffix)
  if (suffix) return suffix
  if (limitSize.value && !(rules.maxKb >= 10 && rules.maxKb <= 100_000)) return '「压到多少 KB 以内」要在 10 到 100000 之间。'
  if (rules.resize === 'fixed' && !(rules.width >= 16 && rules.width <= 10_000 && rules.height >= 16 && rules.height <= 10_000)) {
    return '固定尺寸的宽和高要在 16 到 10000 像素之间。'
  }
  return ''
})

function statusText(i: Item): string {
  switch (i.status) {
    case 'waiting':
      return '等待处理'
    case 'working':
      return '正在处理…'
    case 'done':
      return `存成「${i.savedAs}」，${formatBytes(i.file.size)} → ${formatBytes(i.after)}${i.note ? `（${i.note}）` : ''}`
    default:
      return i.note
  }
}

function addFiles(list: FileList | File[] | null | undefined): void {
  if (!list || running.value) return
  error.value = ''
  finished.value = false
  const known = new Set(items.value.map((i) => `${i.file.name}|${i.file.size}|${i.file.lastModified}`))
  let dropped = 0
  for (const file of Array.from(list)) {
    const key = `${file.name}|${file.size}|${file.lastModified}`
    if (known.has(key)) continue
    if (items.value.length >= MAX_FILES) {
      dropped++
      continue
    }
    known.add(key)
    items.value.push({ id: nextId++, file, status: 'waiting', savedAs: '', after: 0, note: '' })
  }
  if (dropped) error.value = `一次最多处理 ${MAX_FILES} 张，多出来的 ${dropped} 张没有加进来。`
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

function choosePreset(): void {
  const preset = ID_PHOTO_SIZES[Number(fixedPreset.value)]
  if (preset) Object.assign(rules, { width: preset.width, height: preset.height })
}

function clearList(): void {
  if (running.value) return
  items.value = []
  error.value = ''
  notice.value = ''
  finished.value = false
}

async function selectFolder(): Promise<void> {
  if (busy.value) return
  busy.value = true
  error.value = ''
  try {
    const selected = await imageSelectFolder()
    if (selected) folder.value = selected
  } catch (e) {
    error.value = errorText(e)
  } finally {
    busy.value = false
  }
}

async function openFolder(): Promise<void> {
  try {
    await imageOpenFolder()
  } catch (e) {
    error.value = errorText(e)
  }
}

function toBlob(canvas: HTMLCanvasElement, type: OutputMime, quality: number): Promise<Blob> {
  return new Promise((resolve, reject) => {
    canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error('没能生成图片（图片可能太大了）。'))), type, quality)
  })
}

const WATERMARK_FONT = (size: number) => `${size}px "Microsoft YaHei", "PingFang SC", sans-serif`

/** 斜着铺满整张图的水印文字（灰色，半透明，深浅图上都看得见） */
function drawWatermark(context: CanvasRenderingContext2D, width: number, height: number, text: string, opacity: number): void {
  context.save()
  const plan = watermarkPlan(width, height, (size) => {
    context.font = WATERMARK_FONT(size)
    return context.measureText(text).width
  })
  context.font = WATERMARK_FONT(plan.fontSize)
  context.fillStyle = `rgba(128, 128, 128, ${opacity})`
  context.textAlign = 'center'
  context.textBaseline = 'middle'
  context.translate(width / 2, height / 2)
  context.rotate(plan.angle)
  for (let y = -plan.reach, row = 0; y <= plan.reach; y += plan.stepY, row++) {
    const offset = row % 2 ? plan.stepX / 2 : 0
    for (let x = -plan.reach - offset; x <= plan.reach + plan.stepX; x += plan.stepX) context.fillText(text, x, y)
  }
  context.restore()
}

/** 处理一张：返回要存的内容和说明；处理不了时抛出一句给用户看的话 */
async function processOne(file: File, canvas: HTMLCanvasElement): Promise<{ bytes: Uint8Array; mime: OutputMime; note: string }> {
  const reason = unsupportedReason(file.name, file.type)
  if (reason) throw new Error(reason)
  let bitmap: ImageBitmap
  try {
    // 按照片里记的方向转正（手机竖着拍的照片常见）；很老的 WebView2 不认这个参数，就不带参数再试一次
    bitmap = await createImageBitmap(file, { imageOrientation: 'from-image' }).catch(() => createImageBitmap(file))
  } catch {
    throw new Error('这张图片打不开：文件可能坏了，或者不是它的扩展名说的那种格式。')
  }
  try {
    if (bitmap.width * bitmap.height > MAX_PIXELS) {
      throw new Error(`这张图片有 ${Math.round((bitmap.width * bitmap.height) / 1_000_000)} 百万像素，太大了，处理不了。`)
    }
    const mime = outputMime(file.type, rules.format)
    const plan = drawPlan(bitmap.width, bitmap.height, rules)
    const context = canvas.getContext('2d')
    if (!context) throw new Error('这台电脑上建不了处理图片用的画布。')
    const encode: Encoder<Blob> = async (quality, scale) => {
      const width = Math.max(1, Math.round(plan.width * scale))
      const height = Math.max(1, Math.round(plan.height * scale))
      canvas.width = width
      canvas.height = height
      // 改了画布大小，画笔的设置会重置，所以每次都设
      context.imageSmoothingEnabled = true
      context.imageSmoothingQuality = 'high'
      if (mime === 'image/jpeg') {
        // JPG 没有透明，透明的地方填白色（不填会变成黑色）
        context.fillStyle = '#ffffff'
        context.fillRect(0, 0, width, height)
      }
      context.drawImage(bitmap, plan.sx, plan.sy, plan.sw, plan.sh, 0, 0, width, height)
      if (watermarkText.value) drawWatermark(context, width, height, watermarkText.value, rules.watermarkOpacity)
      const blob = await toBlob(canvas, mime, quality)
      if (blob.type !== mime) throw new Error(`这台电脑存不了 ${formatLabel(mime)} 格式。`)
      return { size: blob.size, data: blob }
    }
    const quality = rules.quality / 100
    const lossy = mime !== 'image/png'
    let out: Fitted<Blob>
    if (limitSize.value) {
      const fitted = await fitToSize(encode, rules.maxKb * 1024, quality, lossy, plan.width, plan.height)
      if (!fitted) throw new Error(`缩到最小也压不到 ${rules.maxKb} KB 以内。`)
      out = fitted
    } else {
      out = { ...(await encode(quality, 1)), quality, scale: 1 }
    }
    const notes: string[] = []
    if (out.scale < 1) notes.push(`为了压到 ${rules.maxKb} KB 以内，缩小成了 ${canvas.width} × ${canvas.height}`)
    else if (lossy && limitSize.value && out.quality < quality - 0.001) notes.push(`质量降到了 ${Math.round(out.quality * 100)}`)
    // 格式、尺寸都不变，处理完反而更大：存原图（原图的开头要真的是这种格式）。限了大小时也一样：
    // 原图比处理完的还小，自然也在限制以内
    if (rules.keepSmaller && !watermarkText.value && out.size >= file.size && sameFormat(file.type, mime) && keepsSize(plan, bitmap.width, bitmap.height)) {
      const original = new Uint8Array(await file.arrayBuffer())
      if (matchesFormat(original.subarray(0, 12), mime)) {
        return { bytes: original, mime, note: '处理完反而更大，存的是原图，原图带的拍摄信息也留着' }
      }
    }
    if (file.type === 'image/gif') notes.push('动图只留了第一帧')
    return { bytes: new Uint8Array(await out.data.arrayBuffer()), mime, note: notes.join('；') }
  } finally {
    bitmap.close()
  }
}

async function start(): Promise<void> {
  if (running.value || !items.value.length || !folder.value || problem.value) return
  running.value = true
  stopRequested.value = false
  finished.value = false
  error.value = ''
  notice.value = ''
  for (const i of items.value) Object.assign(i, { status: 'waiting', savedAs: '', after: 0, note: '' })
  const canvas = document.createElement('canvas')
  try {
    for (const [index, item] of items.value.entries()) {
      if (stopRequested.value) break
      current.value = index + 1
      item.status = 'working'
      let result
      try {
        result = await processOne(item.file, canvas)
      } catch (e) {
        Object.assign(item, { status: 'skipped', note: errorText(e) })
        continue
      }
      try {
        const saved = await imageSave(outputName(item.file.name, result.mime, rules.suffix), item.file.lastModified, result.bytes)
        Object.assign(item, { status: 'done', savedAs: saved, after: result.bytes.length, note: result.note })
      } catch (e) {
        Object.assign(item, { status: 'failed', note: `没存上：${errorText(e)}` })
      }
    }
  } finally {
    canvas.width = 0
    canvas.height = 0
    running.value = false
    finished.value = true
    const left = items.value.filter((i) => i.status === 'waiting' || i.status === 'working')
    for (const i of left) Object.assign(i, { status: 'waiting', note: '' })
    if (stopRequested.value && left.length) notice.value = `已经停下，还有 ${left.length} 张没处理。`
  }
}
</script>

<template>
  <article class="card image-card">
    <h3 :class="hideTitle ? 'visually-hidden' : 'section-title'">图片批量压缩、转格式、改尺寸、加水印</h3>
    <p class="muted small">
      选一些图片（也可以直接拖到下面的框里），按设置处理，存进你选的文件夹。原图不动，也不会覆盖任何已有的文件（重名的在名字后面加「 (2)」）。处理后的图片不带拍摄时间、地点、设备这些信息。
    </p>
    <p v-if="!isTauri()" class="muted small">浏览器里只演示处理过程，不会真的保存文件；在 Windows 的小药箱里才会存进文件夹。</p>

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
        accept="image/*,.heic,.heif"
        tabindex="-1"
        aria-hidden="true"
        @change="onPick"
      />
      <span v-if="items.length" class="muted small">已选 {{ items.length }} 张，一共 {{ formatBytes(totalBefore) }}</span>
      <button v-if="items.length && !running" type="button" class="btn btn-ghost btn-small" @click="clearList">清空</button>
    </div>

    <fieldset class="group" :disabled="running">
      <span class="field-label">存成什么格式</span>
      <div class="choices">
        <label><input v-model="rules.format" type="radio" value="keep" /> 格式不变</label>
        <label><input v-model="rules.format" type="radio" value="jpeg" /> JPG（照片最常用）</label>
        <label><input v-model="rules.format" type="radio" value="png" /> PNG（不损失画质，文件大）</label>
        <label><input v-model="rules.format" type="radio" value="webp" /> WebP（更小，老软件可能打不开）</label>
      </div>
      <p class="muted small">「格式不变」时，GIF、BMP 这些存成 PNG；GIF 动图只留第一帧。HEIC（苹果手机的照片）和 TIFF 打不开。</p>

      <template v-if="mayBeLossy">
        <label class="field-label" for="image-quality">画质：{{ rules.quality }}</label>
        <input id="image-quality" v-model.number="rules.quality" class="range" type="range" min="40" max="100" step="5" />
        <p class="muted small">数字越小文件越小；80 左右肉眼一般看不出区别。PNG 不用调。</p>
      </template>

      <span class="field-label">尺寸</span>
      <div class="choices column">
        <label><input v-model="rules.resize" type="radio" value="none" /> 不改</label>
        <label class="inline">
          <input v-model="rules.resize" type="radio" value="long-edge" /> 长边不超过
          <select v-model.number="rules.longEdge" class="input small-input" aria-label="长边不超过多少像素" :disabled="rules.resize !== 'long-edge'">
            <option v-for="n in LONG_EDGES" :key="n" :value="n">{{ n }} 像素</option>
          </select>
          （比这小的不放大）
        </label>
        <label class="inline">
          <input v-model="rules.resize" type="radio" value="fixed" /> 固定尺寸，居中裁剪
          <select
            v-model="fixedPreset"
            class="input small-input wide-select"
            aria-label="固定尺寸"
            :disabled="rules.resize !== 'fixed'"
            @change="choosePreset"
          >
            <option v-for="(s, i) in ID_PHOTO_SIZES" :key="s.label" :value="String(i)">{{ s.label }}</option>
            <option value="custom">自己填</option>
          </select>
        </label>
        <div v-if="rules.resize === 'fixed' && fixedPreset === 'custom'" class="pair">
          <label class="inline">宽 <input v-model.number="rules.width" class="input small-input" type="number" min="16" max="10000" aria-label="宽（像素）" /></label>
          <label class="inline">高 <input v-model.number="rules.height" class="input small-input" type="number" min="16" max="10000" aria-label="高（像素）" /></label>
          <span class="muted small">像素</span>
        </div>
      </div>

      <label class="check">
        <input v-model="limitSize" type="checkbox" /> 每张压到
        <input v-model.number="rules.maxKb" class="input small-input" type="number" min="10" max="100000" aria-label="最多多少 KB" :disabled="!limitSize" />
        KB 以内（报名、上传照片常要求 200 KB 以内）
      </label>
      <p v-if="limitSize" class="muted small">先降画质，还不够就缩小尺寸，直到放得下。PNG 只能靠缩小尺寸。</p>

      <label class="field-label" for="image-watermark">加水印（可以不填）</label>
      <input
        id="image-watermark"
        v-model="rules.watermark"
        class="input"
        type="text"
        :maxlength="WATERMARK_MAX"
        placeholder="例如：仅用于办理 XX 业务，他用无效"
      />
      <template v-if="watermarkText">
        <div class="choices">
          <span class="small">深浅</span>
          <label v-for="o in WATERMARK_OPACITIES" :key="o.value"><input v-model.number="rules.watermarkOpacity" type="radio" :value="o.value" /> {{ o.label }}</label>
        </div>
        <p class="muted small">文字斜着铺满整张图。交身份证、户口本、银行卡的照片或复印件时写清楚用途和日期，别人拿去也没法挪作他用。</p>
      </template>

      <label class="check">
        新文件名后面加
        <input v-model="rules.suffix" class="input small-input wide-select" type="text" maxlength="40" placeholder="可以不填，例如 _压缩" aria-label="新文件名后面加的字" />
      </label>
      <label class="check">
        <input v-model="rules.keepSmaller" type="checkbox" :disabled="!!watermarkText" /> 格式、尺寸都没变而压完反而更大的，存原图<template v-if="watermarkText">（加了水印时不存原图）</template>
      </label>
    </fieldset>

    <div class="row">
      <button type="button" class="btn btn-secondary btn-small" :disabled="busy || running" @click="selectFolder">选择保存到哪个文件夹</button>
      <span v-if="folder" class="muted small path">{{ folder }}</span>
    </div>

    <p v-if="problem" class="danger-text small" role="alert">{{ problem }}</p>
    <div class="row">
      <button type="button" class="btn btn-primary btn-small" :disabled="running || !items.length || !folder || !!problem" @click="start">
        开始处理{{ items.length ? `（${items.length} 张）` : '' }}
      </button>
      <button v-if="running" type="button" class="btn btn-ghost btn-small" :disabled="stopRequested" @click="stopRequested = true">
        {{ stopRequested ? '处理完这一张就停' : '停止' }}
      </button>
      <span v-if="!folder && items.length" class="muted small">先选好保存到哪个文件夹。</span>
    </div>
    <p v-if="running" class="muted small" role="status">正在处理第 {{ current }} 张（共 {{ items.length }} 张）…</p>
    <p v-if="error" class="danger-text small" role="alert">{{ error }}</p>
    <p v-if="notice" class="muted small" role="status">{{ notice }}</p>

    <template v-if="finished && !running">
      <p v-if="done.length" class="success-text small" role="status">
        处理好 {{ done.length }} 张：原来一共 {{ formatBytes(doneBefore) }}，现在 {{ formatBytes(doneAfter) }}（{{ change }}）。<template v-if="notDone.length">{{ notDone.length }} 张没处理，原因见下面。</template>
      </p>
      <p v-else-if="notDone.length" class="danger-text small" role="status">一张都没处理成，原因见下面。</p>
      <button v-if="done.length && isTauri()" type="button" class="btn btn-secondary btn-small self-start" @click="openFolder">打开保存的文件夹</button>
    </template>

    <p v-if="heicSkipped" class="hint small">{{ HEIC_ADVICE }}</p>
    <div v-if="items.length" class="list">
      <table>
        <thead><tr><th scope="col">图片</th><th scope="col">结果</th></tr></thead>
        <tbody>
          <tr v-for="item in items" :key="item.id" :class="item.status">
            <td>{{ item.file.name }}<span class="muted small">（{{ formatBytes(item.file.size) }}）</span></td>
            <td>{{ statusText(item) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </article>
</template>

<style scoped>
.image-card { display: flex; flex-direction: column; gap: 9px; grid-column: 1 / -1; }
.group { display: flex; flex-direction: column; gap: 7px; border: 0; padding: 0; margin: 0; min-width: 0; }
.field-label { font-weight: 600; font-size: var(--text-small); margin-top: 4px; }
.input { min-width: 0; padding: 6px 9px; border: 1px solid var(--color-border-strong); border-radius: var(--radius); background: var(--color-surface); color: inherit; }
.small-input { width: 7.5em; display: inline-block; }
select.small-input { width: 9em; }
.wide-select, select.wide-select { width: 14em; }
.range { width: min(100%, 22em); }
.row, .pair, .choices { display: flex; flex-wrap: wrap; gap: 8px 14px; align-items: center; }
.choices.column { flex-direction: column; align-items: flex-start; gap: 6px; }
.inline, .check, .choices label { display: inline-flex; flex-wrap: wrap; gap: 6px; align-items: center; font-size: var(--text-small); }
.self-start { align-self: flex-start; }
.path { overflow-wrap: anywhere; }
.drop {
  display: flex; flex-wrap: wrap; gap: 8px 12px; align-items: center;
  padding: 14px; border: 2px dashed var(--color-border-strong); border-radius: var(--radius);
  background: var(--color-surface-2);
}
.drop.over { border-color: var(--color-primary); background: var(--color-primary-soft); }
.hint { padding: 8px 11px; border-radius: var(--radius); background: var(--tone-info-bg); color: var(--tone-info-text); }
.list { max-height: 280px; overflow: auto; border: 1px solid var(--color-border-strong); border-radius: var(--radius); }
table { width: 100%; border-collapse: collapse; text-align: left; font-size: var(--text-small); }
td, th { padding: 6px 9px; border-bottom: 1px solid var(--color-border-strong); overflow-wrap: anywhere; vertical-align: top; }
td:first-child { width: 40%; }
tr.done td:last-child { color: var(--color-success-text); }
tr.skipped td:last-child, tr.failed td:last-child { color: var(--color-danger-text); }
tr.waiting td:last-child { color: var(--color-text-muted); }
</style>
