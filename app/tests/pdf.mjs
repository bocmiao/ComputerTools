import assert from 'node:assert/strict'
import { test } from 'node:test'
import { A4, buildPdf, byName, pageLayout, scaledSize, suggestedName, turnedSize } from '../src/utils/pdf.ts'

const RULES = { page: 'a4', orientation: 'auto', margin: 'narrow', clarity: 'standard' }
const close = (a, b, what) => assert.ok(Math.abs(a - b) < 0.01, `${what}: ${a} ≠ ${b}`)

test('A4 pages: portrait photos stand, landscape ones lie down unless told otherwise', () => {
  const tall = pageLayout(3000, 4000, RULES)
  close(tall.width, 595.28, 'A4 宽')
  close(tall.height, 841.89, 'A4 高')
  // 窄白边 1 厘米：按宽放满，上下居中
  close(tall.w, 595.28 - 2 * 28.35, '图片宽')
  close(tall.h, tall.w * 4 / 3, '比例不变')
  close(tall.x, 28.35, '左边留白')
  close(tall.y, (841.89 - tall.h) / 2, '上下居中')

  const wide = pageLayout(4000, 3000, RULES)
  close(wide.width, 841.89, '横图横放')
  close(wide.h, 595.28 - 2 * 28.35, '按高放满')

  const upright = pageLayout(4000, 3000, { ...RULES, orientation: 'portrait' })
  close(upright.width, 595.28, '都竖着放')
  close(upright.w, 595.28 - 2 * 28.35, '按宽放满')
  assert.ok(upright.h < upright.w)

  const bare = pageLayout(100, 100, { ...RULES, margin: 'none' })
  close(bare.w, 595.28, '小图也放大到整页宽')
  close(bare.x, 0, '不留白')
})

test('pages shaped like the image have no margin and an A4-long long edge', () => {
  const shot = pageLayout(1080, 2400, { ...RULES, page: 'image' })
  close(shot.height, A4.height, '长边和 A4 一样长')
  close(shot.width, A4.height * 1080 / 2400, '比例和图片一样')
  assert.deepEqual([shot.x, shot.y], [0, 0])
  close(shot.w, shot.width, '没有白边')
  const sliver = pageLayout(10, 100000, { ...RULES, page: 'image' })
  assert.ok(sliver.width >= 3, '页面至少 3 点宽')
})

test('images only shrink, to the long edge of the clarity chosen', () => {
  assert.deepEqual(scaledSize(4000, 3000, 'standard'), { width: 2339, height: 1754 })
  assert.deepEqual(scaledSize(3000, 4000, 'high'), { width: 2631, height: 3508 })
  assert.deepEqual(scaledSize(4000, 3000, 'small'), { width: 1654, height: 1241 })
  assert.deepEqual(scaledSize(800, 600, 'small'), { width: 800, height: 600 }, '不放大')
  assert.deepEqual(turnedSize(4000, 3000, 1), { width: 3000, height: 4000 })
  assert.deepEqual(turnedSize(4000, 3000, 2), { width: 4000, height: 3000 })
})

// 按交叉引用表找到每个对象，核对 PDF 的结构
function parse(bytes) {
  const text = new TextDecoder('latin1').decode(bytes)
  assert.ok(text.startsWith('%PDF-1.4\n'))
  assert.ok(text.endsWith('%%EOF\n'))
  const startxref = Number(/startxref\n(\d+)\n%%EOF\n$/.exec(text)[1])
  assert.equal(text.slice(startxref, startxref + 5), 'xref\n')
  const [, first, count] = /^xref\n(\d+) (\d+)\n/.exec(text.slice(startxref))
  assert.equal(Number(first), 0)
  const table = text.slice(startxref).split('\n').slice(2, 2 + Number(count))
  const entries = text.slice(startxref + text.slice(startxref).indexOf('0000000000'), text.indexOf('trailer', startxref))
  assert.equal(entries.length, Number(count) * 20, '每一项 20 个字节')
  const objects = {}
  table.forEach((line, n) => {
    if (n === 0) return assert.equal(line, '0000000000 65535 f\r')
    const offset = Number(line.slice(0, 10))
    assert.equal(text.slice(offset, offset + `${n} 0 obj\n`.length), `${n} 0 obj\n`, `对象 ${n} 的位置`)
    objects[n] = text.slice(offset, text.indexOf('\nendobj\n', offset))
  })
  assert.match(text, new RegExp(`trailer\\n<< /Size ${count} /Root 1 0 R >>`))
  return { text, objects, count: Number(count) }
}

test('the PDF holds each JPEG as it is, one per page, and its cross-reference table adds up', () => {
  const jpegA = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 1, 2, 3, 0x0a, 0x0d, 0xff, 0xd9])
  const jpegB = new Uint8Array([0xff, 0xd8, 0xff, 0xdb, 9, 8, 7, 6, 5, 0xff, 0xd9])
  const pages = [
    { jpeg: jpegA, pixelWidth: 3000, pixelHeight: 4000, layout: pageLayout(3000, 4000, RULES) },
    { jpeg: jpegB, pixelWidth: 4000, pixelHeight: 3000, layout: pageLayout(4000, 3000, RULES) },
  ]
  const bytes = buildPdf(pages)
  const { text, objects, count } = parse(bytes)
  assert.equal(count, 9, '目录、页面树，每页三个对象，加上 0 号')
  assert.equal(objects[1], '1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>')
  assert.match(objects[2], /\/Kids \[3 0 R 6 0 R\] \/Count 2/)
  assert.match(objects[3], /\/MediaBox \[0 0 595.28 841.89\].*\/Im0 5 0 R.*\/Contents 4 0 R/)
  assert.match(objects[6], /\/MediaBox \[0 0 841.89 595.28\]/)
  // 内容流：/Length 和实际长度一样，画的位置和 pageLayout 算的一样
  const stream = /<< \/Length (\d+) >>\nstream\n(.*)\nendstream$/s.exec(objects[4])
  assert.equal(Number(stream[1]), stream[2].length)
  assert.equal(stream[2], 'q 538.58 0 0 718.11 28.35 61.89 cm /Im0 Do Q')
  // 图片：原样放进去
  for (const [n, jpeg, w, h] of [[5, jpegA, 3000, 4000], [8, jpegB, 4000, 3000]]) {
    assert.match(objects[n], new RegExp(`/Width ${w} /Height ${h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length ${jpeg.length} >>`))
    const start = text.indexOf('stream\n', text.indexOf(`${n} 0 obj\n`)) + 'stream\n'.length
    assert.deepEqual(bytes.subarray(start, start + jpeg.length), jpeg)
    assert.equal(text.slice(start + jpeg.length, start + jpeg.length + 11), '\nendstream\n')
  }
  // 第二行是二进制注释
  assert.deepEqual([...bytes.subarray(9, 15)], [0x25, 0xe2, 0xe3, 0xcf, 0xd3, 0x0a])
  assert.throws(() => buildPdf([]), /没有图片/)
})

test('numbers never use exponents and have at most two decimals', () => {
  const layout = { width: 1e-7, height: 1234567.891, x: -0.001, y: 0.005, w: 10, h: 0.1 }
  const { text } = parse(buildPdf([{ jpeg: new Uint8Array([0xff, 0xd8, 0xff, 0xd9]), pixelWidth: 1, pixelHeight: 1, layout }]))
  assert.match(text, /\/MediaBox \[0 0 0 1234567.89\]/)
  assert.match(text, /q 10 0 0 0.1 0 0.01 cm/)
  assert.doesNotMatch(text, /e[-+]\d/)
})

test('the suggested name has the date and files sort like people number them', () => {
  assert.equal(suggestedName(new Date(2026, 8, 7, 23, 59)), '图片合成 2026-09-07.pdf')
  // 中文和字母谁在前面按系统的排序规则，这里只看数字是不是按大小排、大小写是不是不分
  const sorted = ['第10页.jpg', '第2页.jpg', 'IMG_0012.JPG', 'img_0003.jpg', '第1页.jpg', 'scan 9.png'].sort(byName)
  assert.deepEqual(sorted.filter((n) => n.startsWith('第')), ['第1页.jpg', '第2页.jpg', '第10页.jpg'])
  assert.deepEqual(sorted.filter((n) => /^img/i.test(n)), ['img_0003.jpg', 'IMG_0012.JPG'])
})
