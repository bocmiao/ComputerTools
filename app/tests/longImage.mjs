import assert from 'node:assert/strict'
import { test } from 'node:test'
import { MAX_AREA, MAX_SIDE, gapPixels, stitchLayout, suggestedName } from '../src/utils/longImage.ts'

const V = { direction: 'vertical', size: 'narrowest', gap: false }

/** 挨着的两张之间没有多出、也没有少一条缝 */
function contiguous(layout, vertical, gap = 0) {
  for (let i = 1; i < layout.placements.length; i++) {
    const a = layout.placements[i - 1]
    const b = layout.placements[i]
    if (vertical) assert.equal(b.y, a.y + a.height + gap, `第 ${i + 1} 张紧接着上一张`)
    else assert.equal(b.x, a.x + a.width + gap, `第 ${i + 1} 张紧接着上一张`)
  }
  const last = layout.placements.at(-1)
  if (vertical) assert.equal(last.y + last.height, layout.height, '最后一张到底')
  else assert.equal(last.x + last.width, layout.width, '最后一张到头')
}

test('same-size phone screenshots stack up exactly', () => {
  const l = stitchLayout([{ width: 1080, height: 2400 }, { width: 1080, height: 2400 }, { width: 1080, height: 1200 }], V)
  assert.deepEqual([l.width, l.height, l.scale], [1080, 6000, 1])
  assert.deepEqual(l.placements.map((p) => [p.x, p.y, p.width, p.height]), [[0, 0, 1080, 2400], [0, 2400, 1080, 2400], [0, 4800, 1080, 1200]])
})

test('widths are unified to the narrowest or the widest, keeping proportions', () => {
  const sizes = [{ width: 1000, height: 1000 }, { width: 500, height: 250 }]
  const narrow = stitchLayout(sizes, V)
  assert.deepEqual([narrow.width, narrow.height], [500, 750])
  assert.deepEqual(narrow.placements[0], { x: 0, y: 0, width: 500, height: 500 })
  assert.deepEqual(narrow.placements[1], { x: 0, y: 500, width: 500, height: 250 })

  const wide = stitchLayout(sizes, { ...V, size: 'widest' })
  assert.deepEqual([wide.width, wide.height], [1000, 1500])
  assert.deepEqual(wide.placements[1], { x: 0, y: 1000, width: 1000, height: 500 })
})

test('"keep" leaves every image as it is and centres the narrow ones', () => {
  const l = stitchLayout([{ width: 1000, height: 800 }, { width: 500, height: 300 }], { ...V, size: 'keep' })
  assert.deepEqual([l.width, l.height], [1000, 1100])
  assert.deepEqual(l.placements[1], { x: 250, y: 800, width: 500, height: 300 })
})

test('a white gap goes between images, not around them', () => {
  assert.equal(gapPixels(1080), 22)
  assert.equal(gapPixels(100), 8, '至少 8 像素')
  assert.equal(gapPixels(5000), 40, '最多 40 像素')
  const l = stitchLayout([{ width: 1080, height: 2400 }, { width: 1080, height: 2400 }], { ...V, gap: true })
  assert.equal(l.height, 2400 * 2 + 22)
  contiguous(l, true, 22)
  const one = stitchLayout([{ width: 1080, height: 2400 }], { ...V, gap: true })
  assert.equal(one.height, 2400, '只有一张时没有缝')
})

test('side by side unifies heights', () => {
  const l = stitchLayout([{ width: 1000, height: 500 }, { width: 400, height: 400 }], { ...V, direction: 'horizontal' })
  assert.deepEqual([l.width, l.height], [1200, 400])
  assert.deepEqual(l.placements.map((p) => [p.x, p.y, p.width, p.height]), [[0, 0, 800, 400], [800, 0, 400, 400]])
  const kept = stitchLayout([{ width: 1000, height: 500 }, { width: 400, height: 300 }], { direction: 'horizontal', size: 'keep', gap: false })
  assert.deepEqual(kept.placements[1], { x: 1000, y: 100, width: 400, height: 300 }, '矮的上下居中')
})

test('too long: the whole image is scaled down without seams', () => {
  const sizes = Array.from({ length: 20 }, () => ({ width: 1080, height: 2400 }))
  const l = stitchLayout(sizes, V)
  assert.ok(l.scale < 1)
  assert.ok(l.height <= MAX_SIDE, `${l.height}`)
  assert.equal(l.width, 720)
  contiguous(l, true)

  const odd = stitchLayout(Array.from({ length: 37 }, (_, i) => ({ width: 1179, height: 1000 + i * 37 })), { ...V, gap: true })
  assert.ok(odd.height <= MAX_SIDE && odd.width * odd.height <= MAX_AREA * 1.001)
  for (let i = 1; i < odd.placements.length; i++) {
    const a = odd.placements[i - 1]
    const b = odd.placements[i]
    assert.ok(b.y > a.y + a.height, '缩小以后缝还在')
  }
})

test('too big in area: scaled to about 50 million pixels', () => {
  const l = stitchLayout([{ width: 8000, height: 8000 }, { width: 8000, height: 8000 }], { ...V, size: 'keep' })
  assert.ok(Math.abs(l.width * l.height - MAX_AREA) / MAX_AREA < 0.01, `${l.width}×${l.height}`)
  assert.deepEqual([l.width, l.height], [5000, 10000])
})

test('nothing to stitch', () => {
  assert.deepEqual(stitchLayout([], V), { width: 0, height: 0, placements: [], scale: 1 })
})

test('suggested names carry the date and the right extension', () => {
  const day = new Date(2026, 0, 5, 23, 59)
  assert.equal(suggestedName(day, 'jpeg'), '长图 2026-01-05.jpg')
  assert.equal(suggestedName(day, 'png'), '长图 2026-01-05.png')
})
