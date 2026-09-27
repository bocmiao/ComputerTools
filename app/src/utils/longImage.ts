// 长图拼接里不碰页面的部分：几张图片上下（或者左右）排成一张时，每张放在哪、画多大，整张多大。
// 解码、画到画布上、编码在 LongImageTool.vue 里（WebView2 自带的解码器和编码器，不加新的依赖）。
//
// 竖着拼时「统一宽度」（横着拼时统一高度）：都缩放成最窄（或者最宽）那张的宽度，按比例不变形；「原样」不缩放，
// 窄的居中，两边留白。画布边长、面积都有上限（WebView2 的画布最长 32767 像素，太大了也容易内存不够），
// 拼出来超过上限时整张按比例缩小。

export type Direction = 'vertical' | 'horizontal'
export type SizeMode = 'narrowest' | 'widest' | 'keep'
export type OutputKind = 'jpeg' | 'png'

export interface StitchRules {
  direction: Direction
  size: SizeMode
  /** 两张之间留一道白缝 */
  gap: boolean
  format: OutputKind
}

export interface Size {
  width: number
  height: number
}

export interface Placement {
  x: number
  y: number
  width: number
  height: number
}

export interface StitchLayout {
  width: number
  height: number
  placements: Placement[]
  /** 因为太大整张缩小的比例，1 表示没缩 */
  scale: number
}

/** 拼出来的长图最长多少像素（WebView2 的画布上限是 32767） */
export const MAX_SIDE = 32000
/** 最多多少像素（5000 万，画布要占 200 MB 内存） */
export const MAX_AREA = 50_000_000
/** 一次最多拼多少张 */
export const MAX_IMAGES = 100
/** 存 JPG 时的质量（聊天记录、网页截图上的字要清楚） */
export const JPEG_QUALITY = 0.92

/** 白缝多宽：统一后的宽度（横着拼时是高度）的 2%，8 到 40 像素 */
export function gapPixels(cross: number): number {
  return Math.min(40, Math.max(8, Math.round(cross * 0.02)))
}

/** 排好每一张。`sizes` 是转正以后的尺寸，都大于 0 */
export function stitchLayout(sizes: readonly Size[], rules: Pick<StitchRules, 'direction' | 'size' | 'gap'>): StitchLayout {
  if (!sizes.length) return { width: 0, height: 0, placements: [], scale: 1 }
  const vertical = rules.direction === 'vertical'
  // 沿着拼的方向叫「长」，另一个方向叫「宽」（竖着拼时就是高和宽）
  const cross = sizes.map((s) => (vertical ? s.width : s.height))
  const along = sizes.map((s) => (vertical ? s.height : s.width))
  const target = rules.size === 'widest' ? Math.max(...cross) : Math.min(...cross)
  const canvasCross = rules.size === 'keep' ? Math.max(...cross) : target
  const gap = rules.gap && sizes.length > 1 ? gapPixels(canvasCross) : 0

  // 不缩放整张时的位置（可能有小数）
  const items: { start: number; length: number; offset: number; breadth: number }[] = []
  let position = 0
  for (const [i, c] of cross.entries()) {
    const a = along[i] ?? 0
    const breadth = rules.size === 'keep' ? c : target
    const length = rules.size === 'keep' ? a : (a * target) / c
    items.push({ start: position, length, offset: (canvasCross - breadth) / 2, breadth })
    position += length + gap
  }
  const total = position - gap

  const scale = Math.min(1, MAX_SIDE / Math.max(total, canvasCross), Math.sqrt(MAX_AREA / (total * canvasCross)))
  const outAlong = Math.max(1, Math.round(total * scale))
  const outCross = Math.max(1, Math.round(canvasCross * scale))
  // 按起点、终点分别取整，挨着的两张之间不会多出、也不会少一条缝
  const placements = items.map((it) => {
    const start = Math.round(it.start * scale)
    const length = Math.max(1, Math.round((it.start + it.length) * scale) - start)
    const offset = Math.round(it.offset * scale)
    const breadth = Math.max(1, Math.round((it.offset + it.breadth) * scale) - offset)
    return vertical
      ? { x: offset, y: start, width: breadth, height: length }
      : { x: start, y: offset, width: length, height: breadth }
  })
  return vertical
    ? { width: outCross, height: outAlong, placements, scale }
    : { width: outAlong, height: outCross, placements, scale }
}

/** 「另存为」里建议的名字：长图 2026-09-27.jpg */
export function suggestedName(now: Date, format: OutputKind): string {
  const pad = (n: number): string => String(n).padStart(2, '0')
  return `长图 ${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}.${format === 'png' ? 'png' : 'jpg'}`
}
