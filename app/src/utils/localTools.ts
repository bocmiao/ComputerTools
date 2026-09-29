import type { IconName } from '../components/AppIcon.vue'

// 工具箱里在本机处理的工具（不在目录 catalog 里，是界面自己的组件）：名字、一句话、分在哪一类、图标。
// 工具箱的图标网格、单个工具页和导航栏的搜索都用这一份；组件在 views/ToolsView.vue 里按 id 对上。

export type LocalToolGroup = 'text' | 'image' | 'file' | 'test'

export interface LocalTool {
  id: string
  title: string
  /** 一句话：图标下面那一行 */
  short: string
  /** 单个工具页页头的说明 */
  description: string
  group: LocalToolGroup
  icon: IconName
  /** 搜索时额外认的说法 */
  keywords: string[]
}

export const LOCAL_TOOL_GROUPS: { id: LocalToolGroup; title: string }[] = [
  { id: 'text', title: '文字' },
  { id: 'image', title: '图片' },
  { id: 'file', title: '文件' },
  { id: 'test', title: '测试和控制' },
]

export const LOCAL_TOOLS: LocalTool[] = [
  {
    id: 'file-hash',
    title: '文件校验',
    short: 'MD5、SHA-1、SHA-256',
    description: '算出文件的 MD5、SHA-1、SHA-256，和官网公布的值比一比，看下载的文件完不完整、有没有被改过。',
    group: 'text',
    icon: 'hash',
    keywords: ['校验', 'md5', 'sha', 'sha256', '哈希', 'hash', '文件完整'],
  },
  {
    id: 'text-tidy',
    title: '文本整理',
    short: '全角半角、去空行、去重',
    description: '全角半角互换、去掉空行和重复的行、按行排序，顺便数数有多少字。',
    group: 'text',
    icon: 'text',
    keywords: ['全角', '半角', '去重', '空行', '排序', '字数'],
  },
  {
    id: 'json',
    title: 'JSON 格式化',
    short: '格式化、压缩、查错',
    description: '把 JSON 排整齐或者压成一行，格式不对时告诉你错在哪一行。',
    group: 'text',
    icon: 'braces',
    keywords: ['json', '格式化', '压缩', '校验'],
  },
  {
    id: 'text-encode',
    title: '编码转换',
    short: '网址编码、Base64',
    description: '网址参数编码、解码，文字和 Base64 互转。',
    group: 'text',
    icon: 'swap',
    keywords: ['编码', '解码', 'base64', 'url', '网址编码'],
  },
  {
    id: 'text-diff',
    title: '文本对比',
    short: '改过的字标出来',
    description: '合同、通知改过以后看改了哪里：改过的行里不一样的字会标出来。',
    group: 'text',
    icon: 'diff',
    keywords: ['对比', '比较', '差异', '改了哪里'],
  },
  {
    id: 'text-qr',
    title: '文字变二维码',
    short: '网址、取件码发到手机',
    description: '把电脑上的文字、网址变成二维码，手机一扫就拿到。',
    group: 'text',
    icon: 'qr',
    keywords: ['二维码', '网址', '取件码', '扫码'],
  },
  {
    id: 'amount-words',
    title: '金额日期转大写',
    short: '支票、借条、发票',
    description: '金额和日期转成中文大写，照人民银行的写法，写支票、借条、发票用。',
    group: 'text',
    icon: 'money',
    keywords: ['大写', '金额', '人民币', '支票', '借条', '发票'],
  },
  {
    id: 'date-calc',
    title: '日期计算',
    short: '相差几天、工作日',
    description: '两个日期相差几天、几年几个月、中间几个工作日；往后推多少天是哪天。',
    group: 'text',
    icon: 'calendar',
    keywords: ['日期', '天数', '工作日', '倒计时'],
  },
  {
    id: 'image-batch',
    title: '图片批量处理',
    short: '压缩、转格式、加水印',
    description: '一次处理很多张图片：压缩到多少 KB 以内、转格式、改尺寸、加水印，原图不动。',
    group: 'image',
    icon: 'image',
    keywords: ['压缩', '图片', '转格式', '水印', '尺寸', '证件照', 'jpg', 'png'],
  },
  {
    id: 'images-pdf',
    title: '图片合成 PDF',
    short: '证件、合同照片合一个',
    description: '把拍的证件、合同、作业照片按顺序合成一个 PDF，能调顺序、转方向。',
    group: 'image',
    icon: 'pdf',
    keywords: ['pdf', '合成', '扫描', '证件'],
  },
  {
    id: 'long-image',
    title: '长图拼接',
    short: '几张截图拼成一张',
    description: '几张聊天记录、网页截图上下拼成一张长图或者左右并排，能直接粘贴截图。',
    group: 'image',
    icon: 'long-image',
    keywords: ['长图', '拼接', '截图', '拼图'],
  },
  {
    id: 'ocr',
    title: '图片转文字',
    short: '认出截图里的字',
    description: '认出截图、拍的文件里的字，能改、能复制。用 Windows 自带的文字识别，不联网。',
    group: 'image',
    icon: 'scan-text',
    keywords: ['ocr', '文字识别', '识字', '提取文字', '截图转文字'],
  },
  {
    id: 'image-privacy',
    title: '图片隐私清理',
    short: '去掉拍摄地点这些信息',
    description: '发照片之前另存一份干净的：照片里藏着的拍摄地点、手机型号、拍摄时间都去掉，原图不动。',
    group: 'image',
    icon: 'eye-off',
    keywords: ['隐私', '定位', 'exif', '拍摄地点', 'gps'],
  },
  {
    id: 'batch-rename',
    title: '批量重命名',
    short: '序号、替换、改扩展名',
    description: '一次改一个文件夹里很多文件的名字：查找替换、加序号、加前后缀、改扩展名，能撤销。',
    group: 'file',
    icon: 'rename',
    keywords: ['重命名', '改名', '序号', '扩展名'],
  },
  {
    id: 'file-lockers',
    title: '文件删不掉',
    short: '看是哪个程序占着',
    description: '删不掉、改不了名、提示「文件已在另一程序中打开」时，看看是哪个程序占着。只查不改。',
    group: 'file',
    icon: 'file-lock',
    keywords: ['删不掉', '占用', '被占用', '另一程序中打开', '解锁'],
  },
  {
    id: 'popup-owner',
    title: '弹窗是哪个软件的',
    short: '鼠标移上去就知道',
    description: '鼠标移到弹窗上停几秒，看看是哪个程序弹的、属于哪个已安装的软件。只查不改。',
    group: 'file',
    icon: 'popup',
    keywords: ['弹窗', '广告', '哪个软件'],
  },
  {
    id: 'space-finder',
    title: '找大文件和重复文件',
    short: '只查不删',
    description: '找出一个文件夹里最占地方的文件，和内容完全一样的重复文件。只查不删。',
    group: 'file',
    icon: 'search-file',
    keywords: ['大文件', '重复文件', '占空间', 'c盘满'],
  },
  {
    id: 'wechat',
    title: '微信占的地方',
    short: '缓存、很久以前的文件',
    description: '数一数微信的缓存和很久以前的聊天图片、视频、文件各占多少，勾上的放进回收站。聊天记录（文字）不碰。',
    group: 'file',
    icon: 'chat',
    keywords: ['微信', '聊天记录', '缓存', 'c盘满'],
  },
  {
    id: 'hidden-files',
    title: 'U 盘里的文件不见了',
    short: '被病毒藏起来的文件',
    description: 'U 盘里的文件不见了、变成快捷方式时，把被病毒藏起来的文件显示回来，能改回去。',
    group: 'file',
    icon: 'usb',
    keywords: ['u盘', '文件不见', '快捷方式', '隐藏', '病毒'],
  },
  {
    id: 'device-tests',
    title: '屏幕、键盘、鼠标、声音测试',
    short: '坏点、按键、声道、麦克风',
    description: '看屏幕有没有坏点和漏光，试试键盘、鼠标每个键，喇叭左右声道、麦克风和摄像头能不能用。',
    group: 'test',
    icon: 'monitor',
    keywords: ['坏点', '漏光', '键盘测试', '鼠标测试', '声道', '麦克风', '摄像头', '验机'],
  },
  {
    id: 'disk-speed',
    title: '硬盘测速',
    short: '读写快不快',
    description: '测一测硬盘顺序读写和 4K 随机读，看是不是固态、有没有慢得不正常。',
    group: 'test',
    icon: 'gauge',
    keywords: ['测速', '硬盘', '固态', '读写速度', '验机'],
  },
  {
    id: 'keep-awake',
    title: '别让电脑睡着',
    short: '下载、上网课时用',
    description: '下载、上网课、开会时不让电脑自己睡着，关掉就恢复。',
    group: 'test',
    icon: 'coffee',
    keywords: ['不睡眠', '防休眠', '保持唤醒', '熄屏'],
  },
  {
    id: 'brightness',
    title: '显示器亮度',
    short: '在电脑上调亮度',
    description: '台式机接的显示器不用去按显示器上的按钮，在电脑上拖滑块调亮度（要显示器支持 DDC/CI）。',
    group: 'test',
    icon: 'sun',
    keywords: ['亮度', '显示器', '屏幕太亮', '屏幕太暗'],
  },
  {
    id: 'shutdown-timer',
    title: '定时关机',
    short: '多久以后或者到几点',
    description: '多久以后或者到几点自动关机、重启，随时能取消。',
    group: 'test',
    icon: 'clock',
    keywords: ['定时关机', '自动关机', '定时重启'],
  },
]

export function findLocalTool(id: string): LocalTool | undefined {
  return LOCAL_TOOLS.find((t) => t.id === id)
}
