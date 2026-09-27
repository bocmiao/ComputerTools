import assert from 'node:assert/strict'
import { test } from 'node:test'
import { formatBytes } from '../src/utils/imageBatch.ts'
import { duplicateSummary, spaceNotes, spaceSummary, whereLine } from '../src/utils/space.ts'

const report = (over) => ({
  folder: 'D:\\资料', files: 1234, totalBytes: 5 * 1024 ** 3, skipped: 0, onlineOnly: 0, truncated: false,
  largest: [], duplicates: [], duplicateGroups: 0, wastedBytes: 0, comparedAll: true, ...over,
})

test('the summary names the folder, the count and the size', () => {
  assert.equal(spaceSummary(report({}), formatBytes), 'D:\\资料：1,234 个文件，一共 5.00 GB。')
  assert.match(duplicateSummary(report({}), formatBytes), /没有找到/)
  const group = { size: 10, count: 2, files: [] }
  assert.equal(
    duplicateSummary(report({ duplicates: [group], duplicateGroups: 3, wastedBytes: 2 * 1024 ** 3 }), formatBytes),
    '有 3 组内容完全一样的文件（下面列出最大的 1 组）：每组只留一个的话，能腾出 2.00 GB。',
  )
})

test('notes say what was not counted or compared', () => {
  assert.deepEqual(spaceNotes(report({})), [])
  const notes = spaceNotes(report({ truncated: true, skipped: 2, onlineOnly: 5, comparedAll: false }))
  assert.equal(notes.length, 4)
  assert.match(notes[0], /只数了前 1,234 个/)
  assert.match(notes[2], /仅在线/)
})

test('where a file is, relative to the chosen folder', () => {
  const f = { id: 0, name: 'a', folder: '', size: 1, modified: null, protected: false }
  assert.equal(whereLine(f), '就在选的文件夹里')
  assert.match(whereLine({ ...f, folder: '视频', modified: Date.UTC(2026, 4, 11, 4) }), /^视频 · 2026\/5\/11 改过$/)
})
