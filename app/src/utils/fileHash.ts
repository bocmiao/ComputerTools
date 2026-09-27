import { md5, sha1 } from '@noble/hashes/legacy.js'
import { sha256 } from '@noble/hashes/sha2.js'

/** 能算的校验值。MD5、SHA-1 已经不安全（能被故意造出同样的值），只用来核对下载有没有损坏 */
export type HashAlgorithm = 'md5' | 'sha1' | 'sha256'
export type FileDigests = Record<HashAlgorithm, string>

const CREATORS = { md5: () => md5.create(), sha1: () => sha1.create(), sha256: () => sha256.create() }

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('')
}

/**
 * 按小块读一遍文件，同时算出 MD5、SHA-1、SHA-256（不上传，也不把整个文件放进内存）。
 * cancelled() 返回 true 时停下，返回 null。
 */
export async function hashFile(
  file: Blob,
  onProgress: (percent: number) => void,
  cancelled: () => boolean = () => false,
): Promise<FileDigests | null> {
  const hashes = { md5: CREATORS.md5(), sha1: CREATORS.sha1(), sha256: CREATORS.sha256() }
  const chunkSize = 1024 * 1024
  for (let offset = 0; offset < file.size; offset += chunkSize) {
    const chunk = new Uint8Array(await file.slice(offset, offset + chunkSize).arrayBuffer())
    if (cancelled()) return null
    hashes.md5.update(chunk)
    hashes.sha1.update(chunk)
    hashes.sha256.update(chunk)
    onProgress(Math.min(100, Math.round(((offset + chunk.byteLength) / file.size) * 100)))
    if (offset % (8 * chunkSize) === 0) await new Promise<void>((resolve) => setTimeout(resolve, 0))
  }
  if (cancelled()) return null
  onProgress(100)
  return { md5: hex(hashes.md5.digest()), sha1: hex(hashes.sha1.digest()), sha256: hex(hashes.sha256.digest()) }
}

/** 只要 SHA-256 的旧用法 */
export async function sha256File(
  file: Blob,
  onProgress: (percent: number) => void,
  cancelled: () => boolean = () => false,
): Promise<string | null> {
  return (await hashFile(file, onProgress, cancelled))?.sha256 ?? null
}

/** 官方给的校验值按长度认是哪一种：32 位 MD5、40 位 SHA-1、64 位 SHA-256；认不出返回 null */
export function algorithmOf(expected: string): HashAlgorithm | null {
  const v = expected.trim().toLowerCase()
  if (!/^[0-9a-f]+$/.test(v)) return null
  return v.length === 32 ? 'md5' : v.length === 40 ? 'sha1' : v.length === 64 ? 'sha256' : null
}
