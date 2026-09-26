//! 界面拿到的数据（和 docs/architecture.md 第 9 节的 TypeScript 类型一一对应）。
//! 字段名序列化成 camelCase，所有文本已经渲染成用户语言。

use serde::Serialize;
use serde_json::{Map, Value};

use crate::model::{Audience, Fixer, Level, Maturity, Reboot, Recommend, Risk, Status, ToolGroup};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeatureStateKind {
    Applied,
    NotApplied,
    Partial,
    Unknown,
}

impl FeatureStateKind {
    pub fn parse(s: &str) -> Self {
        match s {
            "applied" => Self::Applied,
            "not-applied" => Self::NotApplied,
            "partial" => Self::Partial,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub os_caption: String,
    pub build: u32,
    pub edition: String,
    pub is_admin: bool,
    pub interactive_user: Option<String>,
    pub elevated_user_mismatch: bool,
    pub app_version: String,
    pub catalog_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub id: String,
    pub title: String,
    pub check_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymptomSummary {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,
    pub keywords: Vec<String>,
    pub maturity: Maturity,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureSummary {
    pub id: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub risk: Risk,
    pub level: Level,
    pub recommend: Recommend,
    pub subjective: bool,
    pub reboot: Reboot,
    pub reversible: bool,
    pub irreversible_reason: Option<String>,
    /// 这台电脑能不能用这一项（系统版本、Windows 版本不对就不能）
    pub applicable: bool,
    /// 不能用的原因，给用户看
    pub not_applicable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSummary {
    pub profiles: Vec<ProfileSummary>,
    pub symptoms: Vec<SymptomSummary>,
    pub features: Vec<FeatureSummary>,
    pub tools: Vec<ToolSummary>,
}

/// open 小工具打开的是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolOpens {
    /// 系统自带的工具（任务管理器、设备管理器……）
    Program,
    /// 「设置」里的一页
    Settings,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSummary {
    pub id: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub group: ToolGroup,
    /// 只有 open 有值
    pub opens: Option<ToolOpens>,
    pub audience: Audience,
    /// 只有 action 可能有：执行前要用户确认的说明
    pub confirm: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRow {
    pub label: String,
    pub value: String,
    /// 默认遮住，不进「复制全部」（例如 WiFi 密码）
    pub secret: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSection {
    pub title: String,
    pub rows: Vec<ToolRow>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub result_code: Option<String>,
    pub message: String,
    pub next: Option<String>,
    pub links: Vec<String>,
    /// 只有 info 有内容
    pub sections: Vec<ToolSection>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymptomStep {
    pub check: String,
    pub check_title: String,
    /// 这一步的结论在列表里时，不再往下查
    pub stop_on: Vec<Status>,
    pub fixes: Vec<FeatureSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymptomDetail {
    #[serde(flatten)]
    pub summary: SymptomSummary,
    pub causes: Vec<String>,
    pub guide: Option<String>,
    pub steps: Vec<SymptomStep>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub id: String,
    pub title: String,
    pub category: String,
    pub status: Status,
    pub result_code: Option<String>,
    pub message: String,
    pub fixer: Option<Fixer>,
    pub next: Option<String>,
    pub links: Vec<String>,
    pub facts: Map<String, Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureState {
    pub id: String,
    pub state: FeatureStateKind,
    pub details: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewChange {
    pub target: String,
    pub current: String,
    pub planned: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub feature: FeatureSummary,
    pub changes: Vec<PreviewChange>,
    pub will_create_restore_point: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub feature: String,
    pub session_id: String,
    pub entry_ids: Vec<String>,
    pub ok: bool,
    pub verified: FeatureStateKind,
    pub message: String,
    pub reboot: Reboot,
    pub notes: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntryView {
    pub id: String,
    pub session_id: String,
    pub time: String,
    pub feature: String,
    pub feature_title: String,
    pub target: String,
    pub before: String,
    pub after: String,
    pub ok: bool,
    /// 程序在改的过程中退出，没来得及记下结果
    pub pending: bool,
    pub undone: bool,
    pub undone_at: Option<String>,
    /// 界面据此决定显不显示「恢复原状」
    pub can_undo: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalSession {
    pub id: String,
    pub started_at: String,
    pub entries: Vec<JournalEntryView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResult {
    pub entry_id: String,
    pub ok: bool,
    pub drift: bool,
    pub message: String,
    pub error: Option<String>,
}
