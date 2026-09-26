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
