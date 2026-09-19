//! SoloStack 自身：磁盘布局、用户设置、操作日志。
//!
//! 本层只关心「应用把东西放在哪、记了什么」，不含任何组件业务概念。

pub mod app_log;
pub mod environment;
pub mod environment_usage;
pub mod id;
pub mod migration;
pub mod paths;
pub mod notification;
pub mod settings;
