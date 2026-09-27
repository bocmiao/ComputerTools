// 「找大文件和重复文件」的说法。大小的写法由调用的地方给（formatBytes），这里不引别的模块。
import type { SpaceFile, SpaceReport } from '../api/types'

type Size = (bytes: number) => string

/** 第一句：数了什么 */
export function spaceSummary(r: SpaceReport, size: Size): string {
  return `${r.folder}：${r.files.toLocaleString('zh-CN')} 个文件，一共 ${size(r.totalBytes)}。`
}

/** 重复文件的总结 */
export function duplicateSummary(r: SpaceReport, size: Size): string {
  if (r.duplicateGroups === 0) return '没有找到内容完全一样的文件（只比较 1 MB 以上的，Windows 和程序自己的文件不比较）。'
  const shown = r.duplicates.length < r.duplicateGroups ? `（下面列出最大的 ${r.duplicates.length} 组）` : ''
  return `有 ${r.duplicateGroups} 组内容完全一样的文件${shown}：每组只留一个的话，能腾出 ${size(r.wastedBytes)}。`
}

/** 没数到、没比完的部分 */
export function spaceNotes(r: SpaceReport): string[] {
  const notes: string[] = []
  if (r.truncated) notes.push(`文件太多，只数了前 ${r.files.toLocaleString('zh-CN')} 个（最多数 60 秒、50 万个）：结果只包括数到的部分。`)
  if (r.skipped > 0) notes.push(`有 ${r.skipped} 个文件夹没有权限打开，里面的文件没数。`)
  if (r.onlineOnly > 0) notes.push(`有 ${r.onlineOnly} 个「仅在线」的网盘文件没算：它们不占这台电脑的空间。`)
  if (!r.comparedAll) notes.push('可能重复的文件太多，比较了 60 秒就停了：下面只列出比完的那些。')
  return notes
}

/** 文件在哪儿：相对选的文件夹 */
export function whereLine(f: SpaceFile): string {
  const date = f.modified ? ` · ${new Date(f.modified).toLocaleDateString('zh-CN')} 改过` : ''
  return `${f.folder || '就在选的文件夹里'}${date}`
}
