import assert from 'node:assert/strict'
import { test } from 'node:test'
import { canReveal, ownerAdvice, ownerLinks, ownerName, ownerSummary } from '../src/utils/windowOwner.ts'

const base = {
  kind: 'program', exe: 'KanTuNews.exe', description: '热点资讯', company: '示例网络科技有限公司', product: '快看图',
  folder: 'C:\\Program Files (x86)\\KanTu\\News', installed: '快看图', publisher: '示例网络科技有限公司',
  position: 'bottom-right', width: 360, height: 260,
}
const empty = { ...base, exe: null, description: null, company: null, product: null, folder: null, installed: null, publisher: null }

test('a program popup is named with where it is and how big', () => {
  assert.equal(ownerSummary(base), '鼠标指着屏幕右下角一个 360 × 260 的窗口，是这个程序弹的：「热点资讯」。')
  const advice = ownerAdvice(base)
  assert.match(advice[0], /属于已安装的软件「快看图」（示例网络科技有限公司）/)
  assert.ok(advice.some((a) => a.includes('卸载')))
  assert.ok(advice.some((a) => a.includes('启动应用')))
  assert.deepEqual(ownerLinks(base), ['tool:settings.apps', 'tool:settings.startup-apps'])
  assert.equal(canReveal(base), true)
})

test('names fall back to the product and the file name', () => {
  assert.equal(ownerName({ ...base, description: null }), '快看图')
  assert.equal(ownerName({ ...base, description: null, product: null }), 'KanTuNews.exe')
  assert.equal(ownerName(empty), '没有名字的程序')
  const loose = { ...base, installed: null, publisher: null }
  assert.match(ownerAdvice(loose)[0], /没对上是哪个软件/)
  assert.ok(ownerAdvice(loose).every((a) => !a.includes('「快看图」')))
})

test('full screen, no size and no position read naturally', () => {
  assert.equal(ownerSummary({ ...base, position: 'full' }), '鼠标指着一个铺满整个屏幕的窗口，是这个程序弹的：「热点资讯」。')
  assert.equal(ownerSummary({ ...base, width: 0, height: 0 }), '鼠标指着屏幕右下角一个窗口，是这个程序弹的：「热点资讯」。')
  assert.equal(ownerSummary({ ...base, position: null, width: 0 }), '鼠标指着一个窗口，是这个程序弹的：「热点资讯」。')
})

test('notifications, Windows itself and the other cases', () => {
  const note = { ...base, kind: 'notification', exe: 'ShellExperienceHost.exe', installed: null, position: 'bottom-right', width: 380, height: 150 }
  assert.match(ownerSummary(note), /屏幕右下角一个 380 × 150 的 Windows 通知/)
  assert.ok(ownerAdvice(note).some((a) => a.includes('浏览器')))
  assert.deepEqual(ownerLinks(note), ['tool:settings.notifications'])
  assert.equal(canReveal(note), false)

  const sys = { ...base, kind: 'system', exe: 'notepad.exe', description: '记事本', installed: null }
  assert.match(ownerSummary(sys), /Windows 自带的程序「记事本」/)
  assert.ok(ownerAdvice(sys).some((a) => a.includes('推荐和广告')))
  assert.equal(canReveal(sys), true)
  assert.deepEqual(ownerLinks(sys), [])

  for (const kind of ['nothing', 'shell', 'medkit', 'unreadable']) {
    const r = { ...empty, kind }
    assert.ok(ownerSummary(r).length > 0, kind)
    assert.equal(canReveal(r), false, kind)
    assert.deepEqual(ownerLinks(r), [], kind)
  }
  assert.match(ownerSummary({ ...empty, kind: 'medkit' }), /小药箱的窗口/)
  assert.deepEqual(ownerAdvice({ ...empty, kind: 'medkit' }), [])
  assert.equal(ownerAdvice({ ...empty, kind: 'nothing' }).length, 1)
})
