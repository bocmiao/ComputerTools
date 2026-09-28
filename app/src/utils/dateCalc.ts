// 日期计算：两个日期相差多久、从某天往后或往前推几天（几个工作日）是哪天。
// 按日历上的日期算（用 UTC 的整天数，不受时区、夏令时影响）。
// 工作日只按周一到周五算：法定节假日和调休每年由国务院另行公布，这里没有算进去，界面上写明。

export interface CalendarDate {
  y: number
  m: number
  d: number
}

export const WEEKDAYS = ['星期日', '星期一', '星期二', '星期三', '星期四', '星期五', '星期六'] as const

const DAY = 86_400_000

/** 「YYYY-MM-DD」（日期输入框的格式）；不是真实存在的日期时返回 null */
export function parseDate(text: string): CalendarDate | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(text.trim())
  if (!m) return null
  const date = { y: Number(m[1]), m: Number(m[2]), d: Number(m[3]) }
  if (date.m < 1 || date.m > 12 || date.d < 1 || date.d > daysInMonth(date.y, date.m)) return null
  return date
}

export function daysInMonth(y: number, m: number): number {
  return new Date(Date.UTC(y, m, 0)).getUTCDate()
}

/** 从 1970-01-01 起的第几天 */
export function dayNumber(date: CalendarDate): number {
  const t = new Date(0)
  t.setUTCFullYear(date.y, date.m - 1, date.d)
  return Math.round(t.getTime() / DAY)
}

export function fromDayNumber(n: number): CalendarDate {
  const t = new Date(n * DAY)
  return { y: t.getUTCFullYear(), m: t.getUTCMonth() + 1, d: t.getUTCDate() }
}

/** 0 = 星期日 */
export function weekdayOf(n: number): number {
  return (((n + 4) % 7) + 7) % 7 // 1970-01-01 是星期四
}

export function isWorkday(n: number): boolean {
  const w = weekdayOf(n)
  return w !== 0 && w !== 6
}

const pad = (n: number): string => String(n).padStart(2, '0')

/** 「YYYY-MM-DD」 */
export function formatDate(date: CalendarDate): string {
  return `${String(date.y).padStart(4, '0')}-${pad(date.m)}-${pad(date.d)}`
}

/** 「2026 年 9 月 28 日 星期一」 */
export function describeDate(n: number): string {
  const date = fromDayNumber(n)
  return `${date.y} 年 ${date.m} 月 ${date.d} 日 ${WEEKDAYS[weekdayOf(n)]}`
}

/** [first, last] 里（首尾都算）周一到周五有几天 */
export function countWorkdays(first: number, last: number): number {
  if (last < first) return 0
  const total = last - first + 1
  let count = Math.floor(total / 7) * 5
  for (let n = first + Math.floor(total / 7) * 7; n <= last; n++) if (isWorkday(n)) count++
  return count
}

export interface Between {
  /** 后一个日期减前一个日期（结束比开始早时是负数） */
  days: number
  /** 首尾都算的天数 */
  inclusive: number
  weeks: number
  weekRest: number
  /** 几年几个月几天（像算年龄、合同期限那样） */
  years: number
  months: number
  monthDays: number
  /** 首尾都算，周一到周五有几天 */
  workdays: number
}

/** 两个日期相差多久；有一个不是日期时返回 null */
export function between(startText: string, endText: string): Between | null {
  const start = parseDate(startText)
  const end = parseDate(endText)
  if (!start || !end) return null
  const a = dayNumber(start)
  const b = dayNumber(end)
  const [from, to] = a <= b ? [start, end] : [end, start]
  const span = Math.abs(b - a)
  let years = to.y - from.y
  let months = to.m - from.m
  let monthDays = to.d - from.d
  if (monthDays < 0) {
    // 借上个月的天数（to 的上一个月），像 1 月 31 日到 3 月 1 日是 1 个月 1 天（2 月不够 31 天时按月底算）
    months--
    const prevMonth = to.m === 1 ? 12 : to.m - 1
    const prevYear = to.m === 1 ? to.y - 1 : to.y
    monthDays += Math.max(daysInMonth(prevYear, prevMonth), from.d)
  }
  if (months < 0) {
    years--
    months += 12
  }
  return {
    days: b - a,
    inclusive: span + 1,
    weeks: Math.floor(span / 7),
    weekRest: span % 7,
    years,
    months,
    monthDays,
    workdays: countWorkdays(Math.min(a, b), Math.max(a, b)),
  }
}

export type Unit = 'days' | 'workdays'

/** 从某天往后（count 为正）或往前（为负）推几天、几个工作日；超出范围时返回 null */
export function shiftDate(startText: string, count: number, unit: Unit): number | null {
  const start = parseDate(startText)
  if (!start || !Number.isInteger(count) || Math.abs(count) > 100_000) return null
  const n = dayNumber(start)
  if (unit === 'days') return n + count
  const step = count < 0 ? -1 : 1
  // 整周一次跳过 5 个工作日，零头一天一天数
  let day = n
  let left = Math.abs(count)
  while (left >= 5 && isWorkday(day)) {
    day += 7 * step
    left -= 5
  }
  while (left > 0) {
    day += step
    if (isWorkday(day)) left--
  }
  return day
}
