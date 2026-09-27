import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { test } from 'node:test'
import { sha256File } from '../src/utils/fileHash.ts'

test('SHA-256 handles empty and multi-chunk files', async () => {
  for (const bytes of [new Uint8Array(0), new TextEncoder().encode('abc'), new Uint8Array(2_100_000).fill(37)]) {
    let progress = 0
    const actual = await sha256File(new Blob([bytes]), (value) => { progress = value })
    const expected = createHash('sha256').update(bytes).digest('hex')
    assert.equal(actual, expected)
    assert.equal(progress, 100)
  }
})

test('cancellation does not return a partial digest', async () => {
  const result = await sha256File(new Blob([new Uint8Array(2_100_000)]), () => {}, () => true)
  assert.equal(result, null)
})
