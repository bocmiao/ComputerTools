import assert from 'node:assert/strict'
import { test } from 'node:test'
import { diffChars, diffRuns, diffTexts, withContext, MAX_EDITS } from '../src/utils/textDiff.ts'

// 按段还原两边，核对编辑脚本是对的
function apply(a, b, runs) {
  const left = []
  const right = []
  for (const r of runs) {
    if (r.op !== 'add') left.push(...a.slice(r.from, r.from + r.count))
    if (r.op !== 'del') right.push(...b.slice(r.to, r.to + r.count))
    if (r.op === 'same') assert.deepEqual(a.slice(r.from, r.from + r.count), b.slice(r.to, r.to + r.count))
  }
  assert.deepEqual(left, a)
  assert.deepEqual(right, b)
  return runs.filter((r) => r.op !== 'same').reduce((n, r) => n + r.count, 0)
}

test('the edit script rebuilds both sides and is the shortest one', () => {
  const cases = [
    ['ABCABBA', 'CBABAC', 5], // Myers 论文里的例子：最短编辑 5 步
    ['', '', 0],
    ['abc', '', 3],
    ['', 'abc', 3],
    ['abc', 'abc', 0],
    ['abcdef', 'abXdef', 2],
    ['kitten', 'sitting', 5],
  ]
  for (const [x, y, edits] of cases) {
    const a = Array.from(x)
    const b = Array.from(y)
    assert.equal(apply(a, b, diffRuns(a, b)), edits, `${x} → ${y}`)
  }
})

test('random sequences always rebuild', () => {
  let seed = 7
  const rand = (n) => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff
    return seed % n
  }
  for (let t = 0; t < 200; t++) {
    const a = Array.from({ length: rand(30) }, () => 'abc'[rand(3)])
    const b = Array.from({ length: rand(30) }, () => 'abc'[rand(3)])
    apply(a, b, diffRuns(a, b))
  }
})

test('too many edits give up instead of hanging', () => {
  const a = Array.from({ length: MAX_EDITS + 10 }, (_, i) => `a${i}`)
  const b = Array.from({ length: MAX_EDITS + 10 }, (_, i) => `b${i}`)
  assert.equal(diffRuns(a, b), null)
  const r = diffTexts(a.join('\n'), b.join('\n'))
  assert.equal(r.tooDifferent, true)
  assert.equal(r.removed, a.length)
  assert.equal(r.added, b.length)
})

test('a changed line is paired and compared character by character', () => {
  const r = diffTexts('甲方：张三\n金额：壹万元整\n日期：2026 年 9 月 1 日', '甲方：张三\n金额：壹万伍仟元整\n日期：2026 年 9 月 1 日')
  assert.equal(r.same, false)
  assert.equal(r.added, 1)
  assert.equal(r.removed, 1)
  assert.equal(r.blocks, 1)
  const del = r.lines.find((l) => l.op === 'del')
  const add = r.lines.find((l) => l.op === 'add')
  assert.equal(del.oldNo, 2)
  assert.equal(add.newNo, 2)
  assert.deepEqual(add.pieces, [
    { changed: false, text: '金额：壹万' },
    { changed: true, text: '伍仟' },
    { changed: false, text: '元整' },
  ])
  assert.deepEqual(del.pieces, [{ changed: false, text: '金额：壹万元整' }])
})

test('added and removed lines, line numbers on both sides', () => {
  const r = diffTexts('一\n二\n三\n四', '一\n三\n四\n五')
  assert.deepEqual(
    r.lines.map((l) => `${l.op}:${l.text}:${l.oldNo ?? ''}:${l.newNo ?? ''}`),
    ['same:一:1:1', 'del:二:2:', 'same:三:3:2', 'same:四:4:3', 'add:五::4'],
  )
  assert.equal(r.blocks, 2)
})

test('identical texts, line endings and a trailing newline do not count', () => {
  assert.equal(diffTexts('a\r\nb\r\n', 'a\nb').same, true)
  assert.equal(diffTexts('', '').same, true)
  assert.equal(diffTexts('a', '').removed, 1)
})

test('spaces at the ends of lines can be ignored', () => {
  assert.equal(diffTexts('  第一条\t\n第二条', '第一条\n第二条  ').same, false)
  assert.equal(diffTexts('  第一条\t\n第二条', '第一条\n第二条  ', { ignoreSpaces: true }).same, true)
  assert.equal(diffTexts('第一条　款', '第一条 款', { ignoreSpaces: true }).same, true)
})

test('characters outside the basic plane stay whole', () => {
  const c = diffChars('签字😀', '签名😀')
  assert.deepEqual(c.after, [
    { changed: false, text: '签' },
    { changed: true, text: '名' },
    { changed: false, text: '😀' },
  ])
})

test('only the changes with some context are shown', () => {
  const before = Array.from({ length: 20 }, (_, i) => `第 ${i + 1} 行`)
  const after = [...before]
  after[10] = '改过的一行'
  const shown = withContext(diffTexts(before.join('\n'), after.join('\n')).lines, 2)
  assert.deepEqual(
    shown.map((l) => (l.op === 'gap' ? `gap ${l.count}` : l.op)),
    ['gap 8', 'same', 'same', 'del', 'add', 'same', 'same', 'gap 7'],
  )
})
