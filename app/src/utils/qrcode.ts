// 二维码（QR Code Model 2）编码：字节模式（UTF-8），不加依赖。
// 按 ISO/IEC 18004 的做法：选最小的版本 → 数据位 → 分块加 Reed-Solomon 纠错码并交错 → 画定位、校正、时序图形 →
// 按之字形放数据 → 8 种掩模里挑扣分最少的 → 写格式信息（和版本信息）。
// 表格和算法照 Project Nayuki 公开的 QR Code generator（MIT）写的；测试用 segno 生成的码逐格对比。

export type EccLevel = 'L' | 'M' | 'Q' | 'H'

export interface QrCode {
  version: number
  /** 边长（格数），不含四周留白 */
  size: number
  mask: number
  /** modules[y][x]：true 是黑格 */
  modules: boolean[][]
}

const ECC_INDEX: Record<EccLevel, number> = { L: 0, M: 1, Q: 2, H: 3 }
/** 格式信息里的纠错等级位：L=01、M=00、Q=11、H=10 */
const ECC_FORMAT_BITS: Record<EccLevel, number> = { L: 1, M: 0, Q: 3, H: 2 }

// 每块的纠错码字数、块数，按纠错等级（L、M、Q、H）和版本（1–40，下标 0 不用）
const ECC_CODEWORDS_PER_BLOCK: number[][] = [
  [-1, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28, 30, 30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
  [-1, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28],
  [-1, 13, 22, 18, 26, 18, 24, 18, 22, 20, 24, 28, 26, 24, 20, 30, 24, 28, 28, 26, 30, 28, 30, 30, 30, 30, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
  [-1, 17, 28, 22, 16, 22, 28, 26, 26, 24, 28, 24, 28, 22, 24, 24, 30, 28, 28, 26, 28, 30, 24, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
]
const NUM_ERROR_CORRECTION_BLOCKS: number[][] = [
  [-1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12, 13, 14, 15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25],
  [-1, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49],
  [-1, 1, 1, 2, 2, 4, 4, 6, 6, 8, 8, 8, 10, 12, 16, 12, 17, 16, 18, 21, 20, 23, 23, 25, 27, 29, 34, 34, 35, 38, 40, 43, 45, 48, 51, 53, 56, 59, 62, 65, 68],
  [-1, 1, 1, 2, 4, 4, 4, 5, 6, 8, 8, 11, 11, 16, 16, 18, 16, 19, 21, 25, 25, 25, 34, 30, 32, 35, 37, 40, 42, 45, 48, 51, 54, 57, 60, 63, 66, 70, 74, 77, 81],
]

/** 这个版本里能放数据和纠错码的格数（去掉定位、校正、时序、格式和版本信息） */
function rawDataModules(ver: number): number {
  let result = (16 * ver + 128) * ver + 64
  if (ver >= 2) {
    const numAlign = Math.floor(ver / 7) + 2
    result -= (25 * numAlign - 10) * numAlign - 55
    if (ver >= 7) result -= 36
  }
  return result
}

function dataCodewords(ver: number, ecl: EccLevel): number {
  const e = ECC_INDEX[ecl]
  return Math.floor(rawDataModules(ver) / 8) - ECC_CODEWORDS_PER_BLOCK[e]![ver]! * NUM_ERROR_CORRECTION_BLOCKS[e]![ver]!
}

/** 字节模式的字符数占几位 */
function countBits(ver: number): number {
  return ver <= 9 ? 8 : 16
}

// ── GF(2^8)（本原多项式 0x11D）上的 Reed-Solomon ──
function gfMultiply(x: number, y: number): number {
  let z = 0
  for (let i = 7; i >= 0; i--) {
    z = (z << 1) ^ ((z >>> 7) * 0x11d)
    z ^= ((y >>> i) & 1) * x
  }
  return z
}

function rsDivisor(degree: number): number[] {
  const result = new Array<number>(degree).fill(0)
  result[degree - 1] = 1
  let root = 1
  for (let i = 0; i < degree; i++) {
    for (let j = 0; j < result.length; j++) {
      result[j] = gfMultiply(result[j]!, root)
      if (j + 1 < result.length) result[j]! ^= result[j + 1]!
    }
    root = gfMultiply(root, 0x02)
  }
  return result
}

function rsRemainder(data: number[], divisor: number[]): number[] {
  const result = divisor.map(() => 0)
  for (const b of data) {
    const factor = b ^ result.shift()!
    result.push(0)
    divisor.forEach((coef, i) => {
      result[i]! ^= gfMultiply(coef, factor)
    })
  }
  return result
}

function withEcc(data: number[], ver: number, ecl: EccLevel): number[] {
  const e = ECC_INDEX[ecl]
  const numBlocks = NUM_ERROR_CORRECTION_BLOCKS[e]![ver]!
  const blockEccLen = ECC_CODEWORDS_PER_BLOCK[e]![ver]!
  const rawCodewords = Math.floor(rawDataModules(ver) / 8)
  const numShortBlocks = numBlocks - (rawCodewords % numBlocks)
  const shortBlockLen = Math.floor(rawCodewords / numBlocks)
  const divisor = rsDivisor(blockEccLen)
  const blocks: number[][] = []
  for (let i = 0, k = 0; i < numBlocks; i++) {
    const dat = data.slice(k, k + shortBlockLen - blockEccLen + (i < numShortBlocks ? 0 : 1))
    k += dat.length
    const ecc = rsRemainder(dat, divisor)
    if (i < numShortBlocks) dat.push(0)
    blocks.push(dat.concat(ecc))
  }
  // 交错：依次取每块的第 i 个码字（短块补的那个 0 跳过）
  const result: number[] = []
  for (let i = 0; i < blocks[0]!.length; i++) {
    blocks.forEach((block, j) => {
      if (i !== shortBlockLen - blockEccLen || j >= numShortBlocks) result.push(block[i]!)
    })
  }
  return result
}

/** 字节模式的数据码字（含结束符和填充） */
function dataBytes(bytes: Uint8Array, ver: number, ecl: EccLevel): number[] {
  const bits: number[] = []
  const put = (value: number, len: number) => {
    for (let i = len - 1; i >= 0; i--) bits.push((value >>> i) & 1)
  }
  put(0b0100, 4)
  put(bytes.length, countBits(ver))
  for (const b of bytes) put(b, 8)
  const capacity = dataCodewords(ver, ecl) * 8
  put(0, Math.min(4, capacity - bits.length))
  put(0, (8 - (bits.length % 8)) % 8)
  const out: number[] = []
  for (let i = 0; i < bits.length; i += 8) out.push(bits.slice(i, i + 8).reduce((a, b) => (a << 1) | b, 0))
  for (let pad = 0xec; out.length < capacity / 8; pad ^= 0xec ^ 0x11) out.push(pad)
  return out
}

function alignmentPositions(ver: number, size: number): number[] {
  if (ver === 1) return []
  const numAlign = Math.floor(ver / 7) + 2
  const step = Math.floor((ver * 8 + numAlign * 3 + 5) / (numAlign * 4 - 4)) * 2
  const result = [6]
  for (let pos = size - 7; result.length < numAlign; pos -= step) result.splice(1, 0, pos)
  return result
}

class Grid {
  readonly size: number
  readonly modules: boolean[][]
  readonly isFunction: boolean[][]

  constructor(size: number) {
    this.size = size
    this.modules = Array.from({ length: size }, () => new Array<boolean>(size).fill(false))
    this.isFunction = Array.from({ length: size }, () => new Array<boolean>(size).fill(false))
  }

  setFunction(x: number, y: number, dark: boolean): void {
    this.modules[y]![x] = dark
    this.isFunction[y]![x] = true
  }
}

function drawFunctionPatterns(g: Grid, ver: number, ecl: EccLevel): void {
  const size = g.size
  for (let i = 0; i < size; i++) {
    g.setFunction(6, i, i % 2 === 0)
    g.setFunction(i, 6, i % 2 === 0)
  }
  const finder = (x: number, y: number) => {
    for (let dy = -4; dy <= 4; dy++) {
      for (let dx = -4; dx <= 4; dx++) {
        const dist = Math.max(Math.abs(dx), Math.abs(dy))
        const xx = x + dx
        const yy = y + dy
        if (xx >= 0 && xx < size && yy >= 0 && yy < size) g.setFunction(xx, yy, dist !== 2 && dist !== 4)
      }
    }
  }
  finder(3, 3)
  finder(size - 4, 3)
  finder(3, size - 4)
  const align = alignmentPositions(ver, size)
  const n = align.length
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < n; j++) {
      if ((i === 0 && j === 0) || (i === 0 && j === n - 1) || (i === n - 1 && j === 0)) continue
      for (let dy = -2; dy <= 2; dy++) {
        for (let dx = -2; dx <= 2; dx++) {
          g.setFunction(align[i]! + dx, align[j]! + dy, Math.max(Math.abs(dx), Math.abs(dy)) !== 1)
        }
      }
    }
  }
  drawFormatBits(g, ecl, 0) // 先占位，挑好掩模以后再写
  if (ver >= 7) {
    let rem = ver
    for (let i = 0; i < 12; i++) rem = (rem << 1) ^ ((rem >>> 11) * 0x1f25)
    const bits = (ver << 12) | rem
    for (let i = 0; i < 18; i++) {
      const dark = ((bits >>> i) & 1) !== 0
      const a = size - 11 + (i % 3)
      const b = Math.floor(i / 3)
      g.setFunction(a, b, dark)
      g.setFunction(b, a, dark)
    }
  }
}

function drawFormatBits(g: Grid, ecl: EccLevel, mask: number): void {
  const data = (ECC_FORMAT_BITS[ecl] << 3) | mask
  let rem = data
  for (let i = 0; i < 10; i++) rem = (rem << 1) ^ ((rem >>> 9) * 0x537)
  const bits = ((data << 10) | rem) ^ 0x5412
  const bit = (i: number) => ((bits >>> i) & 1) !== 0
  const size = g.size
  for (let i = 0; i <= 5; i++) g.setFunction(8, i, bit(i))
  g.setFunction(8, 7, bit(6))
  g.setFunction(8, 8, bit(7))
  g.setFunction(7, 8, bit(8))
  for (let i = 9; i < 15; i++) g.setFunction(14 - i, 8, bit(i))
  for (let i = 0; i < 8; i++) g.setFunction(size - 1 - i, 8, bit(i))
  for (let i = 8; i < 15; i++) g.setFunction(8, size - 15 + i, bit(i))
  g.setFunction(8, size - 8, true) // 固定的黑格
}

function drawCodewords(g: Grid, data: number[]): void {
  const size = g.size
  let i = 0
  for (let right = size - 1; right >= 1; right -= 2) {
    if (right === 6) right = 5
    for (let vert = 0; vert < size; vert++) {
      for (let j = 0; j < 2; j++) {
        const x = right - j
        const upward = ((right + 1) & 2) === 0
        const y = upward ? size - 1 - vert : vert
        if (!g.isFunction[y]![x] && i < data.length * 8) {
          g.modules[y]![x] = ((data[i >>> 3]! >>> (7 - (i & 7))) & 1) !== 0
          i++
        }
      }
    }
  }
}

function maskBit(mask: number, x: number, y: number): boolean {
  switch (mask) {
    case 0: return (x + y) % 2 === 0
    case 1: return y % 2 === 0
    case 2: return x % 3 === 0
    case 3: return (x + y) % 3 === 0
    case 4: return (Math.floor(x / 3) + Math.floor(y / 2)) % 2 === 0
    case 5: return ((x * y) % 2) + ((x * y) % 3) === 0
    case 6: return (((x * y) % 2) + ((x * y) % 3)) % 2 === 0
    default: return (((x + y) % 2) + ((x * y) % 3)) % 2 === 0
  }
}

function applyMask(g: Grid, mask: number): void {
  for (let y = 0; y < g.size; y++) {
    for (let x = 0; x < g.size; x++) {
      if (!g.isFunction[y]![x] && maskBit(mask, x, y)) g.modules[y]![x] = !g.modules[y]![x]
    }
  }
}

/** 掩模的扣分（ISO/IEC 18004 的四条规则），越少越好 */
function penalty(g: Grid): number {
  const size = g.size
  const m = g.modules
  let result = 0
  const addHistory = (run: number, history: number[]) => {
    if (history[0] === 0) run += size // 最开头的一段加上外面的留白
    history.pop()
    history.unshift(run)
  }
  const countPatterns = (h: number[]) => {
    const n = h[1]!
    const core = n > 0 && h[2] === n && h[3] === n * 3 && h[4] === n && h[5] === n
    return (core && h[0]! >= n * 4 && h[6]! >= n ? 1 : 0) + (core && h[6]! >= n * 4 && h[0]! >= n ? 1 : 0)
  }
  const terminate = (color: boolean, run: number, history: number[]) => {
    if (color) {
      addHistory(run, history)
      run = 0
    }
    addHistory(run + size, history) // 最后一段加上外面的留白
    return countPatterns(history)
  }
  const line = (get: (a: number, b: number) => boolean) => {
    for (let a = 0; a < size; a++) {
      let color = false
      let run = 0
      const history = [0, 0, 0, 0, 0, 0, 0]
      for (let b = 0; b < size; b++) {
        if (get(a, b) === color) {
          run++
          if (run === 5) result += 3
          else if (run > 5) result++
        } else {
          addHistory(run, history)
          if (!color) result += countPatterns(history) * 40
          color = get(a, b)
          run = 1
        }
      }
      result += terminate(color, run, history) * 40
    }
  }
  line((y, x) => m[y]![x]!)
  line((x, y) => m[y]![x]!)
  for (let y = 0; y < size - 1; y++) {
    for (let x = 0; x < size - 1; x++) {
      const c = m[y]![x]
      if (c === m[y]![x + 1] && c === m[y + 1]![x] && c === m[y + 1]![x + 1]) result += 3
    }
  }
  let dark = 0
  for (const row of m) for (const v of row) if (v) dark++
  const total = size * size
  const k = Math.ceil(Math.abs(dark * 20 - total * 10) / total) - 1
  return result + k * 10
}

/**
 * 把文字编成二维码（UTF-8 字节模式）。放不下（超过版本 40）时抛出错误。
 * mask 不给就自动挑扣分最少的（测试时可以指定，和别的实现逐格对比）。
 */
export function encodeQr(text: string, options: { ecl?: EccLevel; mask?: number } = {}): QrCode {
  const ecl = options.ecl ?? 'M'
  const bytes = new TextEncoder().encode(text)
  let ver = 1
  for (; ver <= 40; ver++) {
    if (4 + countBits(ver) + bytes.length * 8 <= dataCodewords(ver, ecl) * 8) break
  }
  if (ver > 40) throw new Error('内容太长，放不进二维码')
  const size = ver * 4 + 17
  const g = new Grid(size)
  drawFunctionPatterns(g, ver, ecl)
  drawCodewords(g, withEcc(dataBytes(bytes, ver, ecl), ver, ecl))

  let mask = options.mask ?? -1
  if (mask < 0) {
    let best = Infinity
    for (let candidate = 0; candidate < 8; candidate++) {
      applyMask(g, candidate)
      drawFormatBits(g, ecl, candidate)
      const score = penalty(g)
      if (score < best) {
        best = score
        mask = candidate
      }
      applyMask(g, candidate) // 再异或一次就还原了
    }
  }
  applyMask(g, mask)
  drawFormatBits(g, ecl, mask)
  return { version: ver, size, mask, modules: g.modules }
}
