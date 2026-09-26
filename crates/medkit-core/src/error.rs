use crate::platform::PlatformError;
use crate::script::ScriptError;

/// 引擎对外的错误。消息会直接显示在界面上，所以写中文、说人话。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("找不到{kind}：{id}")]
    NotFound { kind: &'static str, id: String },
    #[error("数据有误：{0}")]
    Catalog(String),
    #[error("这项不适用于这台电脑：{0}")]
    NotApplicable(String),
    #[error("{0}")]
    Platform(#[from] PlatformError),
    #[error("{0}")]
    Script(#[from] ScriptError),
    #[error("修改日志读写失败：{0}")]
    Journal(String),
    #[error("{0}")]
    Invalid(String),
    #[error("文件读写失败：{0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

impl Error {
    pub(crate) fn not_found(kind: &'static str, id: &str) -> Self {
        Self::NotFound { kind, id: id.to_owned() }
    }
}
