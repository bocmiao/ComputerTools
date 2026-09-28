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
//! - [`shell_places`]：资源管理器导航栏和「此电脑」里软件加的图标，能隐藏、能恢复
//! - [`keymap`]：改键（键位重映射）：Windows 自带的扫描码映射，读、写、说成人话
//! - [`ocr`]：图片转文字（Windows 自带的文字识别）的结果整理
//! - [`recycle_bin`]：回收站坏了时清空并重建（照微软的办法删掉盘上的 $Recycle.Bin）
//! - [`exe_info`]：「此应用无法在你的电脑上运行」：程序文件本身能不能在这台电脑上运行（只读文件头）
//! - [`lockers`]：文件删不掉时，看是哪些程序在用它（只读）
//! - [`disk_speed`]：硬盘测速（只写一个关掉就删的临时文件）
//! - [`window_owner`]：弹窗是哪个软件的：鼠标指着的窗口是哪个程序的（只读）
//! - [`winsock`]：Winsock 目录里有没有第三方的网络组件（LSP）、文件已经不在的组件（内置检测 `winsock`）

pub mod builtin;
pub mod bundle;
pub mod catalog;
pub mod context_menu;
pub mod disk_speed;
pub mod engine;
pub mod error;
pub mod exe_info;
pub mod journal;
pub mod keymap;
pub mod lint;
pub mod lockers;
pub mod model;
pub mod new_menu;
pub mod ocr;
pub mod platform;
pub mod recycle_bin;
pub mod registry;
pub mod render;
pub mod report;
pub mod script;
pub mod shell_places;
pub mod startup;
pub mod tools;
pub mod views;
pub mod window_owner;
pub mod winsock;
mod yaml;

pub use engine::Engine;
pub use error::{Error, Result};
