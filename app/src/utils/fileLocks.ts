// 「文件删不掉：是谁占着」的说法：查到的每个程序是什么、怎么让它放手，没查到时还可能是什么原因。
import type { FileLockReport, FileLockUser, FileUserKind } from '../api/types'

export const KIND_LABELS: Record<FileUserKind, string> = {
  window: '程序',
  console: '命令行',
  explorer: '资源管理器',
  other: '后台程序',
  service: '系统服务',
  critical: '系统关键进程',
}

/** Windows 安全中心（Defender）的扫描服务：多半是在检查刚下载的文件，等它检查完就好 */
function isDefender(u: FileLockUser): boolean {
  return u.service?.toLowerCase() === 'windefend' || u.program?.toLowerCase() === 'msmpeng.exe'
}

/** 这个程序在用文件时，怎么让它放手 */
export function lockAdvice(u: FileLockUser): string {
  if (u.isSelf) return '就是小药箱自己在用。关掉小药箱再删除。'
  if (u.otherSession) return '这是另一个登录的用户开着的程序。请那个用户关掉它，或者注销那个用户，再删除。'
  switch (u.kind) {
    case 'window':
      return `先在「${u.name}」里保存、关掉这个文件，或者直接退出这个程序，再删除。`
    case 'console':
      return '一个命令行窗口正在用它：等它运行完，或者关掉那个窗口，再删除。'
    case 'explorer':
      return '资源管理器正在用它，多半是在显示预览或者生成缩略图。关掉右边的预览窗格、换到别的文件夹再删；还不行就点下面的「重启资源管理器」。'
    case 'service':
      return isDefender(u)
        ? 'Windows 安全中心正在检查它（刚下载、刚解压的文件常这样），等一两分钟再删除。'
        : `系统服务「${u.name}」正在用它。重启电脑以后再删除。`
    case 'critical':
      return 'Windows 自己的关键部分在用它，不能关掉。重启电脑以后再删除。'
    default:
      return `后台程序「${u.name}」正在用它（没有窗口）。可以按 Ctrl + Shift + Esc 打开任务管理器，在「进程」里找到它、点「结束任务」（它没保存的内容会丢）；或者重启电脑以后再删除。`
  }
}

/** 查的是什么：「报告.docx」、这 3 个文件、文件夹「旧项目」里的文件 */
export function targetPhrase(r: FileLockReport): string {
  if (r.mode === 'folder') return `文件夹「${r.targets[0] ?? ''}」里的文件`
  return r.targets.length === 1 ? `「${r.targets[0]}」` : `这 ${r.targets.length} 个文件`
}

/** 结果的第一句话 */
export function lockSummary(r: FileLockReport): string {
  if (r.checked === 0) {
    return r.mode === 'folder' ? `文件夹「${r.targets[0] ?? ''}」里没有能查的文件。` : '选的文件都没能查。'
  }
  if (r.users.length === 0) return `没有找到在用${targetPhrase(r)}的程序。`
  return `有 ${r.users.length} 个程序在用${targetPhrase(r)}：`
}

/** 查的时候漏掉了哪些（文件不见了、没能查、太多只查了一部分、子文件夹打不开） */
export function lockNotes(r: FileLockReport): string[] {
  const notes: string[] = []
  if (r.missing.length) notes.push(`这些文件已经不在了（可能已经删掉或者改了名）：${r.missing.join('、')}`)
  if (r.failed.length) notes.push(`这些文件没能查：${r.failed.join('、')}`)
  if (r.truncated) notes.push(r.mode === 'folder' ? '文件夹里的文件太多，只查了前 5000 个。' : '选的文件太多，只查了前 100 个。')
  if (r.unreadable > 0) notes.push(`有 ${r.unreadable} 个子文件夹没有权限打开，里面的文件没查。`)
  return notes
}

/** 没查到在用的程序时，还可能是什么原因 */
export const NOTHING_FOUND_TIPS = [
  '刚才占着它的程序可能已经关了：现在再删一次试试。',
  '整个文件夹删不掉：可能有命令行、终端窗口停在这个文件夹里（这种情况查不出来），关掉它们再试。',
  '提示「需要权限」：文件属于别的用户或者系统。删除时点「继续」；Windows 自己的文件不要删。',
]

/** 这个程序在用的是哪些文件 */
export function filesLine(r: FileLockReport, u: FileLockUser): string {
  if (!u.files.length) return r.mode === 'folder' ? '在用这个文件夹里的文件（文件太多，说不出是哪几个）' : ''
  const more = u.moreFiles > 0 ? ` 等 ${u.files.length + u.moreFiles} 个` : ''
  return `在用：${u.files.join('、')}${more}`
}
