// 「此应用无法在你的电脑上运行」：把后端看程序文件头得出的结论（medkit_core::exe_info）说成人话。
import type { ExeCheckView } from '../api/types'

type Machine = NonNullable<ExeCheckView['machine']>

/** 中文和英文、数字之间留一个空格：「这是 ARM64 的程序」 */
function joinWords(a: string, b: string): string {
  return /^[A-Za-z0-9]/.test(b) ? `${a} ${b}` : `${a}${b}`
}

/** 程序是给哪种处理器的 */
export const MACHINE_LABELS: Record<Machine, string> = {
  x86: '32 位（x86）的程序',
  x64: '64 位（x64）的程序',
  arm64: 'ARM64 的程序（给高通骁龙这类 ARM 电脑用的）',
  arm32: '32 位 ARM 的程序',
  ia64: '安腾（Itanium）服务器的程序',
  other: '给别的处理器的程序',
}

/** 这台电脑 */
export function pcLabel(v: ExeCheckView): string {
  const windows = v.windows11 ? 'Windows 11' : 'Windows 10'
  if (v.pc === 'arm64') return `ARM 电脑（${windows}）`
  if (v.pc === 'x86') return `32 位的 ${windows}`
  return `64 位（x64）的 ${windows}`
}

export interface ExeAdvice {
  /** 能运行（从文件本身看） */
  ok: boolean
  title: string
  tips: string[]
}

const REDOWNLOAD = '到软件的官网重新下载一次（别用下载站的「高速下载」），下载完再打开。'

function wrongMachine(v: ExeCheckView): string[] {
  const m = v.machine
  if (m === 'arm64') return ['到官网下载「x64」「64 位」或者写着 Intel、AMD 的那个版本：ARM64 版只能在高通骁龙这类 ARM 电脑上用。']
  if (m === 'x64' && v.pc === 'x86') {
    return [
      '这台电脑装的是 32 位的 Windows，只能运行 32 位的程序：到官网下载「x86」「32 位」的版本。',
      '电脑内存有 4 GB 以上的，可以考虑重装成 64 位的 Windows，以后下载软件就不用挑了。',
    ]
  }
  if (m === 'x64' && v.pc === 'arm64') {
    return ['Windows 10 的 ARM 电脑只能运行 32 位（x86）和 ARM64 的程序：下载这两种版本之一，或者升级到 Windows 11（它能运行 x64 程序）。']
  }
  if (m === 'arm32' && v.pc === 'arm64') {
    return ['新版的 Windows 11（24H2 起）不再支持 32 位 ARM 的程序：找这个软件的 ARM64、x64 或者 x86 版本。']
  }
  if (m === 'ia64') return ['安腾是老式服务器用的处理器，普通电脑都用不了：到官网下载 x64 或者 x86 的版本。']
  return ['到官网下载写着「x64」「64 位」或者「x86」「32 位」的版本。']
}

/** 结论说成人话：标题一句，下面几条该怎么办。 */
export function exeAdvice(v: ExeCheckView): ExeAdvice {
  switch (v.verdict) {
    case 'empty':
      return { ok: false, title: '这个文件是空的（0 字节），里面什么都没有。', tips: ['多半是下载没成功，或者被安全软件清空了。', REDOWNLOAD] }
    case 'not-exe':
      switch (v.guess) {
        case 'msi':
          return { ok: false, title: '这是安装包（MSI），不是程序本身。', tips: ['双击它会用 Windows 自带的安装程序来装；装好以后，从开始菜单或者桌面的快捷方式打开软件。'] }
        case 'archive':
          return { ok: false, title: '这其实是一个压缩包，不是程序（扩展名被改成了 .exe，或者下载时存错了）。', tips: ['用解压软件打开它，看看里面有没有真正的程序。', REDOWNLOAD] }
        case 'html':
          return { ok: false, title: '这其实是一个网页，不是程序。', tips: ['下载时存下来的是下载页面本身（网盘、下载站的跳转页常这样）。到软件官网找真正的下载按钮，重新下载。'] }
        case 'pdf':
          return { ok: false, title: '这是一个 PDF 文档，不是程序。', tips: ['用 PDF 阅读器或者浏览器打开它。'] }
        default:
          return { ok: false, title: '这不是 Windows 能运行的程序。', tips: ['可能是文件坏了、下载没成功，也可能根本不是给 Windows 的（比如苹果电脑、安卓手机的安装包）。', REDOWNLOAD] }
      }
    case 'truncated':
      return {
        ok: false,
        title: '文件不完整：没下载完，或者下载、拷贝的时候坏了。',
        tips: [REDOWNLOAD, '官网给了 SHA-256 这类校验值的，可以用工具箱里的「文件校验」核对下载得对不对。'],
      }
    case 'dll':
      return {
        ok: false,
        title: '这是一个 DLL（程序用的组件），不是能双击运行的程序。',
        tips: ['要打开的应该是软件文件夹里的另一个 .exe，或者开始菜单、桌面上的快捷方式。', '软件提示缺少某个 dll 的，看「软件打不开，提示缺少 dll」。'],
      }
    case 'old16':
      return {
        ok: false,
        title: '这是 16 位的老程序（DOS 或者 Windows 3.x 时代的）。',
        tips: ['64 位和 ARM 版的 Windows 都不能运行 16 位程序（微软说明：它们不带运行 16 位程序的 NTVDM）。', '要用的话，可以用 DOSBox 这类模拟器运行它，或者找这个软件的新版本。'],
      }
    case 'wrong-machine':
      return {
        ok: false,
        title: `${joinWords('这是', v.machine ? MACHINE_LABELS[v.machine] : '给别的处理器的程序')}，${joinWords('这台电脑是', pcLabel(v))}，运行不了。`,
        tips: wrongMachine(v),
      }
    case 'not-desktop':
      return {
        ok: false,
        title: '这不是在 Windows 桌面上直接运行的程序（像是驱动、启动程序这类系统文件）。',
        tips: ['要装驱动的，用驱动自带的安装程序（常叫 setup.exe），或者在设备管理器里更新驱动。'],
      }
    default: {
      const tips: string[] = []
      if (v.console) tips.push('这是命令行程序：双击时黑框一闪就关掉，不是坏了，要在命令提示符里运行它。')
      if (v.machine === 'arm32') tips.push('这是 32 位 ARM 的程序，新版的 Windows 11（24H2 起）不再支持它；打不开的话找这个软件的 ARM64 或者 x64 版本。')
      tips.push(
        '还是提示「此应用无法在你的电脑上运行」的话：右键它 →「属性」→「兼容性」，勾上「以兼容模式运行这个程序」试试，或者用下面的「程序兼容性疑难解答」。',
        '用杀毒软件扫一下这个文件，看是不是被拦住了或者被感染了；' + REDOWNLOAD,
      )
      return {
        ok: true,
        title: `从文件本身看，它能在这台电脑上运行：${joinWords('是', v.machine ? MACHINE_LABELS[v.machine] : 'Windows 程序')}${v.dotnet ? '（.NET 程序）' : ''}，${joinWords('这台电脑是', pcLabel(v))}。`,
        tips,
      }
    }
  }
}
