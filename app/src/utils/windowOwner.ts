// 「弹窗是哪个软件的」的文字：鼠标指着的是什么、接下来怎么办。结果来自后端（medkit-core/src/window_owner.rs）。

import type { WindowOwnerReport, WindowPosition } from '../api/types'

export const POSITION_LABELS: Record<WindowPosition, string> = {
  full: '铺满整个屏幕',
  'top-left': '屏幕左上角',
  top: '屏幕上边',
  'top-right': '屏幕右上角',
  left: '屏幕左边',
  center: '屏幕中间',
  right: '屏幕右边',
  'bottom-left': '屏幕左下角',
  bottom: '屏幕下边',
  'bottom-right': '屏幕右下角',
}

/** 程序叫什么：程序文件里写的说明，没有就用产品名、文件名 */
export function ownerName(r: WindowOwnerReport): string {
  return r.description ?? r.product ?? r.exe ?? '没有名字的程序'
}

/** 「屏幕右下角一个 360 × 260 的窗口」 */
function windowPhrase(r: WindowOwnerReport, noun: string): string {
  if (r.position === 'full') return `一个铺满整个屏幕的${/^[A-Za-z]/.test(noun) ? ' ' : ''}${noun}`
  const where = r.position ? POSITION_LABELS[r.position] : ''
  const size = r.width > 0 && r.height > 0 ? ` ${r.width} × ${r.height} 的` : ''
  // 中文和英文之间空一格：「的 Windows 通知」
  const gap = /^[A-Za-z]/.test(noun) ? ' ' : ''
  return `${where}一个${size}${gap}${noun}`
}

/** 第一句话：鼠标指着的是什么 */
export function ownerSummary(r: WindowOwnerReport): string {
  switch (r.kind) {
    case 'nothing':
      return '鼠标下面没有窗口。'
    case 'medkit':
      return '鼠标还在小药箱的窗口上。点「开始找」以后，在倒数完之前把鼠标移到弹窗上停住（不用点它）。'
    case 'shell':
      return '鼠标指着的是任务栏或者桌面，不是弹窗。'
    case 'unreadable':
      return `鼠标指着${windowPhrase(r, '窗口')}，但读不出是哪个程序的：可能刚好关掉了，也可能是受保护的系统程序。再找一次试试。`
    case 'notification':
      return `鼠标指着${windowPhrase(r, 'Windows 通知')}：这是 Windows 替别的软件显示的通知。`
    case 'system':
      return `鼠标指着${windowPhrase(r, '窗口')}，是 Windows 自带的程序「${ownerName(r)}」。`
    case 'program':
      return `鼠标指着${windowPhrase(r, '窗口')}，是这个程序弹的：「${ownerName(r)}」。`
  }
}

/** 接下来怎么办 */
export function ownerAdvice(r: WindowOwnerReport): string[] {
  switch (r.kind) {
    case 'program': {
      const who = r.installed ? `「${r.installed}」` : '这个软件'
      return [
        r.installed
          ? `它属于已安装的软件${who}${r.publisher ? `（${r.publisher}）` : ''}，在「已安装的应用」里能找到。`
          : '在已安装的应用里没对上是哪个软件：可能是不用安装的绿色软件，或者是别的软件顺带放进来的。点「打开所在的文件夹」看看它在哪。',
        `不认识、用不上${who}的：在「已安装的应用」里把它卸载（下次装软件时留意安装界面上顺带勾选的其他软件，取消勾选再装）。`,
        `还要用${who}的：打开它的设置，把「资讯」「热点」「弹窗推荐」「消息提醒」这类选项关掉。`,
        '它每次开机都自己启动的：在「启动应用」里把它关掉，开机不再自己运行，也就不会弹了。',
      ]
    }
    case 'notification':
      return [
        '看看通知最上面写的是哪个软件。',
        '写的是浏览器（Microsoft Edge、Chrome 这些）：多半是以前在某个网站上点过「允许」通知。在浏览器的设置里搜「通知」，把那个网站删掉或者改成「阻止」。',
        '写的是别的软件：在「通知设置」里把这个软件的通知关掉。',
      ]
    case 'system':
      return [
        '这是 Windows 自己的窗口，不是广告软件弹的。',
        'Windows 自带的推荐和提示，可以在「常用设置」里「推荐和广告」那一组一项一项关掉。',
      ]
    case 'shell':
    case 'nothing':
    case 'unreadable':
      return ['等弹窗出来以后再点「开始找」，倒数完之前把鼠标移到弹窗上停住（不用点它）。']
    case 'medkit':
      return []
  }
}

/** 这种结果要不要给「打开所在的文件夹」的按钮 */
export function canReveal(r: WindowOwnerReport): boolean {
  return (r.kind === 'program' || r.kind === 'system') && r.exe !== null
}

/** 结果下面放哪几个小工具的按钮（tool: 链接） */
export function ownerLinks(r: WindowOwnerReport): string[] {
  if (r.kind === 'program') return ['tool:settings.apps', 'tool:settings.startup-apps']
  if (r.kind === 'notification') return ['tool:settings.notifications']
  return []
}
