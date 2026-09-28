/** 把后端或示例数据抛出的错误变成一句能显示的话 */
export function errorText(e: unknown): string {
  if (typeof e === 'string') return e
  if (e instanceof Error) return e.message
  if (e && typeof e === 'object' && 'message' in e && typeof e.message === 'string') return e.message
  try {
    return JSON.stringify(e) ?? String(e)
  } catch {
    return String(e)
  }
}

function pad(n: number): string {
  return String(n).padStart(2, '0')
}

function parseTime(value: string): Date | null {
  const d = new Date(value)
  return Number.isNaN(d.getTime()) ? null : d
}

function sameDay(a: Date, b: Date): boolean {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate()
}

/** 「今天」「昨天」「9月20日」「2025年9月20日」 */
export function formatDay(value: string): string {
  const d = parseTime(value)
  if (!d) return value
  const now = new Date()
  if (sameDay(d, now)) return '今天'
  const yesterday = new Date(now)
  yesterday.setDate(now.getDate() - 1)
  if (sameDay(d, yesterday)) return '昨天'
  const md = `${d.getMonth() + 1}月${d.getDate()}日`
  return d.getFullYear() === now.getFullYear() ? md : `${d.getFullYear()}年${md}`
}

/** 「14:32」 */
export function formatClock(value: string): string {
  const d = parseTime(value)
  if (!d) return value
  return `${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** 「今天 14:32」「9月20日 14:32」 */
export function formatDateTime(value: string): string {
  const d = parseTime(value)
  if (!d) return value
  return `${formatDay(value)} ${formatClock(value)}`
}

export type SimpleValue = string | number | boolean

export function isSimpleValue(v: unknown): v is SimpleValue {
  return typeof v === 'string' || typeof v === 'boolean' || (typeof v === 'number' && Number.isFinite(v))
}

/** facts 里只显示简单值（字符串、数字、布尔）；对象和数组不显示 */
export function simpleFacts(facts: Record<string, unknown>): { key: string; value: string }[] {
  return Object.entries(facts)
    .filter((entry): entry is [string, SimpleValue] => isSimpleValue(entry[1]))
    .map(([key, v]) => ({ key, value: formatSimple(v) }))
}

function formatSimple(v: SimpleValue): string {
  if (typeof v === 'boolean') return v ? '是' : '否'
  if (typeof v === 'number') return String(Math.round(v * 10) / 10)
  return v
}

/** 搜索用：全角转半角、转小写、去掉空格和常见标点 */
export function normalizeForSearch(s: string): string {
  return s
    .normalize('NFKC')
    .toLowerCase()
    .replace(/[\s\-_·•,，.。、!！?？:：;；'"“”‘’「」『』()（）【】[\]]/g, '')
}

const ID_RE = /^[a-z0-9]+([.-][a-z0-9]+)*$/

export type LinkKind = 'symptom' | 'feature' | 'tool' | 'test'
export type ParsedLink = { kind: LinkKind; id: string }

/**
 * 解析检测结果、小工具结果、症状指引里的 links：只认 symptom:<id>、feature:<id>、tool:<id> 和
 * test:<设备测试>（工具箱里的喇叭、麦克风、键盘这些测试），其余一律忽略
 */
export function parseLink(link: string): ParsedLink | null {
  const i = link.indexOf(':')
  if (i < 0) return null
  const kind = link.slice(0, i)
  const id = link.slice(i + 1)
  if ((kind === 'symptom' || kind === 'feature' || kind === 'tool' || kind === 'test') && ID_RE.test(id)) {
    return { kind, id }
  }
  return null
}
