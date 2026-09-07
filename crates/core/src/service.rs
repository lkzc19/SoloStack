use crate::config;
use crate::paths;
use crate::process;

/// 组件运行状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Running,
    Stopped,
    Partial,
    Error(String),
}

/// 启动组件（全启）。启停序列为组件固有差异，按 name 代码分支。
///
/// - hadoop：格式化 NameNode（首次）→ hdfs → yarn →（历史服务器开启则 JobHistory）
/// - kafka：broker（`kafka-server-start.sh -daemon <server.properties>`）
pub fn start(name: &str, version: &str) -> Result<(), String> {
    let _ = crate::app_log::append(crate::app_log::INFO, &format!("启动组件 {name} v{version}"));
    config::prepare_config(name, version)?;
    match name {
        "hadoop" => {
            format_namenode_if_needed(name, version)?;
            process::run_script(name, version, "sbin/start-dfs.sh", &[], &[])?;
            process::run_script(name, version, "sbin/start-yarn.sh", &[], &[])?;
            if config::history_enabled(name, version) {
                process::run_script(name, version, "bin/mapred", &["--daemon", "start", "historyserver"], &[])?;
            }
        }
        "kafka" => {
            let conf = paths::etc_instance_dir(name, version)
                .map_err(|e| e.to_string())?
                .join("server.properties");
            process::run_script(
                name,
                version,
                "bin/kafka-server-start.sh",
                &["-daemon", conf.to_str().unwrap_or_default()],
                &[],
            )?;
        }
        _ => return Err(format!("不支持的组件: {name}")),
    }
    Ok(())
}

/// 停止组件（全停），按 name 逆序。
pub fn stop(name: &str, version: &str) -> Result<(), String> {
    let _ = crate::app_log::append(crate::app_log::INFO, &format!("停止组件 {name} v{version}"));
    match name {
        "hadoop" => {
            if config::history_enabled(name, version) {
                process::run_script(name, version, "bin/mapred", &["--daemon", "stop", "historyserver"], &[])?;
            }
            process::run_script(name, version, "sbin/stop-yarn.sh", &[], &[])?;
            process::run_script(name, version, "sbin/stop-dfs.sh", &[], &[])?;
        }
        "kafka" => {
            process::run_script(name, version, "bin/kafka-server-stop.sh", &[], &[])?;
        }
        _ => return Err(format!("不支持的组件: {name}")),
    }
    Ok(())
}

/// 组件整体运行状态（探活 `.detect-ports`：全部开放 Running；部分 Partial；全关 Stopped）。
pub fn component_status(name: &str, version: &str) -> Status {
    let ports = config::read_detect_ports(name, version);
    let ports = if ports.is_empty() { default_ports(name) } else { ports };
    ports_status(&ports)
}

/// 组件默认端口（代码分支，供 .detect-ports 缺失时回退）。
fn default_ports(name: &str) -> Vec<u16> {
    match name {
        "hadoop" => vec![9870, 9864, 8088, 8042],
        "kafka" => vec![9092],
        _ => vec![],
    }
}

/// 端口集合 → 状态。
fn ports_status(ports: &[u16]) -> Status {
    if process::ports_open(ports) {
        Status::Running
    } else if !ports.is_empty() && ports.iter().any(|p| process::port_open(*p)) {
        Status::Partial
    } else {
        Status::Stopped
    }
}

/// Hadoop NameNode 格式化标记文件：`var/data/<组件>-<版本>/.formatted`。
const FORMAT_MARKER: &str = ".formatted";

/// 首次格式化 NameNode（带重复格式化保护）。
///
/// 只在 NameNode 数据目录不存在时才执行 format；已有数据则跳过。
pub fn format_namenode_if_needed(name: &str, version: &str) -> Result<(), String> {
    let root = paths::var_data_instance_dir(name, version).map_err(|e| e.to_string())?;
    if root.join("name").exists() || root.join(FORMAT_MARKER).exists() {
        return Ok(());
    }

    let instance = paths::instance_dir(name, version).map_err(|e| e.to_string())?;
    let bin_dir = instance.join("bin");
    if !bin_dir.join("hdfs").exists() {
        return Err(format!("未找到 hdfs 命令: {}", bin_dir.join("hdfs").display()));
    }

    println!("首次使用，格式化 NameNode...");
    let mut child = process::run_script(name, version, "bin/hdfs", &["namenode", "-format", "-force"], &[])?;
    let out = child.wait().map_err(|e| format!("等待格式化失败: {e}"))?;
    if !out.success() {
        return Err("NameNode 格式化失败".to_string());
    }

    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    std::fs::write(root.join(FORMAT_MARKER), "formatted").map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_status_unknown_empty_stopped() {
        // 未知组件无默认端口 → Stopped（不依赖真实端口占用）
        let s = component_status("no-such-component", "0.0.0");
        assert_eq!(s, Status::Stopped);
    }
}
