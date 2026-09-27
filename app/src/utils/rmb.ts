// 金额、日期转中文大写，照中国人民银行《正确填写票据和结算凭证的基本规定》（人民银行会计司《企业、银行正确
// 办理支付结算指南》）写，规定里举的例子都一字不差（见 tests/rmb.mjs）：
// - 数字用 零壹贰叁肆伍陆柒捌玖，位用 拾佰仟、万亿，金额单位用 元角分；
// - 到「元」为止的，「元」后面写「整」；「角」后面可以不写「整」，这里不写（规定的例子也不写）；有「分」的不写「整」；
// - 数字中间有一个或者连着几个 0，写一个「零」（万位是 0 的也写，规定里两种写法都行）；
// - 元位是 0 而角位不是 0 时，规定说「零」可写可不写，这里不写；角位是 0 而分位不是 0 时，「元」后面必须写「零」；
// - 十几写成「壹拾几」，「壹」不省，免得前面被添字；
// - 金额最多到仟亿（一万亿以下）、两位小数（到分）。
// 出票日期：月为壹、贰和壹拾的，日为壹至玖和壹拾、贰拾、叁拾的，前面加「零」；日为拾壹至拾玖的，前面加「壹」。

const DIGITS = '零壹贰叁肆伍陆柒捌玖'
const PLACES = ['', '拾', '佰', '仟'] as const
const SECTIONS = ['', '万', '亿'] as const
/** 整数部分最多几位（仟亿） */
const MAX_INTEGER_DIGITS = 12

export type AmountResult = { ok: true; words: string; figures: string } | { ok: false; error: string }

/** 整数部分（没有前导零的数字串）的大写，不带「元」；0 返回空串 */
function integerWords(digits: string): string {
  let out = ''
  let zero = false
  let sectionHasDigit = false
  const length = digits.length
  for (let i = 0; i < length; i++) {
    const p = length - 1 - i
    const d = digits.charCodeAt(i) - 48
    if (d !== 0) {
      if (zero) out += '零'
      out += DIGITS.charAt(d) + (PLACES[p % 4] ?? '')
      zero = false
      sectionHasDigit = true
    } else if (out) {
      zero = true
    }
    if (p % 4 === 0 && p > 0) {
      if (sectionHasDigit) out += SECTIONS[p / 4] ?? ''
      sectionHasDigit = false
    }
  }
  return out
}

/** 小写金额的写法：¥12,345.67 */
function figuresOf(integer: string, cents: string): string {
  return `¥${integer.replace(/\B(?=(\d{3})+$)/g, ',')}.${cents}`
}

/** 把用户输入的金额转成中文大写（不带「人民币」）。允许带 ¥、逗号、空格和末尾的「元」 */
export function amountInWords(input: string): AmountResult {
  const text = input.replace(/[\s,，]/g, '').replace(/^[¥￥]/, '').replace(/元$/, '')
  if (!text) return { ok: false, error: '' }
  if (text.startsWith('-')) return { ok: false, error: '金额不能是负数。' }
  const m = /^(\d*)(?:\.(\d*))?$/.exec(text)
  if (!m || (!m[1] && !m[2])) return { ok: false, error: '只能填数字，比如 12345.67。' }
  const fraction = m[2] ?? ''
  if (fraction.length > 2) return { ok: false, error: '最多两位小数（到「分」）。' }
  const integer = (m[1] ?? '').replace(/^0+(?=\d)/, '') || '0'
  if (integer.length > MAX_INTEGER_DIGITS) return { ok: false, error: '金额太大了：最多到仟亿（一万亿以下）。' }
  const jiao = fraction.length > 0 ? fraction.charCodeAt(0) - 48 : 0
  const fen = fraction.length > 1 ? fraction.charCodeAt(1) - 48 : 0
  const figures = figuresOf(integer, `${jiao}${fen}`)

  const whole = integer === '0' ? '' : integerWords(integer)
  if (jiao === 0 && fen === 0) return { ok: true, words: `${whole || '零'}元整`, figures }
  let words = whole ? `${whole}元` : ''
  if (jiao !== 0) words += `${DIGITS.charAt(jiao)}角`
  else if (whole) words += '零'
  if (fen !== 0) words += `${DIGITS.charAt(fen)}分`
  return { ok: true, words, figures }
}

/** 月、日的大写：1 到 31 */
function dayOrMonthWords(n: number): string {
  if (n < 10) return DIGITS.charAt(n)
  const tens = Math.floor(n / 10)
  const ones = n % 10
  return `${DIGITS.charAt(tens)}拾${ones ? DIGITS.charAt(ones) : ''}`
}

/** 票据的出票日期大写，比如 2026 年 1 月 15 日 → 贰零贰陆年零壹月壹拾伍日 */
export function dateInWords(year: number, month: number, day: number): string {
  const yearWords = String(year).split('').map((c) => DIGITS.charAt(Number(c))).join('')
  const monthWords = (month === 1 || month === 2 || month === 10 ? '零' : '') + dayOrMonthWords(month)
  const dayWords = (day < 11 || day === 20 || day === 30 ? '零' : '') + dayOrMonthWords(day)
  return `${yearWords}年${monthWords}月${dayWords}日`
}
