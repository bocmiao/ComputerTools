//! 界面拿到的数据（和 docs/architecture.md 第 9 节的 TypeScript 类型一一对应）。
//! 字段名序列化成 camelCase，所有文本已经渲染成用户语言。

use serde::{Deserialize, Serialize};
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
    /// 「获取帮助」里微软的疑难解答
    GetHelp,
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
    /// 值是给手机扫的二维码内容（例如扫码连 WiFi），界面画成二维码
    pub qr: bool,
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
    /// 手动步骤下面的按钮（`tool:<id>`、`symptom:<id>`、`feature:<id>`）
    pub links: Vec<String>,
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

/// 改键（键位重映射）里能选的一个键（名单见 keymap.rs 的 `KEYS`）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyOption {
    /// 和浏览器 `KeyboardEvent.code` 一样的名字
    pub id: String,
    pub label: String,
    /// 只能当「变成」的键（音量、播放这类多媒体键）
    pub target_only: bool,
}

/// 现在的一条改键：按下 `from` 变成 `to`（`to` 为空：这个键不起作用）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyMappingView {
    pub from: String,
    pub to: Option<String>,
    /// 说成人话，例如「Caps Lock（大写锁定） → 左 Ctrl」
    pub text: String,
}

/// 改键的现状。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyRemapView {
    pub keys: Vec<KeyOption>,
    pub mappings: Vec<KeyMappingView>,
    /// 现在的设置里有小药箱认不出来的键或者格式（别的改键软件设的）：只能整个清掉，不能在这里改
    pub foreign: bool,
    /// 认不出来的那些，说成人话（扫描码），给用户看
    pub foreign_text: Option<String>,
}

/// 图片转文字的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum OcrStatus {
    /// 认完了（可能一个字也没认出来）
    Ok,
    /// 这台电脑一种文字识别都没装
    NoLanguage,
    /// 这台电脑上没有 Windows 的文字识别（很老或者精简过的系统）
    Unsupported,
    /// Windows 读不了这张图片
    BadImage,
}

/// 图片转文字：认出来的文字和用的识别语言。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrView {
    pub status: OcrStatus,
    /// 认出来的文字，一行一行（中文的字之间没有空格）
    pub text: String,
    /// 几行
    pub lines: usize,
    /// 用的识别语言，说成人话（「中文（简体）」）
    pub language: Option<String>,
    /// 这台电脑装了的识别语言
    pub languages: Vec<String>,
    /// 装了中文的识别
    pub chinese: bool,
    /// 图片太长，只认了前面一部分
    pub truncated: bool,
    /// 读不了图片、没有文字识别时 Windows 自己的说法（给懂哥看）
    pub detail: Option<String>,
}

/// 开机启动项的程序签名（Get-AuthenticodeSignature 的结论）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StartupSignature {
    /// 有有效的数字签名
    Valid,
    /// 没有签名
    Unsigned,
    /// 签名无效（文件被改过，或者证书不受信任）
    Invalid,
    /// 没查出来（找不到文件等）
    #[default]
    Unknown,
    /// 来不及查（签名要读整个文件，大文件很慢，脚本有时间上限）
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StartupAdvice {
    /// 建议保持原样（杀毒软件、输入法、系统组件、硬件驱动）
    Keep,
    /// 不需要一开机就用的话，可以停用
    CanDisable,
}

/// 一个开机启动项。路径只在本机界面上显示，不进诊断报告。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupItem {
    /// 改开关时原样传回来（来源加名字）
    pub id: String,
    pub source: crate::startup::Source,
    /// 注册表里的值名或者「启动」文件夹里的文件名
    pub name: String,
    /// 显示的名字：程序文件里写的说明，没有就用 name
    pub title: String,
    /// 程序的文件名
    pub program: String,
    /// 程序的完整路径（读不出来是空的）
    pub path: String,
    pub exists: bool,
    /// 签名的发布者，没有签名时是文件里写的公司名
    pub publisher: Option<String>,
    pub signature: StartupSignature,
    /// 「当前用户（注册表）」这类说明
    pub location: String,
    /// 开机时会不会自动启动（任务管理器里的开关）
    pub enabled: bool,
    pub advice: StartupAdvice,
    /// 建议的理由
    pub reason: String,
}

/// 右键菜单里的一项是哪一类（界面据此说明拿掉会怎样）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContextMenuKind {
    /// 菜单命令（`shell\<名字>`），下次右键就生效
    Command,
    /// 软件的外壳扩展（按 CLSID 拿掉），要重启资源管理器才生效
    Extension,
    /// Windows 11 新菜单里应用加的项目（也按 CLSID 拿掉）
    App,
}

/// 右键菜单里软件加的一项。路径只在本机界面上显示，不进诊断报告。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextMenuItem {
    /// 改开关时原样传回来
    pub id: String,
    pub kind: ContextMenuKind,
    /// 菜单上的字（命令），或者扩展、应用的名字
    pub title: String,
    /// 程序的文件名
    pub program: String,
    /// 程序的完整路径（读不出来是空的）
    pub path: String,
    pub exists: bool,
    /// 签名的发布者，没有签名时是文件里写的公司名，应用是清单里的发布者
    pub publisher: Option<String>,
    pub signature: StartupSignature,
    /// 在哪里右键时出现：「文件」「文件夹空白处」这类
    pub scopes: Vec<String>,
    /// 命令：「所有用户」或「当前用户」；扩展和应用拿掉时对所有用户生效，是空的
    pub location: String,
    /// 现在在菜单里显示不显示
    pub visible: bool,
    /// 只在按住 Shift 再右键时显示
    pub shift_only: bool,
    /// 补充说明（找不到程序、有子菜单这类），没有是空的
    pub note: String,
}

/// 「新建」菜单里的一项（软件加的「新建 Word 文档」这类）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewMenuItem {
    /// 扩展名（小写），改开关时原样传回来
    pub id: String,
    /// 菜单上的字
    pub title: String,
    pub ext: String,
    /// Windows 自带的（位图图像、文本文档、压缩文件夹这类）
    pub windows_own: bool,
    /// 在谁的注册表里：「所有用户」「当前用户」「所有用户和当前用户」
    pub location: String,
    /// 现在在菜单里显示不显示
    pub visible: bool,
}

/// 资源管理器里的图标在哪。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellPlace {
    /// 左边导航栏的最上面一层（和「此电脑」「网络」并列）
    Nav,
    /// 「此电脑」里
    Pc,
}

/// 软件加在资源管理器导航栏或者「此电脑」里的一个图标（网盘、WPS 云文档这些）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellPlaceItem {
    /// 大写的 CLSID（带花括号），改开关时原样传回来
    pub id: String,
    /// 资源管理器里显示的名字
    pub title: String,
    /// 在哪（导航栏的在前）；两处都有的一起隐藏
    pub places: Vec<ShellPlace>,
    /// Windows 自带的（OneDrive、图库、3D 对象这些）
    pub windows_own: bool,
    /// 现在显示不显示
    pub visible: bool,
    /// 要特别说明的（比如是在所有用户的设置里隐藏的），多数是空的
    pub note: String,
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
    /// 恢复以后要做什么才能看到效果（重启资源管理器、注销……）；没恢复成功时是 none
    pub reboot: Reboot,
}

/// 「弹窗是哪个软件的」：鼠标指着的是什么。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowOwnerKind {
    /// 装在这台电脑上的软件（不在 Windows 文件夹里）
    Program,
    /// Windows 自带的程序
    System,
    /// Windows 显示的通知（右下角弹出的那种，别的软件、网站发的通知也是它显示的）
    Notification,
    /// 任务栏或者桌面
    Shell,
    /// 小药箱自己的窗口
    Medkit,
    /// 鼠标下面没有窗口
    Nothing,
    /// 有窗口，但读不到是哪个程序的（窗口已经关了，或者是受保护的系统进程）
    Unreadable,
}

/// 窗口在屏幕上的哪一块：铺满整个屏幕，或者按中心点落在九宫格的哪一格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowPosition {
    Full,
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

/// 「弹窗是哪个软件的」的结果。路径里的用户文件夹名换成 `*`；完整路径留在后端，只用来「打开所在的文件夹」。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowOwnerReport {
    pub kind: WindowOwnerKind,
    /// 程序的文件名
    pub exe: Option<String>,
    /// 程序的说明、公司、产品名（程序文件的版本信息里写的）
    pub description: Option<String>,
    pub company: Option<String>,
    pub product: Option<String>,
    /// 程序所在的文件夹
    pub folder: Option<String>,
    /// 它属于「应用和功能」里的哪个软件（能在那里卸载）
    pub installed: Option<String>,
    pub publisher: Option<String>,
    pub position: Option<WindowPosition>,
    pub width: i32,
    pub height: i32,
}

/// 「文件删不掉：是谁占着」查的是选中的几个文件，还是一个文件夹。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileLockMode {
    Files,
    Folder,
}

/// 「文件删不掉：是谁占着」的结果。只有文件名（查文件夹时是相对这个文件夹的路径），没有完整路径。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileLockReport {
    pub mode: FileLockMode,
    /// 选中的文件的名字，或者文件夹的名字
    pub targets: Vec<String>,
    /// 实际查了几个文件
    pub checked: usize,
    /// 选好以后又不见了的文件（多半已经删掉或者改了名）
    pub missing: Vec<String>,
    /// 没能查的文件
    pub failed: Vec<String>,
    /// 选的文件太多，或者文件夹里的文件太多，只查了前面一部分
    pub truncated: bool,
    /// 文件夹里打不开的子文件夹（没有权限），里面的文件没查
    pub unreadable: usize,
    pub users: Vec<FileLockUser>,
}

/// 一个在用这些文件的程序。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileLockUser {
    pub pid: u32,
    /// 显示的名字：程序的说明、服务的显示名，没有就用程序的文件名
    pub name: String,
    /// 程序的文件名（不带路径）
    pub program: Option<String>,
    pub kind: crate::platform::FileUserKind,
    /// 服务名（系统服务才有）
    pub service: Option<String>,
    /// 它在用的文件；查文件夹、里面的文件又很多时说不出是哪几个，为空
    pub files: Vec<String>,
    /// 还有几个文件没列出来
    pub more_files: usize,
    /// 就是小药箱自己
    pub is_self: bool,
    /// 在另一个用户的登录会话里
    pub other_session: bool,
}
