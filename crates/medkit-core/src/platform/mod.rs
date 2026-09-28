//! 引擎和操作系统之间的接口。
//!
//! 引擎只通过这个 trait 读写系统，所以大部分逻辑可以在任何平台上用 [`mock::MockPlatform`] 测试；
//! 真正的 Windows 实现在 [`windows`] 模块里。

use serde::Serialize;

use crate::model::{Edition, StartType};
use crate::registry::{RegRoot, RegValue};

#[cfg(windows)]
mod displays;
#[cfg(windows)]
pub mod explorer_exec;
pub mod mock;
#[cfg(windows)]
mod restart_manager;
#[cfg(windows)]
pub mod shutdown;
#[cfg(windows)]
mod window_info;
#[cfg(windows)]
pub mod windows;
#[cfg(windows)]
mod winsock;

#[derive(Debug, Clone, thiserror::Error)]
pub enum PlatformError {
    #[error("没有权限：{0}")]
    AccessDenied(String),
    #[error("找不到：{0}")]
    NotFound(String),
    #[error("这台电脑上不支持：{0}")]
    Unsupported(String),
    #[error("系统操作失败：{0}")]
    Other(String),
}

pub type PResult<T> = Result<T, PlatformError>;

/// 键盘的几项辅助功能在这次登录里实际开没开（系统当下生效的状态，不是注册表里存的）。
/// 不小心打开以后，看起来就像键盘「按了没反应」。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyboardAids {
    /// 筛选键：按键要按住一会儿才算数，短按、连按被忽略（按住右 Shift 8 秒会打开）
    pub filter_keys: bool,
    /// 粘滞键：Shift、Ctrl、Alt 按一下就算「按着」，直到按下一个键；连按两下会锁住（连按 5 次 Shift 会打开）
    pub sticky_keys: bool,
    /// 鼠标键：小键盘用来移动鼠标指针，按了不出数字（左 Alt + 左 Shift + Num Lock 会打开）
    pub mouse_keys: bool,
}

/// 正在用的一个显示器（「设置 → 屏幕」里列出来的一个）。怎么看结论见 [`crate::builtin`] 里的 display-resolution。
/// 只有型号名和分辨率，没有序列号。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Display {
    /// 显示器自己报的型号名（EDID 里写的，比如「DELL U2414H」）；笔记本的屏幕、虚拟机里常常没有
    pub name: Option<String>,
    /// 笔记本、一体机、平板自带的屏幕（接在内部接口上）
    pub internal: bool,
    /// 现在的分辨率：桌面的宽和高。竖着放（转了 90 度、270 度）的已经换回横着的方向，好和推荐的比
    pub width: u32,
    pub height: u32,
    /// 显示器推荐的分辨率（「设置」里标着「推荐」的那一项，一般就是屏幕本身的像素）；读不到时是 `None`
    pub preferred: Option<(u32, u32)>,
    /// 和别的显示器显示同一个画面（Win + P 选了「复制」）：分辨率只能选几个屏幕都支持的
    pub cloned: bool,
}

/// 正在用的显示器。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Displays {
    /// 现在是远程桌面连着这台电脑：分辨率由连过来的那台电脑决定
    pub remote: bool,
    pub list: Vec<Display>,
}

/// 在用文件的是什么样的程序（重启管理器报的类型），决定界面上怎么说。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileUserKind {
    /// 有窗口的程序
    Window,
    /// 命令行程序
    Console,
    /// 资源管理器
    Explorer,
    /// 没有窗口的后台程序，或者看不出是什么
    Other,
    /// 系统服务
    Service,
    /// Windows 的关键进程，关不掉
    Critical,
}

/// 一个正在用某些文件的进程（打开着它们，或者把它们当作程序模块加载了）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileUser {
    pub pid: u32,
    /// 进程的启动时间（FILETIME），和进程号一起认出同一个进程
    pub started: u64,
    /// 重启管理器给的名字：程序的说明，服务的显示名
    pub app_name: String,
    /// 程序的文件名，不带路径（路径里可能有用户名）；读不到时为 `None`
    pub program: Option<String>,
    /// 服务名（系统服务才有）
    pub service: Option<String>,
    pub kind: FileUserKind,
    /// 在另一个用户的登录会话里（快速切换用户以后，另一个人开着的程序）
    pub other_session: bool,
}

/// 要打开的系统工具，已经按 [`crate::tools`] 的名单解析过（数据里只能写名单里的名字）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenRequest {
    /// System32 下的程序和参数；以 `.msc` 结尾的参数换成 System32 下的绝对路径。`console` 不为空时在新的命令行
    /// 窗口里依次运行这几条命令（见 [`crate::tools::Program::console`]）
    Program { exe: &'static str, args: &'static [&'static str], console: &'static [&'static str] },
    /// 「设置」里的一页：`ms-settings:<page>`
    Settings(&'static str),
    /// 「获取帮助」里微软的疑难解答：`ms-contact-support://smc-to-emerald/<名字>`。没有「获取帮助」应用时是
    /// [`PlatformError::NotFound`]
    GetHelp(&'static str),
    /// 网页（https，网址只能来自 [`crate::tools`] 里的表）：请资源管理器用登录用户的普通权限在默认浏览器里打开，
    /// 不让浏览器跟着小药箱以管理员身份运行。找不到桌面（资源管理器没在运行）时是 [`PlatformError::NotFound`]
    Web(&'static str),
}

/// 屏幕上的一块长方形（像素，和系统的 RECT 一样：右边、下边不算在里面）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScreenRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl ScreenRect {
    pub fn width(&self) -> i32 {
        (self.right - self.left).max(0)
    }

    pub fn height(&self) -> i32 {
        (self.bottom - self.top).max(0)
    }
}

/// 鼠标指着的那个窗口（最外层的那个：网页、按钮这些里面的小窗口算到它们所在的窗口上）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointedWindow {
    pub pid: u32,
    /// 窗口类名，用来认出任务栏、桌面
    pub class: String,
    /// 窗口在屏幕上的位置和大小
    pub rect: ScreenRect,
    /// 窗口所在的那块屏幕的工作区（不含任务栏）
    pub screen: ScreenRect,
    /// 程序的完整路径；读不到（窗口已经关了、受保护的进程）时为 `None`
    pub path: Option<std::path::PathBuf>,
}

/// 程序文件的版本信息里写的几项（资源管理器「属性 → 详细信息」里看到的那些）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileStrings {
    /// 文件说明（FileDescription）
    pub description: Option<String>,
    /// 公司（CompanyName）
    pub company: Option<String>,
    /// 产品名称（ProductName）
    pub product: Option<String>,
}

/// Winsock 目录里的一项（协议提供程序）。怎么看结论见 [`crate::winsock`]。只有文件名，没有路径。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WinsockEntry {
    /// 协议的名字（`szProtocol`），比如「MSAFD Tcpip [TCP/IP]」
    pub protocol: String,
    /// 提供程序 DLL 的文件名（不含文件夹）
    pub file: String,
    /// DLL 在 Windows 文件夹里
    pub in_windows: bool,
    /// DLL 还在
    pub exists: bool,
    /// DLL 版本信息里的公司、产品名称
    pub company: Option<String>,
    pub product: Option<String>,
    /// 协议链长度：1 是基础提供程序，0 是分层协议（LSP）本身，大于 1 是经过 LSP 的协议链
    pub chain_len: i32,
    /// 地址族（2 = IPv4，23 = IPv6）
    pub family: i32,
    /// 套接字类型（1 = 流，TCP 用的）
    pub socket_type: i32,
    /// 64 位 Windows 上给 32 位程序用的那一份目录里的
    pub wow64: bool,
}

/// 「应用和功能」里的一项：卸载信息里的原文，还没整理（整理见 [`crate::window_owner`]）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstalledProgram {
    pub name: String,
    pub publisher: Option<String>,
    pub install_location: Option<String>,
    pub display_icon: Option<String>,
    pub uninstall_string: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UserIdentity {
    pub sid: String,
    /// `电脑名\用户名`，只用于显示和脱敏
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct OsInfo {
    /// 例如「Windows 11 家庭中文版」
    pub caption: String,
    pub build: u32,
    /// 例如 `25H2`
    pub display_version: String,
    /// 注册表里的 EditionID，例如 `CoreCountrySpecific`
    pub edition_id: String,
    pub edition: Option<Edition>,
    pub computer_name: String,
}

pub trait Platform: Send + Sync {
    /// 读一个值；键或值不存在时返回 `Ok(None)`。
    fn reg_get(&self, root: &RegRoot, key: &str, name: &str) -> PResult<Option<RegValue>>;
    fn reg_key_exists(&self, root: &RegRoot, key: &str) -> PResult<bool>;
    /// 写一个值；路径上缺的键会被创建。
    fn reg_set(&self, root: &RegRoot, key: &str, name: &str, value: &RegValue) -> PResult<()>;
    /// 删一个值；本来就不存在也算成功。
    fn reg_delete_value(&self, root: &RegRoot, key: &str, name: &str) -> PResult<()>;
    /// 键为空（没有子键、没有值）时删除它，返回是否删了。
    fn reg_delete_key_if_empty(&self, root: &RegRoot, key: &str) -> PResult<bool>;
    /// 这个用户的注册表（`HKU\<SID>`）现在有没有加载。用户注销以后就没有了，
    /// 这时写到 `HKU\<SID>\…` 的撤销其实什么都没改，必须先拦下来。
    fn user_hive_loaded(&self, sid: &str) -> bool;

    /// 服务的启动类型；服务不存在时返回 `Ok(None)`。
    fn service_get(&self, name: &str) -> PResult<Option<StartType>>;
    fn service_set(&self, name: &str, start_type: StartType) -> PResult<()>;

    /// 登录到这台电脑桌面的用户（不一定是运行本程序的账户）。
    fn interactive_user(&self) -> Option<UserIdentity>;
    /// 运行本程序的账户。
    fn process_user(&self) -> Option<UserIdentity>;
    fn is_admin(&self) -> bool;
    fn os_info(&self) -> OsInfo;

    /// 键盘的几项辅助功能在这次登录里实际开没开。
    fn keyboard_aids(&self) -> PResult<KeyboardAids>;

    /// 打开一个系统工具或「设置」里的一页，不等它关掉。
    /// 程序不存在（精简系统删掉了）时返回 [`PlatformError::NotFound`]，内容是缺的文件名。
    fn open(&self, request: &OpenRequest) -> PResult<()>;

    /// 解开 `@文件,-编号`、`@{包全名?ms-resource://…}` 这类间接字符串（右键菜单项目的名字常这样写）。
    /// 只读资源，不运行文件里的代码；解不开返回 `None`。
    fn indirect_string(&self, source: &str) -> Option<String> {
        let _ = source;
        None
    }

    /// 哪些进程在用这些文件（任何一个都算）。用重启管理器查，只读，不关任何程序。
    /// 已经退出的进程不列。
    fn file_users(&self, files: &[std::path::PathBuf]) -> PResult<Vec<FileUser>> {
        let _ = files;
        Err(PlatformError::Unsupported("查文件被哪些程序占着".into()))
    }

    /// 鼠标现在指着的那个窗口；鼠标下面没有窗口时返回 `Ok(None)`。只读，不动那个窗口。
    fn pointed_window(&self) -> PResult<Option<PointedWindow>> {
        Err(PlatformError::Unsupported("看鼠标指着的窗口是哪个程序的".into()))
    }

    /// 程序文件的版本信息；没有或者读不出来时各项都是 `None`。只读资源，不运行文件。
    fn file_strings(&self, path: &std::path::Path) -> FileStrings {
        let _ = path;
        FileStrings::default()
    }

    /// 「应用和功能」里的程序：这台电脑的（64 位和 32 位的）加上登录用户自己装的。
    fn installed_programs(&self) -> PResult<Vec<InstalledProgram>> {
        Err(PlatformError::Unsupported("列出装了哪些程序".into()))
    }

    /// Winsock 目录：64 位程序用的一份，64 位 Windows 上再加 32 位程序用的一份。只读。
    fn winsock_catalog(&self) -> PResult<Vec<WinsockEntry>> {
        Err(PlatformError::Unsupported("读 Winsock 目录".into()))
    }

    /// 正在用的显示器，现在的分辨率和推荐的分辨率。只读。
    fn displays(&self) -> PResult<Displays> {
        Err(PlatformError::Unsupported("读显示器的分辨率".into()))
    }
}

/// 注册表 EditionID → 版本类型。
pub fn edition_from_id(id: &str) -> Option<Edition> {
    let id = id.trim();
    if id.starts_with("Core") {
        Some(Edition::Home)
    } else if id.starts_with("Professional") {
        Some(Edition::Pro)
    } else if id.starts_with("Education") {
        Some(Edition::Education)
    } else if id.starts_with("Enterprise") || id.starts_with("IoTEnterprise") || id.starts_with("ServerRdsh") {
        Some(Edition::Enterprise)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editions() {
        assert_eq!(edition_from_id("CoreCountrySpecific"), Some(Edition::Home));
        assert_eq!(edition_from_id("Core"), Some(Edition::Home));
        assert_eq!(edition_from_id("Professional"), Some(Edition::Pro));
        assert_eq!(edition_from_id("ProfessionalWorkstation"), Some(Edition::Pro));
        assert_eq!(edition_from_id("EnterpriseS"), Some(Edition::Enterprise));
        assert_eq!(edition_from_id("IoTEnterpriseS"), Some(Edition::Enterprise));
        assert_eq!(edition_from_id("EducationN"), Some(Edition::Education));
        assert_eq!(edition_from_id("ServerStandard"), None);
    }
}
