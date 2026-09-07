use std::net::TcpStream;
use std::process::Command;

use crate::jdk;
use crate::paths;

/// 解析 JDK home：优先读配置副本 `.java-home`（安装时确定并写入）；
/// 缺失则从 config json 的支持版本里挑一个本机已装的 JDK。
pub fn resolve_java_home(name: &str, version: &str) -> Result<String, String> {
    if let Some(home) = crate::config::read_java_home(name, version) {
        return Ok(home);
    }
    // 回退：组件版本支持的 JDK 里找本机已装的
    let supported = crate::config_defs::component(name)
        .map(|c| {
            c.java_support
                .get(version)
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for req in supported {
        if let Some(j) = jdk::find_matching(&req) {
            return Ok(j.path.display().to_string());
        }
    }
    // 最后兜底：任意本机 JDK
    if let Some(j) = jdk::scan().first() {
        return Ok(j.path.display().to_string());
    }
    Err(format!("未找到 {name} v{version} 可用的 JDK"))
}

/// 在组件实例目录下运行一个脚本（可带参数），注入组件所需环境变量。
///
/// - 注入 `JAVA_HOME`（解析自本机 JDK）
/// - 注入 `HADOOP_CONF_DIR` 指向配置副本
/// - 继承当前进程其余环境
/// 子进程结束即失效，不修改任何全局配置。
pub fn run_script(
    name: &str,
    version: &str,
    script: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<std::process::Child, String> {
    let instance = paths::instance_dir(name, version).map_err(|e| e.to_string())?;
    let script_path = instance.join(script);
    if !script_path.exists() {
        return Err(format!("脚本不存在: {}", script_path.display()));
    }

    let java_home = resolve_java_home(name, version)?;
    let conf_dir = paths::etc_instance_dir(name, version)
        .map_err(|e| e.to_string())?;

    let mut cmd = Command::new("bash");
    cmd.arg(&script_path)
        .args(args)
        .current_dir(&instance)
        .env("JAVA_HOME", &java_home)
        .env("HADOOP_CONF_DIR", &conf_dir);

    for (k, v) in envs {
        cmd.env(k, v);
    }

    cmd.spawn().map_err(|e| format!("启动脚本 {} 失败: {e}", script_path.display()))
}

/// 检测端口是否被监听（进程存活判断）。
pub fn port_open(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).is_ok()
}

/// 检测一组端口是否全部开放。
pub fn ports_open(ports: &[u16]) -> bool {
    !ports.is_empty() && ports.iter().all(|p| port_open(*p))
}

/// 获取监听指定端口的进程 PID（通过 lsof）。
pub fn pid_for_port(port: u16) -> Option<u32> {
    let out = Command::new("lsof")
        .args(["-ti", &format!("tcp:{}", port)])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u32>()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_open_false_when_closed() {
        // 用大概率无服务的高位端口测试
        assert!(!port_open(65533));
    }
}
