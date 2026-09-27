//! 引擎和操作系统之间的接口。
//!
//! 引擎只通过这个 trait 读写系统，所以大部分逻辑可以在任何平台上用 [`mock::MockPlatform`] 测试；
//! 真正的 Windows 实现在 [`windows`] 模块里。

use serde::Serialize;

use crate::model::{Edition, StartType};
use crate::registry::{RegRoot, RegValue};

pub mod mock;
#[cfg(windows)]
pub mod windows;

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

/// 要打开的系统工具，已经按 [`crate::tools`] 的名单解析过（数据里只能写名单里的名字）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenRequest {
    /// System32 下的程序和参数；以 `.msc` 结尾的参数换成 System32 下的绝对路径
    Program { exe: &'static str, args: &'static [&'static str] },
    /// 「设置」里的一页：`ms-settings:<page>`
    Settings(&'static str),
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
