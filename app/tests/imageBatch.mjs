import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  drawPlan,
  fitToSize,
  formatBytes,
  keepsSize,
  matchesFormat,
  outputMime,
  outputName,
  sameFormat,
  suffixProblem,
  unsupportedReason,
  watermarkPlan,
} from '../src/utils/imageBatch.ts'

test('HEIC, TIFF, SVG and non-images are explained, common images pass', () => {
  assert.match(unsupportedReason('IMG_0001.HEIC', ''), /苹果手机/)
  assert.match(unsupportedReason('a.heif', 'image/heif'), /HEIC/)
  assert.match(unsupportedReason('scan.tif', 'image/tiff'), /TIFF/)
  assert.match(unsupportedReason('logo.svg', 'image/svg+xml'), /矢量图/)
  assert.match(unsupportedReason('说明.txt', 'text/plain'), /不是图片/)
  for (const [name, type] of [['a.jpg', 'image/jpeg'], ['b.PNG', 'image/png'], ['c.webp', ''], ['d.gif', 'image/gif'], ['e.bmp', 'image/bmp']]) {
    assert.equal(unsupportedReason(name, type), null, name)
  }
})

test('keep format falls back to PNG for formats the canvas cannot write', () => {
  assert.equal(outputMime('image/jpeg', 'keep'), 'image/jpeg')
  assert.equal(outputMime('image/jpg', 'keep'), 'image/jpeg')
  assert.equal(outputMime('image/webp', 'keep'), 'image/webp')
  assert.equal(outputMime('image/gif', 'keep'), 'image/png')
  assert.equal(outputMime('image/bmp', 'keep'), 'image/png')
  assert.equal(outputMime('image/png', 'jpeg'), 'image/jpeg')
  assert.equal(outputMime('image/jpeg', 'webp'), 'image/webp')
})

test('output names keep the original name and change only the extension', () => {
  assert.equal(outputName('IMG_0001.JPG', 'image/jpeg', ''), 'IMG_0001.jpg')
  assert.equal(outputName('海边.png', 'image/webp', '_压缩'), '海边_压缩.webp')
  assert.equal(outputName('archive.tar.png', 'image/png', ''), 'archive.tar.png')
  assert.equal(outputName('noext', 'image/jpeg', ''), 'noext.jpg')
  assert.equal(outputName('.png', 'image/png', ''), '.png.png')
  assert.equal(outputName('trailing. .png', 'image/png', ''), 'trailing.png')
  assert.equal(suffixProblem('_小'), null)
  assert.match(suffixProblem('a/b'), /不允许/)
  assert.match(suffixProblem('a:b'), /不允许/)
})

test('long edge never enlarges; fixed size crops the middle', () => {
  assert.deepEqual(drawPlan(4000, 3000, { resize: 'long-edge', longEdge: 1920, width: 0, height: 0 }), {
    sx: 0, sy: 0, sw: 4000, sh: 3000, width: 1920, height: 1440,
  })
  const small = drawPlan(800, 600, { resize: 'long-edge', longEdge: 1920, width: 0, height: 0 })
  assert.equal(small.width, 800)
  assert.ok(keepsSize(small, 800, 600))
  // 3:4 的竖图（3000×4000）裁成一寸照（295×413，比 3:4 还瘦）：高度用满，左右各裁掉一点
  const id = drawPlan(3000, 4000, { resize: 'fixed', longEdge: 0, width: 295, height: 413 })
  assert.equal(id.width, 295)
  assert.equal(id.height, 413)
  assert.equal(id.sy, 0)
  assert.equal(id.sh, 4000)
  assert.ok(Math.abs(id.sw - (4000 * 295) / 413) < 1e-6)
  assert.ok(Math.abs(id.sx - (3000 - id.sw) / 2) < 1e-9)
  // 比目标还瘦的图：宽度用满，上下裁掉
  const tall = drawPlan(1000, 3000, { resize: 'fixed', longEdge: 0, width: 295, height: 413 })
  assert.equal(tall.sw, 1000)
  assert.ok(tall.sy > 0 && tall.sh < 3000)
  // 横图裁成竖的：左右裁掉
  const wide = drawPlan(4000, 3000, { resize: 'fixed', longEdge: 0, width: 413, height: 579 })
  assert.equal(wide.sh, 3000)
  assert.ok(wide.sw < 4000 && wide.sx > 0)
  assert.ok(!keepsSize(wide, 4000, 3000))
  assert.ok(keepsSize(drawPlan(10, 20, { resize: 'none', longEdge: 0, width: 0, height: 0 }), 10, 20))
})

// 假的编码器：大小和质量、像素数成正比
function fakeEncoder(bytesAtFull, lossy = true) {
  const calls = []
  const encode = async (quality, scale) => {
    calls.push([quality, scale])
    const size = Math.round(bytesAtFull * (lossy ? quality : 1) * scale * scale)
    return { size, data: `${quality.toFixed(3)}@${scale.toFixed(3)}` }
  }
  return { encode, calls }
}

test('fits under the limit with the highest quality that works', async () => {
  const { encode } = fakeEncoder(1_000_000)
  const fitted = await fitToSize(encode, 600_000, 0.85, true, 4000, 3000)
  assert.ok(fitted && fitted.size <= 600_000)
  assert.equal(fitted.scale, 1)
  assert.ok(fitted.quality > 0.55 && fitted.quality <= 0.6, `quality ${fitted.quality}`)
})

test('already small enough is encoded once', async () => {
  const { encode, calls } = fakeEncoder(100_000)
  const fitted = await fitToSize(encode, 200_000, 0.85, true, 800, 600)
  assert.equal(calls.length, 1)
  assert.equal(fitted.quality, 0.85)
})

test('shrinks the picture when even the lowest quality is too big', async () => {
  const { encode } = fakeEncoder(10_000_000)
  const fitted = await fitToSize(encode, 200_000, 0.85, true, 4000, 3000)
  assert.ok(fitted && fitted.size <= 200_000)
  assert.ok(fitted.scale < 1)
})

test('PNG can only be shrunk, and a hopeless target gives up', async () => {
  const png = fakeEncoder(3_000_000, false)
  const fitted = await fitToSize(png.encode, 500_000, 0.85, false, 2000, 1500)
  assert.ok(fitted && fitted.size <= 500_000 && fitted.scale < 1)
  const hopeless = fakeEncoder(3_000_000, false)
  assert.equal(await fitToSize(hopeless.encode, 10, 0.85, false, 200, 150), null)
  assert.ok(hopeless.calls.length <= 9)
})

test('sizes read naturally', () => {
  assert.equal(formatBytes(512), '512 B')
  assert.equal(formatBytes(2048), '2.0 KB')
  assert.equal(formatBytes(200 * 1024), '200 KB')
  assert.equal(formatBytes(5 * 1024 * 1024), '5.0 MB')
})

test('the original is copied only when its content really is that format', () => {
  const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0xe0])
  const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13])
  const webp = new TextEncoder().encode('RIFF\u0010\u0000\u0000\u0000WEBPVP8 ')
  assert.ok(matchesFormat(jpeg, 'image/jpeg'))
  assert.ok(!matchesFormat(png, 'image/jpeg'))
  assert.ok(matchesFormat(png, 'image/png'))
  assert.ok(matchesFormat(webp, 'image/webp'))
  assert.ok(!matchesFormat(new TextEncoder().encode('RIFF....WAVE'), 'image/webp'))
  assert.ok(!matchesFormat(new Uint8Array(), 'image/jpeg'))
  assert.ok(sameFormat('image/jpg', 'image/jpeg'))
  assert.ok(!sameFormat('image/png', 'image/jpeg'))
})

test('watermarks follow the short edge and reach every corner', () => {
  const measure = (size) => size * 12 // 12 个汉字
  const photo = watermarkPlan(4000, 3000, measure)
  assert.equal(photo.fontSize, 188, '短边 3000 的 1/16')
  assert.equal(photo.stepX, Math.ceil(188 * 12 + 188 * 3))
  assert.equal(photo.stepY, 188 * 4)
  assert.equal(photo.reach, 2500, '对角线的一半：转了角度也铺得到四个角')
  assert.ok(photo.angle < 0 && photo.angle > -Math.PI / 2, '往右上斜')
  // 一寸照这么小，字也不小于 14 像素
  assert.equal(watermarkPlan(295, 413, measure).fontSize, 18)
  assert.equal(watermarkPlan(100, 100, measure).fontSize, 14)
})
