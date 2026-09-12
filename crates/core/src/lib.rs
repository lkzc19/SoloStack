//! SoloStack 核心库：本机大数据组件的全生命周期管理。
//!
//! # 分层
//!
//! 依赖**只向下、不成环**：
//!
//! ```text
//! lifecycle  ──►  component  ──►  config / package / platform / app
//! ```
//!
//! | 层 | 职责 | 不含 |
//! |---|---|---|
//! | `app` | SoloStack 自身：磁盘布局、设置、操作日志 | 组件概念 |
//! | `platform` | 机器与 OS 能力：架构、端口/子进程、本机 JDK、本机应用扫描 | 组件业务 |
//! | `config` | 配置文件读写（多格式键值 IO） | 组件概念 |
//! | `package` | 安装包获取：下载源清单、下载、解压 | 组件概念 |
//! | `component` | 组件抽象与实现、实例发现、配置字段调度、脚本执行 | 生命周期编排 |
//! | `lifecycle` | 安装 / 卸载 / 启停 / 日志等生命周期编排 | 组件内部实现细节 |

pub mod app;
pub mod component;
pub mod config;
pub mod lifecycle;
pub mod package;
pub mod platform;

/// 测试工具：串行化修改全局 HOME 的测试（Rust 测试并行运行，直接改 env 会竞态）。
#[cfg(test)]
pub(crate) mod test_util {
    use std::sync::Mutex;
    pub(crate) static HOME_LOCK: Mutex<()> = Mutex::new(());
}
