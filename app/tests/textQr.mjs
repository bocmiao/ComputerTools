import assert from 'node:assert/strict'
import { test } from 'node:test'
import { encodeQr } from '../src/utils/qrcode.ts'
import { QR_MAX_BYTES, qrHint, qrPixels, qrText, utf8Bytes } from '../src/utils/textQr.ts'

test('the text is trimmed and counted in UTF-8 bytes', () => {
  assert.equal(qrText('  https://example.com/a\r\n'), 'https://example.com/a')
  assert.equal(utf8Bytes('取件码ab12'), 13)
})

test('the most that is allowed still fits a version a phone can scan from a screen', () => {
  const longest = encodeQr('汉'.repeat(QR_MAX_BYTES / 3))
  assert.equal(longest.version, 24)
  // 每格大约 3 像素
  assert.equal(qrPixels(longest.size), 360)
})

test('small codes are drawn at the smallest size, dense ones larger', () => {
  assert.equal(qrPixels(21), 220)
  assert.equal(qrPixels(77), 255)
  assert.equal(qrPixels(177), 360)
})

test('hints: too long, and a web address without https://', () => {
  assert.equal(qrHint('https://www.baidu.com'), null)
  assert.equal(qrHint('取件码 1234'), null)
  assert.equal(qrHint('a'.repeat(QR_MAX_BYTES)), null)
  const long = qrHint('a'.repeat(QR_MAX_BYTES + 1))
  assert.equal(long?.tone, 'danger')
  assert.match(long?.text ?? '', /901 字节/)
  assert.equal(qrHint('www.baidu.com/s?wd=天气')?.tone, 'muted')
  assert.equal(qrHint('www.baidu.com 和 www.qq.com'), null)
})
