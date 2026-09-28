import assert from 'node:assert/strict'
import { test } from 'node:test'
import { exeAdvice, pcLabel } from '../src/utils/exeCheck.ts'

const base = {
  name: 'setup.exe', size: 1000, verdict: 'ok', guess: null, machine: 'x64', pc: 'x64', windows11: true, console: false, dotnet: false,
}

test('a program that fits this PC is ok, with what to try next', () => {
  const a = exeAdvice(base)
  assert.equal(a.ok, true)
  assert.match(a.title, /64 位（x64）的程序/)
  assert.match(a.title, /64 位（x64）的 Windows 11/)
  assert.ok(a.tips.some((t) => t.includes('兼容模式')))
  const console = exeAdvice({ ...base, console: true })
  assert.match(console.tips[0], /命令行程序/)
})

test('the wrong version for the processor says which one to download', () => {
  const arm = exeAdvice({ ...base, verdict: 'wrong-machine', machine: 'arm64' })
  assert.equal(arm.ok, false)
  assert.match(arm.title, /^这是 ARM64 的程序.*这台电脑是 64 位（x64）的 Windows 11，运行不了。$/)
  assert.match(arm.tips[0], /x64/)
  const on32 = exeAdvice({ ...base, verdict: 'wrong-machine', machine: 'x64', pc: 'x86', windows11: false })
  assert.match(on32.title, /32 位的 Windows 10/)
  assert.match(on32.tips[0], /32 位/)
  const armWin10 = exeAdvice({ ...base, verdict: 'wrong-machine', machine: 'x64', pc: 'arm64', windows11: false })
  assert.match(armWin10.tips[0], /Windows 11/)
  assert.equal(pcLabel({ ...base, pc: 'arm64' }), 'ARM 电脑（Windows 11）')
})

test('files that are not programs say what they are', () => {
  assert.match(exeAdvice({ ...base, verdict: 'not-exe', guess: 'html', machine: null }).title, /网页/)
  assert.match(exeAdvice({ ...base, verdict: 'not-exe', guess: 'archive', machine: null }).title, /压缩包/)
  assert.match(exeAdvice({ ...base, verdict: 'not-exe', guess: 'msi', machine: null }).title, /安装包/)
  assert.match(exeAdvice({ ...base, verdict: 'truncated', machine: null }).title, /不完整/)
  assert.match(exeAdvice({ ...base, verdict: 'empty', machine: null }).title, /0 字节/)
  assert.match(exeAdvice({ ...base, verdict: 'old16', machine: null }).title, /16 位/)
  assert.match(exeAdvice({ ...base, verdict: 'dll' }).title, /DLL/)
})
