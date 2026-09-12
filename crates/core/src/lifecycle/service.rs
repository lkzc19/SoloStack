use crate::component::{self, registry};
use crate::platform::process;

/// 组件运行状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Running,
    Stopped,
    Partial,
    Error(String),
}

/// 启动组件：配置就绪后由组件自己的启停序列执行。
pub fn start(name: &str, version: &str) -> Result<(), String> {
    let _ = crate::app::app_log::append(
        crate::app::app_log::INFO,
        &format!("启动组件 {name} v{version}"),
    );
    component::prepare_config(name, version)?;
    let c = registry::by_component(name).ok_or_else(|| format!("不支持的组件: {name}"))?;
    c.start(version)
}

/// 停止组件。
pub fn stop(name: &str, version: &str) -> Result<(), String> {
    let _ = crate::app::app_log::append(
        crate::app::app_log::INFO,
        &format!("停止组件 {name} v{version}"),
    );
    let c = registry::by_component(name).ok_or_else(|| format!("不支持的组件: {name}"))?;
    c.stop(version)
}

/// 组件整体运行状态：端口由组件从自己的配置文件**精确读**出，
/// 全部开放 Running / 部分 Partial / 全关 Stopped。
pub fn component_status(name: &str, version: &str) -> Status {
    let ports = registry::by_component(name)
        .map(|c| c.detect_ports(version))
        .unwrap_or_default();
    ports_status(&ports)
}

/// 端口集合 → 状态。
fn ports_status(ports: &[u16]) -> Status {
    if process::ports_open(ports) {
        Status::Running
    } else if ports.iter().any(|p| process::port_open(*p)) {
        Status::Partial
    } else {
        Status::Stopped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_status_unknown_empty_stopped() {
        // 未知组件无端口 → Stopped（不依赖真实端口占用）
        let s = component_status("no-such-component", "0.0.0");
        assert_eq!(s, Status::Stopped);
    }
}
