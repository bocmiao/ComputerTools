import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  cancelText,
  delayAfter,
  delayUntil,
  durationText,
  planText,
  previewText,
  whenText,
} from '../src/utils/shutdownTimer.ts'

// 本地时间 2026-09-27 22:40:30
const now = new Date(2026, 8, 27, 22, 40, 30)

test('minutes from now', () => {
  const d = delayAfter(90, now)
  assert.equal(d.seconds, 5400)
  assert.equal(d.at.getTime(), now.getTime() + 5_400_000)
  for (const bad of [0, -5, 1.5, 1441, NaN]) assert.ok('error' in delayAfter(bad, now), String(bad))
  assert.equal(delayAfter(1440, now).seconds, 86_400)
})

test('a clock time later today, or tomorrow when it has passed', () => {
  const today = delayUntil('23:30', now)
  assert.equal(today.seconds, 49 * 60 + 30)
  assert.equal(whenText(today.at, now), '今天 23:30')
  const tomorrow = delayUntil('07:00', now)
  assert.equal(whenText(tomorrow.at, now), '明天 07:00')
  assert.equal(tomorrow.seconds, (8 * 60 + 19) * 60 + 30)
  // 不到 1 分钟了：明天的这个时间（后端最多接受 24 小时加 60 秒）
  const soon = delayUntil('22:41', now)
  assert.equal(whenText(soon.at, now), '明天 22:41')
  assert.ok(soon.seconds <= 24 * 3600 + 60)
  assert.equal(delayUntil('22:42', now).seconds, 90)
  for (const bad of ['', '24:00', '7:5', '12:60', 'abc']) assert.ok('error' in delayUntil(bad, now), bad)
})

test('durations round up to whole minutes', () => {
  assert.equal(durationText(59_000), '不到 1 分钟')
  assert.equal(durationText(60_000), '1 分钟')
  assert.equal(durationText(3_599_900), '1 小时')
  assert.equal(durationText(3_900_000), '1 小时 5 分钟')
  assert.equal(durationText(86_400_000), '24 小时')
})

test('what the card says', () => {
  assert.equal(previewText(delayAfter(30, now), false, now), '会在今天 23:10 关机（30 分钟以后）。')
  assert.equal(previewText(delayUntil('07:00', now), true, now), '会在明天 07:00 重启（8 小时 20 分钟以后）。')
  assert.equal(previewText(delayAfter(0, now), false, now), '分钟数要在 1 到 1440（24 小时）之间。')
  const plan = { at: now.getTime() + 65 * 60_000, restart: false }
  assert.equal(planText(plan, now), '已经安排好：今天 23:45 关机（还有 1 小时 5 分钟）。')
  assert.equal(planText({ ...plan, restart: true }, new Date(plan.at + 1000)), '到时间了，电脑正在重启。')
  assert.match(cancelText(true), /取消了/)
  assert.match(cancelText(false), /没有安排/)
})
