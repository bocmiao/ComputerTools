// 二维码显示的规则，和「文字、网址变二维码」的提示。

/** 最多放多少字节：再多码就太密，手机从屏幕上扫不出来（900 字节是 M 级纠错的第 24 版，113×113 格） */
export const QR_MAX_BYTES = 900

/** 四周留的白边格数：扫码软件要这圈白边才认得出来 */
export const QR_QUIET = 4

/** 要编进二维码的内容：去掉前后的空白（粘贴时常带上换行） */
export function qrText(input: string): string {
  return input.trim()
}

/** UTF-8 字节数：二维码按字节放，一个汉字 3 个字节 */
export function utf8Bytes(text: string): number {
  return new TextEncoder().encode(text).length
}

/** 画多大（像素）：每格大约 3 像素，码越密画得越大，在 220 到 360 之间 */
export function qrPixels(size: number): number {
  return Math.min(360, Math.max(220, (size + QR_QUIET * 2) * 3))
}

export interface QrHint {
  tone: 'danger' | 'muted'
  text: string
}

/** 输入框下面的提示：太长了，或者网址前面没写 https:// */
export function qrHint(text: string): QrHint | null {
  const bytes = utf8Bytes(text)
  if (bytes > QR_MAX_BYTES) {
    return {
      tone: 'danger',
      text: `内容太长了（${bytes} 字节，最多 ${QR_MAX_BYTES}）：码太密，手机扫不出来。长的文字分几次发，或者用微信的「文件传输助手」。`,
    }
  }
  if (/^www\./i.test(text) && !/\s/.test(text)) {
    return { tone: 'muted', text: '网址前面加上 https://，手机扫了才会直接打开网页。' }
  }
  return null
}
