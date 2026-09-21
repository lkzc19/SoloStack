//! 组件生命周期编排：安装、卸载、启停、日志。
//!
//! 本层负责「按什么顺序做、失败了怎么办」，具体行为委托给 `component` 层的组件实现。

pub mod environment;
pub mod install;
pub mod lock;
pub mod service;
pub mod uninstall;
