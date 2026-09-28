import assert from 'node:assert/strict'
import { test } from 'node:test'
import { defaultTarget, planRecovery } from '../src/utils/fileRecovery.ts'

const GB = 1024 ** 3
const drives = [
  { letter: 'C', label: '', fileSystem: 'NTFS', removable: false, system: true, free: 30 * GB },
  { letter: 'D', label: '资料', fileSystem: 'NTFS', removable: false, system: false, free: 200 * GB },
  { letter: 'E', label: 'U盘', fileSystem: 'exFAT', removable: true, system: false, free: 20 * GB },
]
const choice = { source: 'C', target: 'D', kinds: ['documents'], name: '', thorough: false }

test('documents deleted from C are looked for quickly and saved to another drive', () => {
  const p = planRecovery(drives, choice)
  assert.equal(p.problem, '')
  assert.equal(p.extensive, false)
  assert.match(p.command, /^winfr C: D: \/regular \/n \*\.doc \/n \*\.docx /)
  assert.ok(p.command.includes('/n *.wps') && p.command.includes('/n *.pdf'))
  assert.ok(p.notes.some((n) => n.includes('系统盘')))
})

test('a USB drive (exFAT) always gets the thorough mode', () => {
  const p = planRecovery(drives, { ...choice, source: 'E', target: 'D', kinds: ['photos'] })
  assert.equal(p.extensive, true)
  assert.match(p.command, /^winfr E: D: \/extensive \/n \*\.jpg/)
  assert.ok(p.notes.some((n) => n.includes('exFAT')))
  const thorough = planRecovery(drives, { ...choice, thorough: true })
  assert.match(thorough.command, /\/extensive/)
})

test('part of the name narrows the search, with or without types', () => {
  const only = planRecovery(drives, { ...choice, kinds: [], name: '合同' })
  assert.equal(only.command, 'winfr C: D: /regular /n *合同*')
  const typed = planRecovery(drives, { ...choice, kinds: ['archives'], name: '年终 总结' })
  assert.ok(typed.command.includes('/n "*年终 总结*.zip"'))
  assert.match(planRecovery(drives, { ...choice, name: 'a*b' }).problem, /符号/)
})

test('what cannot be done yet is said instead of a command', () => {
  assert.match(planRecovery(drives, { ...choice, target: 'C' }).problem, /不能存回原来的盘/)
  assert.match(planRecovery(drives, { ...choice, kinds: [], name: ' ' }).problem, /选一下/)
  assert.match(planRecovery([drives[0]], choice).problem, /只有一个盘/)
  assert.match(planRecovery(drives, { ...choice, source: 'Z' }).problem, /原来在哪个盘/)
  const full = drives.map((d) => (d.letter === 'D' ? { ...d, free: GB / 2 } : d))
  assert.ok(planRecovery(full, choice).notes.some((n) => n.includes('不到 1 GB')))
})

test('the default target prefers a removable drive, then the most free space', () => {
  assert.equal(defaultTarget(drives, 'C'), 'E')
  assert.equal(defaultTarget(drives, 'E'), 'D')
  assert.equal(defaultTarget([drives[0]], 'C'), '')
})
