import assert from 'node:assert/strict'
import { test } from 'node:test'
import { errorCodes, matchSymptoms, normalizeForSearch } from '../src/utils/symptomMatch.ts'

const symptoms = [
  { id: 'app-missing-dll', title: '软件打不开，提示缺少 dll', keywords: ['dll', '找不到dll', '无法继续执行代码', 'msvcp140.dll', '0xc000007b', '应用程序无法正常启动'] },
  { id: 'update-failed', title: '系统更新失败、卡住', keywords: ['更新失败', '0x80070643', '0x800f0922', '更新'] },
  { id: 'network', title: '上不了网', keywords: ['上不了网', '打不开', 'err_connection_reset'] },
]

test('the search form ignores width, case, spaces and punctuation', () => {
  assert.equal(normalizeForSearch('ＭＳＶＣＰ140.DLL 丢失！'), 'msvcp140dll丢失')
})

test('an error dialog is matched to the symptom with the most specific keywords', () => {
  const text = 'QQ.exe - 系统错误\n由于找不到 MSVCP140.dll，无法继续执行代码。重新安装程序可能会解决此问题。'
  const [first, ...rest] = matchSymptoms(text, symptoms)
  assert.equal(first.id, 'app-missing-dll')
  assert.deepEqual(first.hits, ['msvcp140.dll', '无法继续执行代码'], '长的在前；「dll」被「msvcp140.dll」包含，不重复算')
  assert.equal(rest.length, 0, '别的症状一个像样的关键词都没对上')
})

test('codes read with a letter O are still matched', () => {
  const text = '安装更新时出现问题。错误代码 Ox80070643'
  assert.equal(matchSymptoms(text, symptoms)[0].id, 'update-failed')
  assert.deepEqual(errorCodes(text), ['0x80070643'])
  assert.deepEqual(errorCodes('0xc000007b 和 0XC000007B'), ['0xC000007B'], '去重，写成大写')
  assert.deepEqual(errorCodes('0x123 太短'), [])
})

test('short, generic words alone do not count', () => {
  assert.deepEqual(matchSymptoms('网页打不开', symptoms), [], '只对上「打不开」三个字')
  assert.deepEqual(matchSymptoms('', symptoms), [])
  assert.equal(matchSymptoms('ERR_CONNECTION_RESET', symptoms)[0].id, 'network')
})
