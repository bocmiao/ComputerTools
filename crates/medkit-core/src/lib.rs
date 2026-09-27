//! 电脑小药箱的引擎。
//!
//! - [`model`]：数据文件的类型（也用来生成 JSON Schema）
//! - [`catalog`]：读取和校验 catalog/
//! - [`bundle`]：把数据和脚本打包进 exe，运行时解压并校验
//! - [`engine`]：检测、执行、撤销、修改日志、报告
//! - [`platform`]：和操作系统打交道的接口（Windows 实现 + 测试用的模拟实现）
//! - [`script`]：PowerShell 宿主
//! - [`tools`]：小工具能打开的程序名单、info 小工具的表格渲染
//! - [`startup`]：开机启动项的开关（和任务管理器同一个）

pub mod builtin;
pub mod bundle;
pub mod catalog;
pub mod context_menu;
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
pub mod startup;
pub mod tools;
pub mod views;
mod yaml;

pub use engine::Engine;
pub use error::{Error, Result};
