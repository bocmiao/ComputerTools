// 文本对比：两段文字逐行比较（Myers 差分算法），改过的行再逐字比较，标出具体哪几个字不一样。
// 只在本机计算。差别太多（要改的行数超过 MAX_EDITS）时不再细比，免得卡住界面。

export type DiffOp = 'same' | 'add' | 'del'

/** 一行里的一段：changed 为 true 的是这一行和对面那一行不一样的字 */
export interface DiffPiece {
  changed: boolean
  text: string
}

export interface DiffLine {
  op: DiffOp
  text: string
  /** 原来的第几行（del、same 有） */
  oldNo?: number
  /** 改过以后的第几行（add、same 有） */
  newNo?: number
  /** 这一行被改过（和对面的一行配成对）时，逐字比较的结果 */
  pieces?: DiffPiece[]
}

export interface TextDiffResult {
  lines: DiffLine[]
  /** 多出来的行、少了的行 */
  added: number
  removed: number
  /** 改动有几处（连在一起的改动算一处） */
  blocks: number
  /** 两段文字一样 */
  same: boolean
  /** 差别太多，没有逐行细比（整段算作换掉了） */
  tooDifferent: boolean
}

export interface DiffOptions {
  /** 比较时不管每行首尾的空格 */
  ignoreSpaces?: boolean
}

/** 最多比较这么多处改动（Myers 算法记下的中间状态和它的平方成正比） */
export const MAX_EDITS = 2000
/** 逐字比较的一行最长这么多字 */
const MAX_CHARS = 2000

type Run = { op: DiffOp; from: number; to: number; count: number }

/**
 * Myers 差分：a 变成 b 的最短编辑，按「相同、删除、插入」连成段返回；from、to 是这一段在 a、b 里的起点。
 * 编辑次数超过 maxEdits 时返回 null。
 */
export function diffRuns<T>(a: readonly T[], b: readonly T[], maxEdits = MAX_EDITS, eq: (x: T, y: T) => boolean = (x, y) => x === y): Run[] | null {
  const n = a.length
  const m = b.length
  const max = n + m
  const offset = max + 1
  const v = new Int32Array(2 * max + 3)
  // 第 d 步开始前 v[-d..d] 的样子，回溯时用
  const trace: Int32Array[] = []
  let steps = -1
  for (let d = 0; d <= max; d++) {
    if (d > maxEdits) return null
    trace.push(v.slice(offset - d, offset + d + 1))
    let done = false
    for (let k = -d; k <= d; k += 2) {
      let x = k === -d || (k !== d && v[offset + k - 1]! < v[offset + k + 1]!) ? v[offset + k + 1]! : v[offset + k - 1]! + 1
      let y = x - k
      while (x < n && y < m && eq(a[x]!, b[y]!)) {
        x++
        y++
      }
      v[offset + k] = x
      if (x >= n && y >= m) {
        done = true
        break
      }
    }
    if (done) {
      steps = d
      break
    }
  }
  // 从终点往回走，得到每一步是删除、插入还是相同
  const ops: DiffOp[] = []
  let x = n
  let y = m
  for (let d = steps; d > 0; d--) {
    const saved = trace[d]!
    const at = (k: number): number => saved[k + d]!
    const k = x - y
    const prevK = k === -d || (k !== d && at(k - 1) < at(k + 1)) ? k + 1 : k - 1
    const prevX = at(prevK)
    const prevY = prevX - prevK
    while (x > prevX && y > prevY) {
      ops.push('same')
      x--
      y--
    }
    ops.push(x === prevX ? 'add' : 'del')
    x = prevX
    y = prevY
  }
  while (x > 0 && y > 0) {
    ops.push('same')
    x--
    y--
  }
  ops.reverse()
  const runs: Run[] = []
  let ai = 0
  let bi = 0
  for (const op of ops) {
    const last = runs[runs.length - 1]
    if (last && last.op === op) last.count++
    else runs.push({ op, from: ai, to: bi, count: 1 })
    if (op !== 'add') ai++
    if (op !== 'del') bi++
  }
  return runs
}

/** 逐字比较两行，分别返回两边的分段 */
export function diffChars(before: string, after: string): { before: DiffPiece[]; after: DiffPiece[] } | null {
  const a = Array.from(before)
  const b = Array.from(after)
  if (a.length > MAX_CHARS || b.length > MAX_CHARS) return null
  const runs = diffRuns(a, b, MAX_CHARS)
  if (!runs) return null
  const left: DiffPiece[] = []
  const right: DiffPiece[] = []
  const push = (list: DiffPiece[], changed: boolean, text: string): void => {
    const last = list[list.length - 1]
    if (last && last.changed === changed) last.text += text
    else list.push({ changed, text })
  }
  for (const r of runs) {
    if (r.op === 'same') {
      const text = a.slice(r.from, r.from + r.count).join('')
      push(left, false, text)
      push(right, false, text)
    } else if (r.op === 'del') {
      push(left, true, a.slice(r.from, r.from + r.count).join(''))
    } else {
      push(right, true, b.slice(r.to, r.to + r.count).join(''))
    }
  }
  return { before: left, after: right }
}

function splitLines(text: string): string[] {
  if (text === '') return []
  const lines = text.replace(/\r\n?/g, '\n').split('\n')
  // 末尾的换行不算多出一行
  if (lines.length > 1 && lines[lines.length - 1] === '') lines.pop()
  return lines
}

/** 两段文字的对比结果 */
export function diffTexts(before: string, after: string, options: DiffOptions = {}): TextDiffResult {
  const a = splitLines(before)
  const b = splitLines(after)
  const key = (line: string): string => (options.ignoreSpaces ? line.trim().replace(/[ \t　]+/g, ' ') : line)
  const ka = a.map(key)
  const kb = b.map(key)
  const found = diffRuns(ka, kb)
  const tooDifferent = found === null
  // 差别太多：整段算作换掉了
  const runs = found ?? [
    ...(a.length ? [{ op: 'del' as const, from: 0, to: 0, count: a.length }] : []),
    ...(b.length ? [{ op: 'add' as const, from: a.length, to: 0, count: b.length }] : []),
  ]
  const lines: DiffLine[] = []
  let added = 0
  let removed = 0
  let blocks = 0
  let inBlock = false
  for (let i = 0; i < runs.length; i++) {
    const r = runs[i]!
    if (r.op === 'same') {
      inBlock = false
      for (let j = 0; j < r.count; j++) {
        lines.push({ op: 'same', text: b[r.to + j]!, oldNo: r.from + j + 1, newNo: r.to + j + 1 })
      }
      continue
    }
    if (!inBlock) blocks++
    inBlock = true
    if (r.op === 'del') {
      removed += r.count
      // 紧接着就是插入的：一行对一行逐字比较（配不上对的多余几行照常显示）
      const next = runs[i + 1]
      const paired = next && next.op === 'add' ? Math.min(r.count, next.count) : 0
      const dels: DiffLine[] = []
      const adds: DiffLine[] = []
      for (let j = 0; j < r.count; j++) dels.push({ op: 'del', text: a[r.from + j]!, oldNo: r.from + j + 1 })
      if (next && next.op === 'add') {
        added += next.count
        for (let j = 0; j < next.count; j++) adds.push({ op: 'add', text: b[next.to + j]!, newNo: next.to + j + 1 })
        i++
      }
      for (let j = 0; j < paired; j++) {
        const del = dels[j]!
        const add = adds[j]!
        const chars = diffChars(del.text, add.text)
        if (chars) {
          del.pieces = chars.before
          add.pieces = chars.after
        }
      }
      lines.push(...dels, ...adds)
    } else {
      added += r.count
      for (let j = 0; j < r.count; j++) lines.push({ op: 'add', text: b[r.to + j]!, newNo: r.to + j + 1 })
    }
  }
  return { lines, added, removed, blocks, same: added === 0 && removed === 0, tooDifferent }
}

export type ShownLine = DiffLine | { op: 'gap'; count: number }

/** 只看改动：改动前后各留 context 行，中间相同的一大段折起来 */
export function withContext(lines: readonly DiffLine[], context = 2): ShownLine[] {
  const keep = new Array<boolean>(lines.length).fill(false)
  lines.forEach((l, i) => {
    if (l.op === 'same') return
    for (let j = Math.max(0, i - context); j <= Math.min(lines.length - 1, i + context); j++) keep[j] = true
  })
  const out: ShownLine[] = []
  let hidden = 0
  lines.forEach((l, i) => {
    if (keep[i]) {
      if (hidden) out.push({ op: 'gap', count: hidden })
      hidden = 0
      out.push(l)
    } else {
      hidden++
    }
  })
  if (hidden) out.push({ op: 'gap', count: hidden })
  return out
}
