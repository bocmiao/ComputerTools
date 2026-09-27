import { sha256 } from '@noble/hashes/sha2.js'

/** Hash a selected local file in bounded chunks, without uploading or holding it all in memory. */
export async function sha256File(
  file: Blob,
  onProgress: (percent: number) => void,
  cancelled: () => boolean = () => false,
): Promise<string | null> {
  const hash = sha256.create()
  const chunkSize = 1024 * 1024
  for (let offset = 0; offset < file.size; offset += chunkSize) {
    const chunk = await file.slice(offset, offset + chunkSize).arrayBuffer()
    if (cancelled()) return null
    hash.update(new Uint8Array(chunk))
    onProgress(Math.min(100, Math.round(((offset + chunk.byteLength) / file.size) * 100)))
    if (offset % (8 * chunkSize) === 0) await new Promise<void>((resolve) => setTimeout(resolve, 0))
  }
  if (cancelled()) return null
  onProgress(100)
  return Array.from(hash.digest(), (b) => b.toString(16).padStart(2, '0')).join('')
}
