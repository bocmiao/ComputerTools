import assert from 'node:assert/strict'
import { test } from 'node:test'
import { amountInWords, dateInWords } from '../src/utils/rmb.ts'

const words = (input) => {
  const r = amountInWords(input)
  assert.ok(r.ok, `${input}: ${r.error}`)
  return r.words
}

test('the examples in the People’s Bank rules come out word for word', () => {
  // 《正确填写票据和结算凭证的基本规定》第四条里的例子（第三条的两种写法里取「万位的零写、元后的零不写」那一种）
  assert.equal(words('1409.50'), '壹仟肆佰零玖元伍角')
  assert.equal(words('6007.14'), '陆仟零柒元壹角肆分')
  assert.equal(words('1680.32'), '壹仟陆佰捌拾元叁角贰分')
  assert.equal(words('107000.53'), '壹拾万零柒仟元伍角叁分')
  assert.equal(words('16409.02'), '壹万陆仟肆佰零玖元零贰分')
  assert.equal(words('325.04'), '叁佰贰拾伍元零肆分')
})

test('whole amounts end in 整, zeros in the middle are written once', () => {
  assert.equal(words('0'), '零元整')
  assert.equal(words('100'), '壹佰元整')
  assert.equal(words('15'), '壹拾伍元整')
  assert.equal(words('10'), '壹拾元整')
  assert.equal(words('1010'), '壹仟零壹拾元整')
  assert.equal(words('1000000'), '壹佰万元整')
  assert.equal(words('1010000'), '壹佰零壹万元整')
  assert.equal(words('10000001'), '壹仟万零壹元整')
  assert.equal(words('100010000'), '壹亿零壹万元整')
  assert.equal(words('100000001'), '壹亿零壹元整')
  assert.equal(words('200000000000'), '贰仟亿元整')
  assert.equal(words('999999999999.99'), '玖仟玖佰玖拾玖亿玖仟玖佰玖拾玖万玖仟玖佰玖拾玖元玖角玖分')
})

test('amounts under one yuan and with 角 only', () => {
  assert.equal(words('0.05'), '伍分')
  assert.equal(words('0.5'), '伍角')
  assert.equal(words('.35'), '叁角伍分')
  assert.equal(words('10.5'), '壹拾元伍角')
  assert.equal(words('1.00'), '壹元整')
  assert.equal(words('20.07'), '贰拾元零柒分')
})

test('the input may carry ¥, commas, spaces and 元; figures are written back', () => {
  const r = amountInWords(' ￥12,345.6 元')
  assert.deepEqual(r, { ok: true, words: '壹万贰仟叁佰肆拾伍元陆角', figures: '¥12,345.60' })
  assert.equal(amountInWords('007').figures, '¥7.00')
  assert.equal(amountInWords('1234567').figures, '¥1,234,567.00')
})

test('bad amounts are explained', () => {
  assert.deepEqual(amountInWords('  '), { ok: false, error: '' })
  assert.match(amountInWords('-5').error, /负数/)
  assert.match(amountInWords('1.234').error, /两位小数/)
  assert.match(amountInWords('12a').error, /只能填数字/)
  assert.match(amountInWords('.').error, /只能填数字/)
  assert.match(amountInWords('1000000000000').error, /太大/)
})

test('dates follow the rules for cheques', () => {
  // 规定第六条的例子
  assert.equal(dateInWords(2026, 1, 15), '贰零贰陆年零壹月壹拾伍日')
  assert.equal(dateInWords(2026, 10, 20), '贰零贰陆年零壹拾月零贰拾日')
  assert.equal(dateInWords(2026, 2, 1), '贰零贰陆年零贰月零壹日')
  assert.equal(dateInWords(2026, 3, 10), '贰零贰陆年叁月零壹拾日')
  assert.equal(dateInWords(2026, 11, 11), '贰零贰陆年壹拾壹月壹拾壹日')
  assert.equal(dateInWords(2026, 12, 30), '贰零贰陆年壹拾贰月零叁拾日')
  assert.equal(dateInWords(2026, 9, 31), '贰零贰陆年玖月叁拾壹日')
  assert.equal(dateInWords(2030, 5, 29), '贰零叁零年伍月贰拾玖日')
})
