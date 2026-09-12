//! 机器与操作系统能力：架构判断、端口探活与子进程、本机 JDK、本机应用扫描。
//!
//! 本层只依赖标准库与 `app` 的路径约定，**不含组件业务概念** ——
//! 组件相关的 JDK 解析与脚本环境注入在 `component` 层完成。

pub mod app_scan;
pub mod arch;
pub mod jdk;
pub mod process;
