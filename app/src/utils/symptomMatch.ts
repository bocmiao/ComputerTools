// 看报错截图：把 Windows 认出来的报错文字和症状的名字、关键词对一对，找出最可能是哪个症状。
// 关键词都是症状数据里写好的（DLL 名字、错误代码、「无法继续执行代码」这类原话），和搜索框用同一套。

/** 搜索用的写法：全角转半角、大写转小写，去掉空格和标点（「电脑 没网」「电脑没网。」都一样） */
export function normalizeForSearch(s: string): string {
  return s
    .normalize('NFKC')
    .toLowerCase()
    .replace(/[\s\-_·•,，.。、!！?？:：;；'"“”‘’「」『』()（）【】[\]]/g, '')
}

export interface MatchableSymptom {
  id: string
  title: string
  keywords: string[]
}

export interface SymptomMatch {
  id: string
  title: string
  /** 对上的关键词（原样），长的在前 */
  hits: string[]
  score: number
}

/** 对上的字加起来至少这么长才算（只对上「dll」「打不开」这种两三个字的不算） */
const MIN_SCORE = 4

/** 文字识别常把 0x 的 0 认成字母 O：错误代码前面的 O 换回 0 */
function fixCodes(text: string): string {
  return text.replace(/\b[oO][xX](?=[0-9a-fA-F]{8}\b)/g, '0x')
}

/**
 * 报错文字里出现了哪些症状的关键词：一个关键词被这个症状另一个对上的关键词包含时只算长的那个（「msvcp140.dll」和「dll」
 * 只算前一个）；按对上的字数排，最多 limit 个。
 */
export function matchSymptoms(text: string, symptoms: MatchableSymptom[], limit = 3): SymptomMatch[] {
  const haystack = normalizeForSearch(fixCodes(text))
  if (!haystack) return []
  const matches: SymptomMatch[] = []
  for (const s of symptoms) {
    const found = new Map<string, string>()
    for (const phrase of [s.title, ...s.keywords]) {
      const key = normalizeForSearch(phrase)
      if (key.length >= 2 && haystack.includes(key) && !found.has(key)) found.set(key, phrase)
    }
    const keys = [...found.keys()]
    const kept = keys.filter((k) => !keys.some((other) => other !== k && other.includes(k)))
    const score = kept.reduce((sum, k) => sum + k.length, 0)
    if (score < MIN_SCORE) continue
    kept.sort((a, b) => b.length - a.length)
    matches.push({ id: s.id, title: s.title, hits: kept.map((k) => found.get(k) ?? k), score })
  }
  matches.sort((a, b) => b.score - a.score || a.title.localeCompare(b.title, 'zh-CN'))
  return matches.slice(0, limit)
}

/** 报错文字里的错误代码（0x 开头的八位十六进制），写成 0x800F0922 这样，去重 */
export function errorCodes(text: string): string[] {
  const codes = new Set<string>()
  for (const m of fixCodes(text).matchAll(/\b0[xX]([0-9a-fA-F]{8})\b/g)) {
    codes.add(`0x${m[1]!.toUpperCase()}`)
  }
  return [...codes]
}
