// 「找回误删的文件」：照用户选的，拼出微软 Windows File Recovery（winfr）的命令，再说清楚怎么用。小药箱不替用户运行它。
// 参数照微软《Windows File Recovery》：winfr 源盘: 目标盘: /regular 或 /extensive，再加 /n 筛选（文件名、类型、通配符）；
// /regular 只能用在没坏的 NTFS 盘上，FAT、exFAT（U 盘、存储卡）要用 /extensive；源盘和目标盘不能是同一个；找到的文件放在
// 目标盘新建的 Recovery_日期_时间 文件夹里。
// 按类型找的分组参考了 kil0bit-kb/winfr-pro（MIT 许可，Copyright (c) 2026 KB - kilObit）的做法，另外加了 WPS 的格式、
// 手机照片的 HEIC 和录音的 AMR。不用 /o:b（winfr-pro 记下的：exFAT 上会让 winfr 崩溃），也不用 /a：让用户看到参数、自己按 y。

/** 要找的文件类型 */
export type FileKind = 'photos' | 'documents' | 'videos' | 'music' | 'archives'

export const FILE_KINDS: FileKind[] = ['photos', 'documents', 'videos', 'music', 'archives']

export const KIND_LABELS: Record<FileKind, string> = {
  photos: '照片、图片',
  documents: '文档（Word、Excel、PPT、WPS、PDF）',
  videos: '视频',
  music: '音乐、录音',
  archives: '压缩包',
}

export const KIND_EXTENSIONS: Record<FileKind, string[]> = {
  photos: ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp', 'heic', 'tif', 'tiff', 'dng', 'cr2', 'nef', 'arw'],
  documents: ['doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'wps', 'et', 'dps', 'pdf', 'txt', 'rtf', 'csv'],
  videos: ['mp4', 'mov', 'avi', 'mkv', 'wmv', 'flv', 'm4v', 'webm', '3gp', 'mpg', 'mpeg'],
  music: ['mp3', 'wav', 'flac', 'aac', 'm4a', 'wma', 'ogg', 'amr'],
  archives: ['zip', 'rar', '7z', 'tar', 'gz', 'iso'],
}

/** 一个盘（「硬盘测速」列盘的结果里要用的几项） */
export interface RecoveryDrive {
  letter: string
  label: string
  fileSystem: string
  removable: boolean
  system: boolean
  free: number
}

export interface RecoveryChoice {
  /** 文件原来在哪个盘 */
  source: string
  /** 找回来存到哪个盘 */
  target: string
  kinds: FileKind[]
  /** 文件名里有的字，可以不填 */
  name: string
  /** 仔细找（/extensive）；源盘不是 NTFS 时不管选没选都是仔细找 */
  thorough: boolean
}

export interface RecoveryPlan {
  /** 拼好的命令；有问题时为空 */
  command: string
  /** 为什么还拼不出命令 */
  problem: string
  /** 用的是 /extensive */
  extensive: boolean
  /** 要提醒用户的 */
  notes: string[]
}

/** 文件名里不能有的字符（也就不能放进 winfr 的筛选里） */
const BAD_NAME = /["<>|:*?\\/]/

export function isNtfs(fileSystem: string): boolean {
  return fileSystem.trim().toUpperCase() === 'NTFS'
}

/** 找回来的文件默认存到哪个盘：别的盘里优先 U 盘、移动硬盘，其次剩余空间最大的 */
export function defaultTarget(drives: RecoveryDrive[], source: string): string {
  const others = drives.filter((d) => d.letter !== source)
  const pick = [...others].sort((a, b) => Number(b.removable) - Number(a.removable) || b.free - a.free)[0]
  return pick?.letter ?? ''
}

function filter(pattern: string): string {
  return /\s/.test(pattern) ? `/n "${pattern}"` : `/n ${pattern}`
}

export function planRecovery(drives: RecoveryDrive[], choice: RecoveryChoice): RecoveryPlan {
  const fail = (problem: string): RecoveryPlan => ({ command: '', problem, extensive: false, notes: [] })
  const source = drives.find((d) => d.letter === choice.source)
  if (!source) return fail('先选文件原来在哪个盘。')
  if (drives.length < 2) {
    return fail('找回来的文件要存到另一个盘，这台电脑上现在只有一个盘：插上 U 盘或者移动硬盘，再点「刷新」。')
  }
  const target = drives.find((d) => d.letter === choice.target)
  if (!target) return fail('再选找回来的文件存到哪个盘。')
  if (target.letter === source.letter) return fail('找回来的文件不能存回原来的盘（会把还没找回来的文件盖掉），换一个盘。')
  const name = choice.name.trim()
  if (BAD_NAME.test(name)) return fail('文件名里的字不能有 \\ / : * ? " < > | 这些符号，去掉再试。')
  if (!choice.kinds.length && !name) return fail('选一下要找哪些文件，或者填上文件名里有的字。')

  const extensive = choice.thorough || !isNtfs(source.fileSystem)
  const patterns: string[] = []
  if (choice.kinds.length) {
    for (const kind of choice.kinds) {
      for (const ext of KIND_EXTENSIONS[kind]) patterns.push(name ? `*${name}*.${ext}` : `*.${ext}`)
    }
  } else {
    patterns.push(`*${name}*`)
  }
  const command = [`winfr ${source.letter}: ${target.letter}:`, extensive ? '/extensive' : '/regular', ...patterns.map(filter)].join(' ')

  const notes: string[] = []
  if (!isNtfs(source.fileSystem)) {
    notes.push(`${source.letter} 盘是 ${source.fileSystem || '非 NTFS'} 格式（U 盘、存储卡常见），只能用「仔细找」，会慢一些。`)
  }
  if (source.system) notes.push(`${source.letter} 盘是系统盘，电脑开着就一直往里写东西，越早找越好；找之前关掉别的软件。`)
  if (target.free < 1024 ** 3) notes.push(`${target.letter} 盘只剩不到 1 GB，找回来的文件可能放不下，最好换一个空一点的盘。`)
  return { command, problem: '', extensive, notes }
}
