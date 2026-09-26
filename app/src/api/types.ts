// 前后端接口的类型定义：逐字照抄 docs/architecture.md 第 9 节，这是和 Rust 后端的合同。
// 不要在这里改字段；要改，先改文档和后端，再同步到这里。

export type Status = 'ok' | 'advice' | 'manual' | 'unknown' | 'na'
export type Fixer = 'medkit' | 'system' | 'user' | 'helper' | 'vendor' | 'isp' | 'hardware'
export type Risk = 'safe' | 'caution' | 'danger'
export type Level = 'light' | 'medium' | 'heavy'
export type Recommend = 'recommended' | 'optional' | 'not-recommended'
export type Reboot = 'none' | 'explorer' | 'logoff' | 'reboot'
export type Maturity = 'guide' | 'semi' | 'one-click'
export type FeatureStateKind = 'applied' | 'not-applied' | 'partial' | 'unknown'

export interface SystemInfo {
  osCaption: string; build: number; edition: string
  isAdmin: boolean; interactiveUser: string | null; elevatedUserMismatch: boolean
  appVersion: string; catalogVersion: string
}
export interface ProfileSummary { id: string; title: string; checkCount: number }
export interface SymptomSummary { id: string; title: string; summary: string | null; keywords: string[]; maturity: Maturity }
export interface FeatureSummary {
  id: string; title: string; description: string; category: string
  risk: Risk; level: Level; recommend: Recommend; subjective: boolean; reboot: Reboot
  reversible: boolean; irreversibleReason: string | null
}
export interface CatalogSummary { profiles: ProfileSummary[]; symptoms: SymptomSummary[]; features: FeatureSummary[] }
export interface SymptomStep { check: string; checkTitle: string; stopOn: Status[]; fixes: FeatureSummary[] }
export interface SymptomDetail extends SymptomSummary { causes: string[]; guide: string | null; steps: SymptomStep[] }
export interface CheckResult {
  id: string; title: string; category: string
  status: Status; resultCode: string | null; message: string
  fixer: Fixer | null; next: string | null; links: string[]
  facts: Record<string, unknown>; error: string | null; durationMs: number
}
export interface FeatureState { id: string; state: FeatureStateKind; details: string[]; error: string | null }
export interface PreviewChange { target: string; current: string; planned: string }
export interface Preview { feature: FeatureSummary; changes: PreviewChange[]; willCreateRestorePoint: boolean; notes: string[] }
export interface ApplyResult {
  feature: string; sessionId: string; entryIds: string[]; ok: boolean
  verified: FeatureStateKind; message: string; reboot: Reboot; notes: string[]; error: string | null
}
export interface JournalEntryView {
  id: string; sessionId: string; time: string; feature: string; featureTitle: string
  target: string; before: string; after: string; ok: boolean
  pending: boolean              // 程序在改的过程中退出，状态不确定
  undone: boolean; undoneAt: string | null
  canUndo: boolean              // 界面据此决定显不显示「恢复原状」
  error: string | null
}
export interface JournalSession { id: string; startedAt: string; entries: JournalEntryView[] }
export interface UndoResult { entryId: string; ok: boolean; drift: boolean; message: string; error: string | null }
