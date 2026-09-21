//! Kafka 组件实现（KRaft 单节点）。
//!
//! 配置直接读写解压包里的官方 `config/server.properties`：只覆盖受管键，
//! 官方模板里的注释与其它默认值原样保留。
//!
//! | 文件 | 职责 |
//! |---|---|
//! | `mod.rs` | 组件类型声明 + `Component` 实现 |
//! | `config/` | 配置布局、生效值 `Effective`、生成与字段读写（`ConfigLifecycle` / `FieldSchema`） |
//! | `runtime.rs` | 启停序列（`Runtime`；Kafka 无 WebUI） |

mod config;
mod runtime;

use super::Component;

pub struct Kafka;

/// 组件名（等于 `package/manifest/<component>.json` 的文件名）。
pub(super) const NAME: &str = "kafka";

/// KRaft controller 内部固定端口（broker 避让时跳过）。
pub(super) const CONTROLLER_PORT: u16 = 9093;
/// `controller.quorum.voters` 里的节点标识（单节点）。
pub(super) const NODE_ID: u32 = 1;

impl Component for Kafka {
    fn display_name(&self) -> &'static str {
        "Kafka"
    }
}
