// 图片合成 PDF 里不碰页面的部分：每页多大、图片画在哪里、缩到多大，以及把一页一张的 JPEG 拼成一个 PDF 文件。
// 图片在 ImagesToPdfTool.vue 里用 WebView2 自带的解码器打开（按照片里记的方向转正）、画到画布上存成 JPEG（不加新的依赖）。
// PDF 按 ISO 32000-1 写（PDF 1.4 的功能就够用）：每张 JPEG 原样放进一个图片对象（DCTDecode），每页一个内容流把它画到
// 指定的位置。对象编号：1 目录，2 页面树，之后每页三个——页面、内容流、图片。

export type PageSize = 'a4' | 'image'
export type Orientation = 'auto' | 'portrait'
export type Margin = 'none' | 'narrow' | 'wide'
export type Clarity = 'high' | 'standard' | 'small'

export interface PdfRules {
  /** A4 纸，或者和图片一样的比例（没有白边） */
  page: PageSize
  /** A4 时：横图横放，或者都竖着放 */
  orientation: Orientation
  /** A4 时四周留多宽的白边 */
  margin: Margin
  clarity: Clarity
}

/** 1 毫米是多少点（PDF 的长度单位是点，1/72 英寸） */
const MM = 72 / 25.4
export const A4 = { width: 210 * MM, height: 297 * MM } as const
const MARGINS: Record<Margin, number> = { none: 0, narrow: 10 * MM, wide: 20 * MM }
/** 页面的边至少多长（PDF 阅读器认的最小页面） */
const MIN_PAGE_EDGE = 3

/** 清晰度：图片长边最多多少像素、JPEG 的质量。长边按 A4 纸的长边（297 毫米）算，大约 300、200、140 dpi */
export const CLARITIES: Record<Clarity, { longEdge: number; quality: number }> = {
  high: { longEdge: 3508, quality: 0.9 },
  standard: { longEdge: 2339, quality: 0.85 },
  small: { longEdge: 1654, quality: 0.75 },
}

/** 一个 PDF 最多多少页 */
export const MAX_PAGES = 200
/** 拼好的 PDF 最多多大（和后端的上限一样） */
export const MAX_PDF_BYTES = 512 * 1024 * 1024

/** 转了 turns 个 90° 以后的宽和高 */
export function turnedSize(width: number, height: number, turns: number): { width: number; height: number } {
  return turns % 2 ? { width: height, height: width } : { width, height }
}

/** 图片缩到多大：长边不超过清晰度的上限，只缩小不放大 */
export function scaledSize(width: number, height: number, clarity: Clarity): { width: number; height: number } {
  const scale = Math.min(1, CLARITIES[clarity].longEdge / Math.max(width, height))
  return { width: Math.max(1, Math.round(width * scale)), height: Math.max(1, Math.round(height * scale)) }
}

export interface PageLayout {
  /** 页面的宽和高（点） */
  width: number
  height: number
  /** 图片左下角的位置、画出来的宽和高（点；PDF 的原点在页面的左下角） */
  x: number
  y: number
  w: number
  h: number
}

/**
 * 一页多大、图片放在哪里。A4：横图横放（方向选了「都竖着放」的除外），图片按比例放大或缩小到白边以内，居中；
 * 和图片一样的比例：没有白边，长边和 A4 的长边一样长。
 */
export function pageLayout(imageWidth: number, imageHeight: number, rules: PdfRules): PageLayout {
  if (rules.page === 'image') {
    const scale = A4.height / Math.max(imageWidth, imageHeight)
    const w = imageWidth * scale
    const h = imageHeight * scale
    const width = Math.max(MIN_PAGE_EDGE, w)
    const height = Math.max(MIN_PAGE_EDGE, h)
    return { width, height, x: (width - w) / 2, y: (height - h) / 2, w, h }
  }
  const landscape = rules.orientation === 'auto' && imageWidth > imageHeight
  const width = landscape ? A4.height : A4.width
  const height = landscape ? A4.width : A4.height
  const margin = MARGINS[rules.margin]
  const scale = Math.min((width - 2 * margin) / imageWidth, (height - 2 * margin) / imageHeight)
  const w = imageWidth * scale
  const h = imageHeight * scale
  return { width, height, x: (width - w) / 2, y: (height - h) / 2, w, h }
}

export interface PdfPage {
  /** 这一页的图片（JPEG 文件的内容，三个颜色通道：画布存出来的就是这样） */
  jpeg: Uint8Array
  pixelWidth: number
  pixelHeight: number
  layout: PageLayout
}

/** PDF 里的数字：最多两位小数，不用科学计数法 */
function num(value: number): string {
  const rounded = Math.round(value * 100) / 100
  return (Object.is(rounded, -0) ? 0 : rounded).toFixed(2).replace(/\.?0+$/, '')
}

/** 把一页一张的 JPEG 拼成 PDF */
export function buildPdf(pages: readonly PdfPage[]): Uint8Array {
  if (!pages.length) throw new Error('没有图片，生成不了 PDF。')
  const encoder = new TextEncoder()
  const chunks: Uint8Array[] = []
  const offsets: number[] = []
  let length = 0
  const push = (data: Uint8Array | string): void => {
    const bytes = typeof data === 'string' ? encoder.encode(data) : data
    chunks.push(bytes)
    length += bytes.length
  }
  const begin = (n: number): void => {
    offsets[n] = length
    push(`${n} 0 obj\n`)
  }
  const pageObject = (index: number): number => 3 + index * 3

  // 第二行的注释里放几个大于 127 的字节，告诉传输工具这是二进制文件
  push('%PDF-1.4\n')
  push(new Uint8Array([0x25, 0xe2, 0xe3, 0xcf, 0xd3, 0x0a]))
  begin(1)
  push('<< /Type /Catalog /Pages 2 0 R >>\nendobj\n')
  begin(2)
  const kids = pages.map((_, i) => `${pageObject(i)} 0 R`).join(' ')
  push(`<< /Type /Pages /Kids [${kids}] /Count ${pages.length} >>\nendobj\n`)
  pages.forEach((page, i) => {
    const n = pageObject(i)
    const l = page.layout
    begin(n)
    push(
      `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${num(l.width)} ${num(l.height)}] ` +
        `/Resources << /XObject << /Im0 ${n + 2} 0 R >> /ProcSet [/PDF /ImageC] >> /Contents ${n + 1} 0 R >>\nendobj\n`,
    )
    const content = `q ${num(l.w)} 0 0 ${num(l.h)} ${num(l.x)} ${num(l.y)} cm /Im0 Do Q`
    begin(n + 1)
    push(`<< /Length ${content.length} >>\nstream\n${content}\nendstream\nendobj\n`)
    begin(n + 2)
    push(
      `<< /Type /XObject /Subtype /Image /Width ${page.pixelWidth} /Height ${page.pixelHeight} ` +
        `/ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length ${page.jpeg.length} >>\nstream\n`,
    )
    push(page.jpeg)
    push('\nendstream\nendobj\n')
  })
  const size = pageObject(pages.length)
  const xref = length
  // 交叉引用表：每一项正好 20 个字节
  push(`xref\n0 ${size}\n0000000000 65535 f\r\n`)
  for (let n = 1; n < size; n++) push(`${String(offsets[n]).padStart(10, '0')} 00000 n\r\n`)
  push(`trailer\n<< /Size ${size} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`)

  const out = new Uint8Array(length)
  let at = 0
  for (const chunk of chunks) {
    out.set(chunk, at)
    at += chunk.length
  }
  return out
}

/** 「另存为」对话框里默认的名字，比如「图片合成 2026-09-27.pdf」 */
export function suggestedName(now: Date): string {
  const pad = (n: number): string => String(n).padStart(2, '0')
  return `图片合成 ${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}.pdf`
}

/** 按文件名排（数字按大小：第2页 排在 第10页 前面） */
export function byName(a: string, b: string): number {
  return a.localeCompare(b, 'zh-CN', { numeric: true, sensitivity: 'base' })
}
