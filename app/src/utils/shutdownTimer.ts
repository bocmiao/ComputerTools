// 「定时关机」：算出要等多少秒，和界面上的说法。
import type { ShutdownPlan } from '../api/types'

export const MAX_MINUTES = 24 * 60

/** 「多久以后」的几个常用选择（分钟） */
export const PRESET_MINUTES = [30, 60, 120, 180] as const

export type Delay = { seconds: number; at: Date } | { error: string }

/** 多久以后（分钟） */
export function delayAfter(minutes: number, now: Date): Delay {
  if (!Number.isInteger(minutes) || minutes < 1 || minutes > MAX_MINUTES) {
    return { error: '分钟数要在 1 到 1440（24 小时）之间。' }
  }
  return { seconds: minutes * 60, at: new Date(now.getTime() + minutes * 60_000) }
}

/** 到几点（"HH:MM"）：今天这个时间已经过了、或者不到 1 分钟了，就是明天的这个时间 */
export function delayUntil(time: string, now: Date): Delay {
  const m = /^(\d{1,2}):(\d{2})$/.exec(time)
  const [hours, minutes] = m ? [Number(m[1]), Number(m[2])] : [NaN, NaN]
  if (!(hours <= 23 && minutes <= 59)) return { error: '请选一个时间，例如 23:30。' }
  const at = new Date(now)
  at.setHours(hours, minutes, 0, 0)
  if (at.getTime() - now.getTime() < 60_000) at.setDate(at.getDate() + 1)
  return { seconds: Math.round((at.getTime() - now.getTime()) / 1000), at }
}

const pad = (n: number): string => String(n).padStart(2, '0')

/** 「今天 23:30」「明天 07:00」 */
export function whenText(at: Date, now: Date): string {
  const time = `${pad(at.getHours())}:${pad(at.getMinutes())}`
  const day = (d: Date): number => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()
  const days = Math.round((day(at) - day(now)) / 86_400_000)
  if (days === 0) return `今天 ${time}`
  if (days === 1) return `明天 ${time}`
  return `${at.getMonth() + 1} 月 ${at.getDate()} 日 ${time}`
}

/** 「1 小时 5 分钟」「不到 1 分钟」：不足 1 分钟的部分算 1 分钟 */
export function durationText(ms: number): string {
  if (ms < 60_000) return '不到 1 分钟'
  const total = Math.ceil(ms / 60_000)
  const hours = Math.floor(total / 60)
  const minutes = total % 60
  return [hours ? `${hours} 小时` : '', minutes ? `${minutes} 分钟` : ''].filter(Boolean).join(' ')
}

/** 还没安排时，按现在选的算出来的时间 */
export function previewText(delay: Delay, restart: boolean, now: Date): string {
  if ('error' in delay) return delay.error
  return `会在${whenText(delay.at, now)} ${restart ? '重启' : '关机'}（${durationText(delay.seconds * 1000)}以后）。`
}

/** 已经安排好的那一次 */
export function planText(plan: ShutdownPlan, now: Date): string {
  const what = plan.restart ? '重启' : '关机'
  const left = plan.at - now.getTime()
  if (left <= 0) return `到时间了，电脑正在${what}。`
  return `已经安排好：${whenText(new Date(plan.at), now)} ${what}（还有 ${durationText(left)}）。`
}

/** 取消以后的说明 */
export function cancelText(cancelled: boolean): string {
  return cancelled ? '取消了：到时间不会关机，也不会重启。' : '现在没有安排关机或重启（可能已经在别处取消了）。'
}
