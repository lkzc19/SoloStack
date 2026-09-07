pub mod app_log;
pub mod app_scan;
pub mod arch;
pub mod config;
pub mod config_defs;
pub mod config_schema;
pub mod configgen;
pub mod download;
pub mod extract;
pub mod install;
pub mod install_config;
pub mod instances;
pub mod jdk;
pub mod logs;
pub mod paths;
pub mod process;
pub mod service;
pub mod settings;
pub mod uninstall;

/// 测试工具：串行化修改全局 HOME 的测试（Rust 测试并行运行，直接改 env 会竞态）。
#[cfg(test)]
pub(crate) mod test_util {
    use std::sync::Mutex;
    pub(crate) static HOME_LOCK: Mutex<()> = Mutex::new(());
}
