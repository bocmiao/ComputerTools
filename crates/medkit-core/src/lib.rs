//! 电脑小药箱的引擎。
//!
//! - [`model`]：数据文件的类型（也用来生成 JSON Schema）
//! - [`catalog`]：读取和校验 catalog/
//! - [`bundle`]：把数据和脚本打包进 exe，运行时解压并校验
//! - [`engine`]：检测、执行、撤销、修改日志、报告
//! - [`platform`]：和操作系统打交道的接口（Windows 实现 + 测试用的模拟实现）
//! - [`script`]：PowerShell 宿主

pub mod builtin;
pub mod bundle;
pub mod catalog;
pub mod engine;
pub mod error;
pub mod journal;
pub mod lint;
pub mod model;
pub mod platform;
pub mod registry;
pub mod render;
pub mod report;
pub mod script;
pub mod views;
mod yaml;

pub use engine::Engine;
pub use error::{Error, Result};
