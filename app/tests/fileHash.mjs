import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { test } from 'node:test'
import { algorithmOf, hashFile, sha256File } from '../src/utils/fileHash.ts'

test('MD5, SHA-1 and SHA-256 are computed in one pass over empty and multi-chunk files', async () => {
  for (const bytes of [new Uint8Array(0), new TextEncoder().encode('abc'), new Uint8Array(2_100_000).fill(37)]) {
    let progress = 0
    const actual = await hashFile(new Blob([bytes]), (value) => { progress = value })
    for (const algorithm of ['md5', 'sha1', 'sha256']) {
      assert.equal(actual[algorithm], createHash(algorithm).update(bytes).digest('hex'), algorithm)
    }
    assert.equal(progress, 100)
    assert.equal(await sha256File(new Blob([bytes]), () => {}), actual.sha256)
  }
})

test('cancellation does not return a partial digest', async () => {
  const result = await hashFile(new Blob([new Uint8Array(2_100_000)]), () => {}, () => true)
  assert.equal(result, null)
})

test('the official value tells which algorithm it is by its length', () => {
  assert.equal(algorithmOf('d41d8cd98f00b204e9800998ecf8427e'), 'md5')
  assert.equal(algorithmOf(' DA39A3EE5E6B4B0D3255BFEF95601890AFD80709 '), 'sha1')
  assert.equal(algorithmOf('e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'), 'sha256')
  assert.equal(algorithmOf('xyz'), null)
  assert.equal(algorithmOf('abcd'), null)
})
