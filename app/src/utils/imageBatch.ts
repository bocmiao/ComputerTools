// 图片批量处理里不碰页面的部分：输出格式、文件名、尺寸怎么算，「压到多少 KB 以内」怎么试。
// 解码、画到画布上、编码在 BatchImageTool.vue 里（WebView2 自带的解码器和编码器，不加新的依赖）。
//
// WebView2 能打开 JPG、PNG、WebP、GIF（只取第一帧）、BMP、ICO、AVIF，打不开 HEIC（苹果手机的照片）和 TIFF；
// 画布只能存成 JPG、PNG、WebP。

export type OutputFormat = 'keep' | 'jpeg' | 'png' | 'webp'
export type OutputMime = 'image/jpeg' | 'image/png' | 'image/webp'
export type ResizeMode = 'none' | 'long-edge' | 'fixed'

export interface ImageRules {
  format: OutputFormat
  /** JPG、WebP 的质量，1–100 */
  quality: number
  resize: ResizeMode
  /** 长边最多多少像素（resize 为 long-edge 时） */
  longEdge: number
  /** 固定尺寸（resize 为 fixed 时，按比例放大或缩小后居中裁掉多的部分） */
  width: number
  height: number
  /** 每张最多多少 KB，0 表示不限 */
  maxKb: number
  /** 新文件名在原名后面加的字 */
  suffix: string
  /** 格式、尺寸都不变，处理完反而更大时，存原图（加了水印时不存原图：原图上没有水印） */
  keepSmaller: boolean
  /** 水印文字，空的就不加 */
  watermark: string
  /** 水印的不透明度，0–1 */
  watermarkOpacity: number
}

/** 一张图片最多多少像素（1 亿：两亿像素的手机照片解码要将近 1 GB 内存，容易失败） */
export const MAX_PIXELS = 100_000_000
/** 一次最多处理多少张 */
export const MAX_FILES = 500
/** 输出的边长至少多少像素 */
const MIN_EDGE = 16

const ENCODABLE: readonly string[] = ['image/jpeg', 'image/png', 'image/webp']

/** 证件照常用的尺寸（300 dpi） */
export const ID_PHOTO_SIZES = [
  { label: '一寸（295 × 413）', width: 295, height: 413 },
  { label: '小二寸（413 × 531）', width: 413, height: 531 },
  { label: '二寸（413 × 579）', width: 413, height: 579 },
] as const

function extensionOf(name: string): string {
  const dot = name.lastIndexOf('.')
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : ''
}

/** 有 HEIC 照片没处理时，在列表上面说一次怎么办（不在每一行里重复） */
export const HEIC_ADVICE =
  'HEIC 是苹果手机的照片格式，这里打不开。以后想直接拍成 JPG：在 iPhone 的「设置 → 相机 → 格式」里选「兼容性最佳」。已经拍了的：在「设置 → 照片 → 传输到 Mac 或 PC」里选「自动」，用数据线传到电脑时会自动转成 JPG。'

export function isHeic(name: string, type: string): boolean {
  const ext = extensionOf(name)
  return ext === 'heic' || ext === 'heif' || type === 'image/heic' || type === 'image/heif'
}

/** 打不开的图片说明原因；能处理的返回 null */
export function unsupportedReason(name: string, type: string): string | null {
  const ext = extensionOf(name)
  if (isHeic(name, type)) return 'HEIC（苹果手机的照片格式）这里打不开，办法见列表上面。'
  if (ext === 'tif' || ext === 'tiff' || type === 'image/tiff') return 'TIFF 格式这里打不开。'
  if (ext === 'svg' || type === 'image/svg+xml') return 'SVG 是矢量图，不用压缩，这里不处理。'
  if (!type.startsWith('image/') && !['jpg', 'jpeg', 'png', 'webp', 'gif', 'bmp', 'ico', 'avif'].includes(ext)) {
    return '这不是图片文件。'
  }
  return null
}

/** 存成什么格式：「格式不变」时，画布存不了的格式（GIF、BMP、ICO、AVIF）存成 PNG */
export function outputMime(inputType: string, format: OutputFormat): OutputMime {
  if (format === 'jpeg') return 'image/jpeg'
  if (format === 'png') return 'image/png'
  if (format === 'webp') return 'image/webp'
  const type = inputType === 'image/jpg' ? 'image/jpeg' : inputType
  return ENCODABLE.includes(type) ? (type as OutputMime) : 'image/png'
}

export function extensionFor(mime: OutputMime): string {
  return mime === 'image/jpeg' ? 'jpg' : mime === 'image/png' ? 'png' : 'webp'
}

export function formatLabel(mime: OutputMime): string {
  return mime === 'image/jpeg' ? 'JPG' : mime === 'image/png' ? 'PNG' : 'WebP'
}

/** 两个类型是不是同一种格式（有的系统把 JPG 报成 image/jpg） */
export function sameFormat(inputType: string, mime: OutputMime): boolean {
  return (inputType === 'image/jpg' ? 'image/jpeg' : inputType) === mime
}

/** 文件开头的标记和格式对不对得上（和后端保存前的检查一样：扩展名是 .jpg、内容却是 PNG 的原图不能原样存） */
export function matchesFormat(head: Uint8Array, mime: OutputMime): boolean {
  const starts = (bytes: number[], at = 0) => bytes.every((b, i) => head[at + i] === b)
  if (mime === 'image/jpeg') return starts([0xff, 0xd8, 0xff])
  if (mime === 'image/png') return starts([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])
  return starts([0x52, 0x49, 0x46, 0x46]) && starts([0x57, 0x45, 0x42, 0x50], 8)
}

/** 加在文件名后面的字能不能用（Windows 文件名不允许的字符） */
export function suffixProblem(suffix: string): string | null {
  return /[<>:"/\\|?*\u0000-\u001f]/.test(suffix) ? '文件名后面加的字里有 Windows 不允许的字符（< > : " / \\ | ? *）。' : null
}

/** 新文件名：原名（去掉扩展名）+ 后面加的字 + 新扩展名。重名由后端处理（加「 (2)」），不覆盖 */
export function outputName(original: string, mime: OutputMime, suffix: string): string {
  const dot = original.lastIndexOf('.')
  const stem = (dot > 0 ? original.slice(0, dot) : original).replace(/[ .]+$/, '') || '图片'
  return `${stem}${suffix}.${extensionFor(mime)}`
}

/** 画的时候从原图取哪一块（sx、sy、sw、sh），画成多大（width、height） */
export interface DrawPlan {
  sx: number
  sy: number
  sw: number
  sh: number
  width: number
  height: number
}

export function drawPlan(w: number, h: number, rules: Pick<ImageRules, 'resize' | 'longEdge' | 'width' | 'height'>): DrawPlan {
  const whole = { sx: 0, sy: 0, sw: w, sh: h }
  if (rules.resize === 'long-edge' && rules.longEdge > 0) {
    const scale = Math.min(1, rules.longEdge / Math.max(w, h))
    return { ...whole, width: Math.max(1, Math.round(w * scale)), height: Math.max(1, Math.round(h * scale)) }
  }
  if (rules.resize === 'fixed' && rules.width > 0 && rules.height > 0) {
    // 按比例缩放到正好盖住目标，居中裁掉多出来的部分
    const scale = Math.max(rules.width / w, rules.height / h)
    const sw = Math.min(w, rules.width / scale)
    const sh = Math.min(h, rules.height / scale)
    return { sx: (w - sw) / 2, sy: (h - sh) / 2, sw, sh, width: rules.width, height: rules.height }
  }
  return { ...whole, width: w, height: h }
}

/** 水印最多多少个字 */
export const WATERMARK_MAX = 40
export const WATERMARK_OPACITIES = [
  { label: '淡', value: 0.25 },
  { label: '中', value: 0.4 },
  { label: '深', value: 0.6 },
] as const

export interface WatermarkPlan {
  /** 字号（像素） */
  fontSize: number
  /** 旋转的角度（弧度）：斜着铺 */
  angle: number
  /** 旋转以后，同一行两处文字的间距、行距（像素） */
  stepX: number
  stepY: number
  /** 以图片中心为原点，要铺到多远（对角线的一半）：转了角度也能铺满四个角 */
  reach: number
}

/**
 * 水印怎么铺：字号跟着短边走（短边的 1/16，至少 14 像素），斜 30 度，一行一行错开半个间距铺满整张图，
 * 身份证复印件这类图片上哪一块都裁不掉。measure 给出这个字号下整段文字有多宽。
 */
export function watermarkPlan(width: number, height: number, measure: (fontSize: number) => number): WatermarkPlan {
  const fontSize = Math.max(14, Math.round(Math.min(width, height) / 16))
  return {
    fontSize,
    angle: -Math.PI / 6,
    stepX: Math.ceil(measure(fontSize) + fontSize * 3),
    stepY: Math.ceil(fontSize * 4),
    reach: Math.ceil(Math.hypot(width, height) / 2),
  }
}

/** 尺寸有没有变 */
export function keepsSize(plan: DrawPlan, w: number, h: number): boolean {
  return plan.width === w && plan.height === h && plan.sw === w && plan.sh === h
}

export interface Encoded<T> {
  size: number
  data: T
}

/** 按质量（0–1）和缩放比例（0–1，相对 DrawPlan 的尺寸）编码一次 */
export type Encoder<T> = (quality: number, scale: number) => Promise<Encoded<T>>

export interface Fitted<T> extends Encoded<T> {
  quality: number
  scale: number
}

/**
 * 压到 maxBytes 以内：先用设定的质量试；太大就在 40% 到设定的质量之间二分，找能放下的最高质量；
 * 最低质量也放不下，就按文件大小的比例缩小尺寸再试（PNG 不能调质量，只能缩小）。
 * 缩到边长不到 16 像素也放不下，返回 null。
 */
export async function fitToSize<T>(
  encode: Encoder<T>,
  maxBytes: number,
  quality: number,
  lossy: boolean,
  width: number,
  height: number,
): Promise<Fitted<T> | null> {
  const first = await encode(quality, 1)
  if (first.size <= maxBytes) return { ...first, quality, scale: 1 }
  let best: Fitted<T> | null = null
  const floor = Math.min(quality, 0.4)
  if (lossy) {
    let low = floor
    let high = quality
    const lowest = await encode(low, 1)
    if (lowest.size <= maxBytes) {
      best = { ...lowest, quality: low, scale: 1 }
      for (let i = 0; i < 6 && high - low > 0.02; i++) {
        const mid = (low + high) / 2
        const tried = await encode(mid, 1)
        if (tried.size <= maxBytes) {
          best = { ...tried, quality: mid, scale: 1 }
          low = mid
        } else {
          high = mid
        }
      }
      return best
    }
  }
  // 只能缩小尺寸：文件大小大致和像素数成正比
  const q = lossy ? Math.max(floor, Math.min(quality, 0.75)) : quality
  let scale = 1
  let last = first.size
  for (let i = 0; i < 8; i++) {
    scale *= Math.min(0.9, Math.sqrt(maxBytes / last) * 0.95)
    if (width * scale < MIN_EDGE || height * scale < MIN_EDGE) return null
    const tried = await encode(q, scale)
    if (tried.size <= maxBytes) return { ...tried, quality: q, scale }
    last = tried.size
  }
  return null
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(bytes < 10 * 1024 ? 1 : 0)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`
}
