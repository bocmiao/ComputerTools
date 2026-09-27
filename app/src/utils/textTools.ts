// 文本整理：全角半角互换、去空行、去重、排序、去首尾空格、统计字数。都在这台电脑上算，不上传。

export type TextAction = 'to-half' | 'to-full' | 'drop-empty' | 'dedupe' | 'sort' | 'trim'

/** 全角的英文字母、数字、标点（！到～）和全角空格换成半角 */
export function toHalfWidth(text: string): string {
  return text.replace(/[！-～]/g, (c) => String.fromCharCode(c.charCodeAt(0) - 0xfee0)).replace(/　/g, ' ')
}

/** 半角的英文字母、数字、标点（! 到 ~）和空格换成全角 */
export function toFullWidth(text: string): string {
  return text.replace(/[!-~]/g, (c) => String.fromCharCode(c.charCodeAt(0) + 0xfee0)).replace(/ /g, '　')
}

function lines(text: string): string[] {
  return text.replace(/\r\n?/g, '\n').split('\n')
}

export function applyTextAction(action: TextAction, text: string): string {
  switch (action) {
    case 'to-half':
      return toHalfWidth(text)
    case 'to-full':
      return toFullWidth(text)
    case 'drop-empty':
      return lines(text).filter((l) => l.trim() !== '').join('\n')
    case 'dedupe': {
      const seen = new Set<string>()
      return lines(text).filter((l) => (seen.has(l) ? false : (seen.add(l), true))).join('\n')
    }
    case 'sort':
      // 中文按拼音排，数字按大小排（「第2章」在「第10章」前面）
      return [...lines(text)].sort((a, b) => a.localeCompare(b, 'zh-CN', { numeric: true })).join('\n')
    case 'trim':
      return lines(text).map((l) => l.trim()).join('\n')
  }
}

export interface TextStats {
  /** 汉字（含中文标点以外的 CJK 字） */
  chinese: number
  /** 英文单词（连续的字母、数字、撇号） */
  words: number
  /** 不算空格、换行的字符数 */
  chars: number
  /** 算上空格的字符数（不算换行） */
  charsWithSpaces: number
  lines: number
}

export function textStats(text: string): TextStats {
  const all = Array.from(text)
  return {
    chinese: (text.match(/[㐀-䶿一-鿿豈-﫿]/g) ?? []).length,
    words: (text.match(/[A-Za-z0-9]+(?:'[A-Za-z]+)?/g) ?? []).length,
    chars: all.filter((c) => !/\s/.test(c)).length,
    charsWithSpaces: all.filter((c) => c !== '\n' && c !== '\r').length,
    lines: text === '' ? 0 : lines(text).length,
  }
}
