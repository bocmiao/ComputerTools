// 交给 Windows 文字识别之前的图片处理（工具箱的「图片转文字」、按症状修的「看报错截图」共用）：
// 转正（照片里记的方向）、画到白底的画布上存成 PNG（透明的截图也认得出；Windows 只读它认得的格式），太大的按比例缩小。

/** 画布每边最多多少像素（WebView2 画不了更大的） */
export const MAX_EDGE = 32767
/** 一张图片最多多少像素（和图片批量处理一样：再大解码容易失败） */
export const MAX_OCR_PIXELS = 100_000_000

/** 这张图片打不开的原因；能认的返回空字符串。HEIC 另外说办法（heic 为 true）。 */
export function imageProblem(name: string, type: string): { text: string; heic: boolean } {
  const lower = name.toLowerCase()
  if (/\.(heic|heif)$/.test(lower) || type === 'image/heic' || type === 'image/heif') {
    return { text: `「${name}」是 HEIC（苹果手机的照片格式），这里打不开。`, heic: true }
  }
  if (/\.tiff?$/.test(lower) || type === 'image/tiff') {
    return { text: `「${name}」是 TIFF 格式，这里打不开：先用「画图」打开，另存为 PNG 再来。`, heic: false }
  }
  if (lower.endsWith('.svg') || type === 'image/svg+xml') return { text: `「${name}」是 SVG 矢量图，这里打不开。`, heic: false }
  if (!type.startsWith('image/') && !/\.(jpe?g|png|webp|gif|bmp|ico|avif)$/.test(lower)) {
    return { text: `「${name}」不是图片文件。`, heic: false }
  }
  return { text: '', heic: false }
}

/** 缩小到多少倍才画得下（1 就是不用缩） */
export function ocrScale(width: number, height: number): number {
  if (width <= 0 || height <= 0) return 1
  return Math.min(1, MAX_EDGE / width, MAX_EDGE / height, Math.sqrt(MAX_OCR_PIXELS / (width * height)))
}

export function openImage(file: Blob): Promise<ImageBitmap> {
  return createImageBitmap(file, { imageOrientation: 'from-image' }).catch(() => createImageBitmap(file))
}

/** 转正、画到白底上，存成 PNG；shrunk：缩小过 */
export async function toOcrPng(file: Blob): Promise<{ bytes: Uint8Array; shrunk: boolean }> {
  const bitmap = await openImage(file)
  try {
    const scale = ocrScale(bitmap.width, bitmap.height)
    const canvas = document.createElement('canvas')
    canvas.width = Math.max(1, Math.floor(bitmap.width * scale))
    canvas.height = Math.max(1, Math.floor(bitmap.height * scale))
    const ctx = canvas.getContext('2d')
    if (!ctx) throw new Error('没能打开这张图片。')
    ctx.fillStyle = '#fff'
    ctx.fillRect(0, 0, canvas.width, canvas.height)
    ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height)
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'))
    if (!blob) throw new Error('没能把图片转成 PNG，可能是图片太大了。')
    return { bytes: new Uint8Array(await blob.arrayBuffer()), shrunk: scale < 1 }
  } finally {
    bitmap.close()
  }
}

/** 粘贴进来的截图名字都一样：换成「截图.png」这样 */
export function pastedFile(file: File): File {
  const ext = file.type === 'image/jpeg' ? 'jpg' : (file.type.split('/')[1] ?? 'png')
  return new File([file], `截图.${ext}`, { type: file.type, lastModified: Date.now() })
}
