import assert from 'node:assert/strict'
import { test } from 'node:test'
import { applyTextAction, textStats, toFullWidth, toHalfWidth } from '../src/utils/textTools.ts'

test('full-width and half-width letters, digits, punctuation and spaces swap both ways', () => {
  assert.equal(toHalfWidth('ＡＢＣ１２３，！　ｘ'), 'ABC123,! x', '中文输入法打的逗号、叹号就是全角标点，也换')
  assert.equal(toHalfWidth('句号。顿号、'), '句号。顿号、', '句号、顿号没有半角的对应，不动')
  assert.equal(toHalfWidth('（Ｗｉｎｄｏｗｓ）'), '(Windows)')
  assert.equal(toFullWidth('Hi 5!'), 'Ｈｉ　５！')
  assert.equal(toHalfWidth(toFullWidth('a-b_c~')), 'a-b_c~')
})

test('line tools drop empty lines, duplicates, spaces, and sort the Chinese way', () => {
  assert.equal(applyTextAction('drop-empty', 'a\n\n  \nb\r\n'), 'a\nb')
  assert.equal(applyTextAction('dedupe', 'b\na\nb\na\nc'), 'b\na\nc')
  assert.equal(applyTextAction('trim', '  a \n\tb'), 'a\nb')
  assert.equal(applyTextAction('sort', '第10章\n第2章\n第1章'), '第1章\n第2章\n第10章')
  assert.equal(applyTextAction('sort', '香蕉\n苹果\n橙子'), '橙子\n苹果\n香蕉', '按拼音：chéng、píng、xiāng')
})

test('word counts separate Chinese characters, English words, characters and lines', () => {
  const s = textStats("电脑小药箱 is free, isn't it?\n第二行")
  assert.equal(s.chinese, 8)
  assert.equal(s.words, 4)
  assert.equal(s.lines, 2)
  assert.equal(s.chars, Array.from("电脑小药箱isfree,isn'tit?第二行").length)
  assert.equal(textStats('').lines, 0)
})
