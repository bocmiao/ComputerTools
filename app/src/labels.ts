import type {
  ApplyResult,
  FeatureStateKind,
  Fixer,
  Maturity,
  Reboot,
  Recommend,
  Risk,
  Status,
} from './api/types'

/** 标签的配色，对应 styles/tokens.css 里的 --tone-* 变量 */
export type Tone = 'ok' | 'advice' | 'manual' | 'unknown' | 'info' | 'neutral'

/** 界面上显示的状态（na 不显示） */
export type ShownStatus = Exclude<Status, 'na'>

export const statusLabel: Record<ShownStatus, string> = {
  ok: '正常',
  advice: '建议处理',
  manual: '需要人工',
  unknown: '没查出来',
}

/** 体检结果的排列顺序：需要人工、建议处理、没查出来在前，正常在后 */
export const statusOrder: Record<Status, number> = {
  manual: 0,
  advice: 1,
  unknown: 2,
  ok: 3,
  na: 4,
}

export const fixerLabel: Record<Fixer, string> = {
  medkit: '小药箱能修',
  system: '用系统自带功能',
  user: '自己在设置里改',
  helper: '找懂哥',
  vendor: '找品牌售后',
  isp: '找运营商',
  hardware: '需要换硬件',
}

export const riskLabel: Record<Risk, string> = {
  safe: '安全',
  caution: '注意',
  danger: '谨慎',
}

export const riskTone: Record<Risk, Tone> = {
  safe: 'ok',
  caution: 'advice',
  danger: 'manual',
}

export const recommendLabel: Record<Recommend, string> = {
  recommended: '推荐',
  optional: '可选',
  'not-recommended': '不推荐',
}

export const recommendTone: Record<Recommend, Tone> = {
  recommended: 'info',
  optional: 'neutral',
  'not-recommended': 'advice',
}

export const maturityLabel: Record<Maturity, string> = {
  guide: '图文指引',
  semi: '半自动',
  'one-click': '一键修复',
}

/**
 * 常用设置里「当前状态」。不用「开启 / 未开启」：功能名常常是「关闭××」，
 * 「关闭快速启动 · 未开启」会被读反。
 */
export const featureStateLabel: Record<FeatureStateKind, string> = {
  applied: '已设置好',
  'not-applied': '还没设置',
  partial: '只设置了一部分',
  unknown: '没查出来',
}

export const featureStateTone: Record<FeatureStateKind, Tone> = {
  applied: 'ok',
  'not-applied': 'neutral',
  partial: 'advice',
  unknown: 'unknown',
}

/** 执行后的复查结果：修好了没 */
export const verifiedLabel: Record<FeatureStateKind, string> = {
  applied: '已生效',
  'not-applied': '还没生效',
  partial: '只生效了一部分',
  unknown: '没能确认',
}

export const verifiedTone: Record<FeatureStateKind, Tone> = {
  applied: 'ok',
  'not-applied': 'advice',
  partial: 'advice',
  unknown: 'unknown',
}

/**
 * 执行一个功能的结果，界面按这四种分开显示（docs/architecture.md 第 9 节）：
 * - done：改好了，复查也确认生效
 * - unverified：改了，但复查没确认生效（message 里有说明）
 * - unchanged：本来就是好的，什么都没改
 * - failed：没改成（引擎已经尽量退回，message 里说明退没退干净）
 */
export type ApplyOutcome = 'done' | 'unverified' | 'unchanged' | 'failed'

export function applyOutcome(r: ApplyResult): ApplyOutcome {
  if (!r.ok) return 'failed'
  // 没确认生效时，不管有没有改动都要让用户知道，所以先于「不用改」判断
  if (r.verified !== 'applied') return 'unverified'
  return r.entryIds.length === 0 ? 'unchanged' : 'done'
}

/** 标题；颜色见 PreviewDialog / BulkApplyDialog 里的 .result-{done,unverified,unchanged,failed} */
export const applyOutcomeTitle: Record<ApplyOutcome, string> = {
  done: '已经改好了',
  unverified: '改了，但还没确认生效',
  unchanged: '不用改',
  failed: '没有改成',
}

/** 改完以后还要做什么；none 时不显示 */
export const rebootLabel: Record<Reboot, string> = {
  none: '',
  explorer: '需要重启资源管理器',
  logoff: '需要注销',
  reboot: '需要重启',
}

/** 多个修改一起做时，取最「重」的那个提示 */
export const rebootWeight: Record<Reboot, number> = {
  none: 0,
  explorer: 1,
  logoff: 2,
  reboot: 3,
}

/** 常用设置页只列这几类，按这个顺序分组 */
export const settingsCategories = [
  { id: 'explorer', title: '资源管理器' },
  { id: 'desktop', title: '桌面' },
  { id: 'taskbar', title: '任务栏' },
  { id: 'start', title: '开始菜单' },
  { id: 'power', title: '电源' },
] as const
