//! Tauri 命令分组：按关注点拆到子模块。
//!
//! 命令层只做「参数转换 → 调 core → 错误映射」，**不含业务逻辑**。

pub mod app;
pub mod component;
pub mod environment;
pub mod install;
pub mod log_stream;
pub mod logs;
pub mod notification;

use solostack_core::component::instances;

/// 按组件名解析已安装实例（失败报「未安装」）。
pub(crate) fn resolve(
    environment_id: &str,
    component: &str,
) -> Result<instances::Installed, String> {
    instances::resolve(environment_id, component)
}
