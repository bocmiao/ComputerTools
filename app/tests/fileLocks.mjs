import assert from 'node:assert/strict'
import { test } from 'node:test'
import { filesLine, lockAdvice, lockNotes, lockSummary } from '../src/utils/fileLocks.ts'

const user = (over) => ({
  pid: 1, name: '记事本', program: 'notepad.exe', kind: 'window', service: null,
  files: ['a.txt'], moreFiles: 0, isSelf: false, otherSession: false, ...over,
})
const report = (over) => ({
  mode: 'files', targets: ['a.txt'], checked: 1, missing: [], failed: [], truncated: false, unreadable: 0, users: [], ...over,
})

test('each kind of program gets its own advice', () => {
  assert.match(lockAdvice(user({})), /在「记事本」里保存、关掉/)
  assert.match(lockAdvice(user({ kind: 'explorer' })), /重启资源管理器/)
  assert.match(lockAdvice(user({ kind: 'service', service: 'WinDefend', program: 'MsMpEng.exe' })), /安全中心正在检查/)
  assert.match(lockAdvice(user({ kind: 'service', name: '打印后台处理程序', service: 'Spooler' })), /重启电脑/)
  assert.match(lockAdvice(user({ kind: 'critical' })), /不能关掉/)
  assert.match(lockAdvice(user({ kind: 'other', name: 'sync.exe' })), /任务管理器/)
  // 另一个用户的、小药箱自己的，先说这个
  assert.match(lockAdvice(user({ otherSession: true })), /另一个登录的用户/)
  assert.match(lockAdvice(user({ isSelf: true, kind: 'other' })), /小药箱自己/)
})

test('the summary names what was checked', () => {
  assert.equal(lockSummary(report({})), '没有找到在用「a.txt」的程序。')
  assert.equal(lockSummary(report({ targets: ['a', 'b'], checked: 2, users: [user({})] })), '有 1 个程序在用这 2 个文件：')
  assert.equal(lockSummary(report({ mode: 'folder', targets: ['旧项目'], checked: 9, users: [user({}), user({ pid: 2 })] })), '有 2 个程序在用文件夹「旧项目」里的文件：')
  assert.equal(lockSummary(report({ mode: 'folder', targets: ['空的'], checked: 0 })), '文件夹「空的」里没有能查的文件。')
})

test('notes say what was left out, and file lists say how many more', () => {
  const notes = lockNotes(report({ missing: ['b.txt'], failed: ['c.txt'], truncated: true, unreadable: 2 }))
  assert.equal(notes.length, 4)
  assert.match(notes[0], /b\.txt/)
  assert.match(notes[2], /前 100 个/)
  assert.equal(filesLine(report({}), user({ files: ['a', 'b'], moreFiles: 3 })), '在用：a、b 等 5 个')
  assert.match(filesLine(report({ mode: 'folder' }), user({ files: [] })), /说不出是哪几个/)
})
