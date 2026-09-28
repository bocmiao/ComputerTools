//! catalog 里数据文件的类型定义。字段含义见 docs/architecture.md。
//!
//! 这些类型同时用来生成 `schema/` 下的 JSON Schema，所以字段上的文档注释会出现在编辑器补全里。

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 本地化文本：语言标签到文本的映射，必须包含 `zh-CN`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(transparent)]
pub struct Text(pub BTreeMap<String, String>);

impl Text {
    pub const DEFAULT_LANG: &'static str = "zh-CN";

    /// 取指定语言的文本，没有就回退到简体中文。
    pub fn get(&self, lang: &str) -> &str {
        self.0.get(lang).or_else(|| self.0.get(Self::DEFAULT_LANG)).map(String::as_str).unwrap_or("")
    }

    pub fn zh(&self) -> &str {
        self.get(Self::DEFAULT_LANG)
    }

    #[cfg(test)]
    pub fn zh_only(s: &str) -> Self {
        Self(BTreeMap::from([(Self::DEFAULT_LANG.to_owned(), s.to_owned())]))
    }
}

/// 检测结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// 正常
    Ok,
    /// 建议处理
    Advice,
    /// 需要人工
    Manual,
    /// 没查出来
    Unknown,
    /// 不适用（不显示）
    Na,
}

/// 谁能修。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Fixer {
    Medkit,
    System,
    User,
    Helper,
    Vendor,
    Isp,
    Hardware,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Risk {
    Safe,
    Caution,
    Danger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    Light,
    Medium,
    Heavy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Recommend {
    Recommended,
    Optional,
    NotRecommended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Reboot {
    #[default]
    None,
    Explorer,
    Logoff,
    Reboot,
}

/// 功能改的是谁的设置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    /// 登录用户（HKCU 会被解析成登录用户的 HKU\<SID>）
    CurrentUser,
    /// 整台电脑（HKLM、服务）
    Machine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Maturity {
    Guide,
    Semi,
    OneClick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Edition {
    Home,
    Pro,
    Enterprise,
    Education,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RegType {
    Dword,
    Qword,
    String,
    ExpandString,
    MultiString,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum StartType {
    Auto,
    DelayedAuto,
    Manual,
    Disabled,
}

impl StartType {
    pub fn label(self) -> &'static str {
        match self {
            StartType::Auto => "自动",
            StartType::DelayedAuto => "自动（延迟启动）",
            StartType::Manual => "手动",
            StartType::Disabled => "禁用",
        }
    }
}

fn default_timeout() -> u32 {
    15
}

/// 只读检测。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Check {
    /// 全局唯一、永不改名的 ID，例如 `disk.system-free-space`
    pub id: String,
    pub schema_version: u32,
    pub title: Text,
    #[serde(default)]
    pub description: Option<Text>,
    /// disk / network / system / security / hardware / boot / printer …
    pub category: String,
    #[serde(default)]
    pub requires_admin: bool,
    #[serde(default = "default_timeout")]
    pub timeout_sec: u32,
    /// 为 true 时，引擎给脚本传 `-UserHive`
    #[serde(default)]
    pub user_hive: bool,
    pub probe: Probe,
    /// 结果代码 → 展示方式
    pub results: BTreeMap<String, ResultSpec>,
    /// 事实值的说明：新事实的名字 → 从哪个事实、按什么表查出文字。脚本只返回代码（比如错误代码），
    /// 每个代码的中文说明写在这里，模板里用 `{新事实的名字}` 引用。
    #[serde(default)]
    pub fact_labels: BTreeMap<String, FactLabels>,
    #[serde(default)]
    pub references: Vec<String>,
}

/// 按值查说明：事实是字符串或数字时查它自己；是数组时逐个查，有说明的按原顺序连起来。一个都查不到时是空字符串，
/// 所以说明要写成完整的句子（带句号），放在模板里一句话的开头或者结尾。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FactLabels {
    /// 脚本返回的事实的名字
    pub from: String,
    /// 值（数字也写成字符串）→ 说明
    pub values: BTreeMap<String, Text>,
}

/// 检测方式：脚本或内置检测，二选一。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Probe {
    /// 相对于 scripts/ 的路径
    #[serde(default)]
    pub script: Option<String>,
    /// 内置检测的名字，例如 `cpu-features`
    #[serde(default)]
    pub builtin: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultSpec {
    pub status: Status,
    /// 可以用 `{事实名}` 引用脚本返回的事实
    pub message: Text,
    #[serde(default)]
    pub fixer: Option<Fixer>,
    #[serde(default)]
    pub next: Option<Text>,
    /// `symptom:<id>` 或 `feature:<id>`
    #[serde(default)]
    pub links: Vec<String>,
}

/// 原子修复。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Feature {
    pub id: String,
    pub schema_version: u32,
    pub title: Text,
    pub description: Text,
    pub category: String,
    pub risk: Risk,
    pub level: Level,
    pub recommend: Recommend,
    /// 属于个人偏好，而不是建议
    #[serde(default)]
    pub subjective: bool,
    #[serde(default)]
    pub requires_admin: bool,
    #[serde(default)]
    pub reboot: Reboot,
    pub target: Target,
    #[serde(default)]
    pub applies_to: AppliesTo,
    /// 原语列表（和 `run` 二选一）
    #[serde(default)]
    pub actions: Vec<Action>,
    /// Windows 的默认状态
    #[serde(default)]
    pub windows_default: Vec<Action>,
    /// 测试时用来制造故障状态；不写就用 `windows_default`
    #[serde(default)]
    pub break_actions: Vec<Action>,
    #[serde(default)]
    pub detect: Option<ScriptRef>,
    /// 可撤销脚本执行前读取原状态；结果必须包含非空的 before。
    #[serde(default)]
    pub prepare: Option<ScriptRef>,
    /// 执行脚本（和 `actions` 二选一）
    #[serde(default)]
    pub run: Option<ScriptRef>,
    pub undo: Undo,
    #[serde(default)]
    pub irreversible_reason: Option<Text>,
    #[serde(default, rename = "break")]
    pub break_script: Option<ScriptRef>,
    /// 修完以后改用某个检测来复查
    #[serde(default)]
    pub verify: Option<String>,
    #[serde(default)]
    pub references: Vec<String>,
}

impl Feature {
    pub fn is_primitive(&self) -> bool {
        self.run.is_none()
    }

    pub fn reversible(&self) -> bool {
        !matches!(self.undo, Undo::Keyword(UndoKeyword::None))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScriptRef {
    pub script: String,
}

/// `auto` / `none`，或者 `{ script: … }`。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Undo {
    Keyword(UndoKeyword),
    Script(ScriptRef),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum UndoKeyword {
    Auto,
    None,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AppliesTo {
    #[serde(default)]
    pub min_build: Option<u32>,
    #[serde(default)]
    pub max_build: Option<u32>,
    /// 空表示所有版本
    #[serde(default)]
    pub editions: Vec<Edition>,
}

/// 引擎原生支持的原语。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Registry(RegistryAction),
    Service(ServiceAction),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegistryAction {
    /// 以 `HKCU\` 或 `HKLM\` 开头
    pub key: String,
    /// 值的名字，`""` 表示默认值
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "type")]
    pub value_type: Option<RegType>,
    #[serde(default)]
    pub value: Option<serde_json::Value>,
    /// 为 true 表示「这个值不应该存在」
    #[serde(default)]
    pub delete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceAction {
    pub name: String,
    pub start_type: StartType,
}

/// 症状诊断树。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Symptom {
    pub id: String,
    pub schema_version: u32,
    pub title: Text,
    #[serde(default)]
    pub summary: Option<Text>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub causes: Vec<Text>,
    pub maturity: Maturity,
    #[serde(default)]
    pub steps: Vec<Step>,
    #[serde(default)]
    pub guide: Option<Text>,
    /// 手动步骤下面的按钮（和检测结果的 links 一样：`tool:<id>`、`symptom:<id>`、`feature:<id>`），
    /// 给指引里提到的小工具、相关症状
    #[serde(default)]
    pub links: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub check: String,
    /// 这一步的结论在列表里时，不再往下查
    #[serde(default)]
    pub stop_on: Vec<Status>,
    #[serde(default)]
    pub fixes: Vec<String>,
}

/// 检测清单，例如体检。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub schema_version: u32,
    pub title: Text,
    pub checks: Vec<String>,
}

/// 小工具：一次性的操作，不改设置（见 docs/architecture.md 第 11 节）。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub id: String,
    pub schema_version: u32,
    pub group: ToolGroup,
    pub title: Text,
    /// 一两句话：做什么、什么时候用
    pub description: Text,
    /// system / network / disk / hardware / settings …
    pub category: String,
    #[serde(default)]
    pub audience: Audience,
    #[serde(default)]
    pub requires_admin: bool,
    #[serde(default = "default_timeout")]
    pub timeout_sec: u32,
    /// 为 true 时，引擎给脚本传 `-UserHive`
    #[serde(default)]
    pub user_hive: bool,
    /// info、action 的脚本
    #[serde(default)]
    pub run: Option<ScriptRef>,
    /// 只有 action 能写：执行前确认框里的话
    #[serde(default)]
    pub confirm: Option<Text>,
    /// info、action：结果代码 → 展示方式（和检测的一样）
    #[serde(default)]
    pub results: BTreeMap<String, ResultSpec>,
    /// 只有 info 能写：表格里的文字
    #[serde(default)]
    pub labels: ToolLabels,
    /// 只有 open 能写：打开什么
    #[serde(default)]
    pub open: Option<OpenTarget>,
    #[serde(default)]
    pub references: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ToolGroup {
    /// 看信息（只读），结果显示成表格
    Info,
    /// 一键处理（没有持久影响），结果显示成一句话
    Action,
    /// 打开系统自带的工具或「设置」里的一页
    Open,
}

/// 给谁用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Audience {
    #[default]
    Everyone,
    /// 给懂哥用的
    Helper,
}

/// info 小工具表格里的文字。脚本只返回键，文字写在这里。
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolLabels {
    /// 表格标题：section id → 文字
    #[serde(default)]
    pub sections: BTreeMap<String, Text>,
    /// 行的标签：row id → 文字
    #[serde(default)]
    pub rows: BTreeMap<String, Text>,
    /// 行的值：value code → 文字
    #[serde(default)]
    pub values: BTreeMap<String, Text>,
}

/// open 小工具打开什么：`program` 和 `settings` 二选一，都必须在引擎的名单里。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenTarget {
    /// 系统工具的名字，例如 `device-manager`（名单见 docs/architecture.md 11.3）
    #[serde(default)]
    pub program: Option<String>,
    /// 「设置」里的页面，例如 `windowsupdate`（打开 ms-settings:windowsupdate）
    #[serde(default)]
    pub settings: Option<String>,
}
