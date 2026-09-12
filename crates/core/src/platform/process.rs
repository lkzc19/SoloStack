//! 与操作系统打交道的进程与端口能力：探活、拉起托管子进程。
//!
//! 本模块**不认识组件** —— 需要注入什么环境变量由调用方给出。组件相关的
//! JAVA_HOME 解析与组件环境变量在 `component::exec` 里组装。

use std::net::TcpStream;
use std::path::Path;
use std::process::Command;

/// 检测端口是否被监听（进程存活判断）。
pub fn port_open(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).is_ok()
}

/// 检测一组端口是否全部开放。
pub fn ports_open(ports: &[u16]) -> bool {
    !ports.is_empty() && ports.iter().all(|p| port_open(*p))
}

/// 在 `dir` 下运行一个脚本（可带参数），注入 `envs` 指定的环境变量。
///
/// - 不注入任何组件专属变量：JAVA_HOME 等由调用方通过 `envs` 传入
/// - 继承当前进程其余环境
/// - 子进程结束即失效，不修改任何全局配置（SoloStack 的红线）
pub fn run_script_in(
    dir: &Path,
    script: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<std::process::Child, String> {
    command_for(dir, script, args, envs)?
        .spawn()
        .map_err(|e| format!("启动脚本 {} 失败: {e}", dir.join(script).display()))
}

/// 同 `run_script_in`，但等待脚本结束并返回其输出（用于需要读 stdout 的脚本，
/// 如 `kafka-storage.sh random-uuid`）。调用方自行检查退出码。
pub fn output_in(
    dir: &Path,
    script: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<std::process::Output, String> {
    command_for(dir, script, args, envs)?
        .output()
        .map_err(|e| format!("执行脚本 {} 失败: {e}", dir.join(script).display()))
}

/// 组装「以 `dir` 为工作目录、bash 执行脚本」的命令（脚本必须存在）。
fn command_for(
    dir: &Path,
    script: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<Command, String> {
    let script_path = dir.join(script);
    if !script_path.exists() {
        return Err(format!("脚本不存在: {}", script_path.display()));
    }
    let mut cmd = Command::new("bash");
    cmd.arg(&script_path).args(args).current_dir(dir);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_open_false_when_closed() {
        // 用大概率无服务的高位端口测试
        assert!(!port_open(65533));
    }

    #[test]
    fn ports_open_false_for_empty_slice() {
        assert!(!ports_open(&[]), "空端口集合不应视为「全部开放」");
    }

    #[test]
    fn run_script_in_rejects_missing_script() {
        let dir = std::env::temp_dir();
        assert!(run_script_in(&dir, "no-such-script-solostack.sh", &[], &[]).is_err());
        assert!(output_in(&dir, "no-such-script-solostack.sh", &[], &[]).is_err());
    }
}
