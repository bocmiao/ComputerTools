import assert from 'node:assert/strict'
import { test } from 'node:test'
import { between, countWorkdays, dayNumber, describeDate, formatDate, fromDayNumber, parseDate, shiftDate, weekdayOf } from '../src/utils/dateCalc.ts'

const n = (text) => dayNumber(parseDate(text))
const shifted = (start, count, unit) => formatDate(fromDayNumber(shiftDate(start, count, unit)))

test('dates are checked against the calendar', () => {
  assert.deepEqual(parseDate('2024-02-29'), { y: 2024, m: 2, d: 29 })
  assert.equal(parseDate('2026-02-29'), null)
  assert.equal(parseDate('2026-13-01'), null)
  assert.equal(parseDate('2026-04-31'), null)
  assert.equal(parseDate('2026/09/28'), null)
  assert.equal(parseDate(''), null)
})

test('day numbers, weekdays and descriptions', () => {
  assert.equal(n('1970-01-01'), 0)
  assert.equal(weekdayOf(0), 4) // 星期四
  assert.equal(describeDate(n('2026-09-28')), '2026 年 9 月 28 日 星期一')
  assert.equal(describeDate(n('2000-01-01')), '2000 年 1 月 1 日 星期六')
  assert.equal(describeDate(n('1969-12-31')), '1969 年 12 月 31 日 星期三')
  for (const text of ['0001-01-01', '1900-03-01', '2024-02-29', '9999-12-31']) {
    assert.equal(formatDate(fromDayNumber(n(text))), text)
  }
})

test('the difference in days, weeks and inclusive days', () => {
  const r = between('2026-09-01', '2026-10-01')
  assert.equal(r.days, 30)
  assert.equal(r.inclusive, 31)
  assert.equal(r.weeks, 4)
  assert.equal(r.weekRest, 2)
  assert.equal(between('2026-10-01', '2026-09-01').days, -30)
  assert.equal(between('2026-09-28', '2026-09-28').inclusive, 1)
  assert.equal(between('2024-01-01', '2025-01-01').days, 366)
  assert.equal(between('2026-02-30', '2026-03-01'), null)
})

test('years, months and days like an age or a contract term', () => {
  const ymd = (a, b) => {
    const r = between(a, b)
    return `${r.years}-${r.months}-${r.monthDays}`
  }
  assert.equal(ymd('1990-05-20', '2026-09-28'), '36-4-8')
  assert.equal(ymd('2026-01-31', '2026-03-01'), '0-1-1')
  assert.equal(ymd('2026-01-30', '2026-03-01'), '0-1-1')
  assert.equal(ymd('2026-01-15', '2026-03-10'), '0-1-23')
  assert.equal(ymd('2026-01-31', '2026-02-28'), '0-0-28')
  assert.equal(ymd('2026-03-31', '2026-05-01'), '0-1-1')
  assert.equal(ymd('2024-02-29', '2025-02-28'), '0-11-30')
  assert.equal(ymd('2025-12-31', '2026-01-01'), '0-0-1')
  // 前后颠倒也一样
  assert.equal(ymd('2026-09-28', '1990-05-20'), '36-4-8')
})

test('workdays are Monday to Friday, both ends included', () => {
  // 2026-09-28 是星期一
  assert.equal(countWorkdays(n('2026-09-28'), n('2026-10-04')), 5)
  assert.equal(countWorkdays(n('2026-10-03'), n('2026-10-04')), 0)
  assert.equal(countWorkdays(n('2026-09-28'), n('2026-09-28')), 1)
  assert.equal(between('2026-09-01', '2026-09-30').workdays, 22)
  // 和一天一天数的结果一样
  for (let a = n('2026-01-01'); a < n('2026-02-01'); a += 3) {
    for (let b = a; b < a + 40; b += 5) {
      let count = 0
      for (let d = a; d <= b; d++) if (weekdayOf(d) !== 0 && weekdayOf(d) !== 6) count++
      assert.equal(countWorkdays(a, b), count)
    }
  }
})

test('shifting by days and by workdays', () => {
  assert.equal(shifted('2026-09-28', 30, 'days'), '2026-10-28')
  assert.equal(shifted('2026-03-01', -1, 'days'), '2026-02-28')
  assert.equal(shifted('2024-03-01', -1, 'days'), '2024-02-29')
  // 星期一往后 7 个工作日是下下个星期三
  assert.equal(shifted('2026-09-28', 7, 'workdays'), '2026-10-07')
  // 星期五往后 1 个工作日是星期一；星期六往后 1 个工作日也是星期一
  assert.equal(shifted('2026-10-02', 1, 'workdays'), '2026-10-05')
  assert.equal(shifted('2026-10-03', 1, 'workdays'), '2026-10-05')
  // 往前：星期一往前 1 个工作日是上个星期五
  assert.equal(shifted('2026-09-28', -1, 'workdays'), '2026-09-25')
  assert.equal(shifted('2026-09-28', 0, 'workdays'), '2026-09-28')
  // 和一天一天数的结果一样
  for (let start = n('2026-09-01'); start < n('2026-09-15'); start++) {
    for (const count of [1, 4, 5, 6, 10, 23, -3, -12]) {
      let day = start
      let left = Math.abs(count)
      while (left > 0) {
        day += Math.sign(count)
        if (weekdayOf(day) !== 0 && weekdayOf(day) !== 6) left--
      }
      assert.equal(shiftDate(formatDate(fromDayNumber(start)), count, 'workdays'), day)
    }
  }
  assert.equal(shiftDate('2026-09-28', 1.5, 'days'), null)
  assert.equal(shiftDate('2026-09-28', 1e9, 'days'), null)
})
