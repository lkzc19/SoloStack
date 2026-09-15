//! 与操作系统打交道的进程与端口能力：探活、拉起托管子进程。
//!
//! 本模块**不认识组件** —— 需要注入什么环境变量由调用方给出。组件相关的
//! JAVA_HOME 解析与组件环境变量在 `component::exec` 里组装。

use std::io::Read;
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const MAX_OUTPUT_CHARS: usize = 4_000;

/// 脚本执行结果。仅在退出码为 0 时返回。
#[derive(Debug)]
pub struct ScriptOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

/// 一个组件实例内部的逻辑运行服务。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSpec {
    pub key: &'static str,
    pub process_needles: Vec<String>,
    pub ports: Vec<u16>,
}

impl ServiceSpec {
    pub fn new(
        key: &'static str,
        process_needles: impl IntoIterator<Item = impl Into<String>>,
        ports: impl IntoIterator<Item = u16>,
    ) -> Self {
        Self {
            key,
            process_needles: process_needles.into_iter().map(Into::into).collect(),
            ports: ports.into_iter().collect(),
        }
    }
}

/// 操作系统中的实际进程身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time: String,
    pub command_line: String,
}

/// 单个逻辑服务的进程身份观测结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceState {
    Running,
    Starting,
    Stopped,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceObservation {
    pub key: &'static str,
    pub state: ServiceState,
    pub process: Option<ProcessIdentity>,
}

/// 检测端口是否被监听（进程存活判断）。
pub fn port_open(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).is_ok()
}

/// 检测一组端口是否全部开放。
pub fn ports_open(ports: &[u16]) -> bool {
    !ports.is_empty() && ports.iter().all(|p| port_open(*p))
}

/// 检查逻辑服务的进程身份和监听端口归属。
pub fn inspect_services(specs: &[ServiceSpec]) -> Result<Vec<ServiceObservation>, String> {
    let processes = scan_processes()?;
    classify_services(specs, &processes, listening_pids)
}

fn classify_services<F>(
    specs: &[ServiceSpec],
    processes: &[ProcessIdentity],
    mut listening_pids: F,
) -> Result<Vec<ServiceObservation>, String>
where
    F: FnMut(u16) -> Result<Vec<u32>, String>,
{
    let mut observations = Vec::with_capacity(specs.len());
    for spec in specs {
        let matched_processes: Vec<&ProcessIdentity> = processes
            .iter()
            .filter(|process| {
                spec.process_needles
                    .iter()
                    .all(|needle| process.command_line.contains(needle))
            })
            .collect();
        let matched_pids: Vec<u32> = matched_processes
            .iter()
            .map(|process| process.pid)
            .collect();
        let mut owners = Vec::new();
        for port in &spec.ports {
            owners.push(listening_pids(*port)?);
        }

        let has_conflict = owners
            .iter()
            .flatten()
            .any(|pid| !matched_pids.contains(pid));
        let owns_all_ports = owners
            .iter()
            .all(|pids| pids.iter().any(|pid| matched_pids.contains(pid)));

        let state = if has_conflict {
            ServiceState::Conflict
        } else if !matched_processes.is_empty() && owns_all_ports {
            ServiceState::Running
        } else if !matched_processes.is_empty() {
            ServiceState::Starting
        } else if owners.iter().any(|pids| !pids.is_empty()) {
            ServiceState::Conflict
        } else {
            ServiceState::Stopped
        };

        observations.push(ServiceObservation {
            key: spec.key,
            state,
            process: matched_processes.first().map(|process| (*process).clone()),
        });
    }
    Ok(observations)
}

fn scan_processes() -> Result<Vec<ProcessIdentity>, String> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,lstart=,command="])
        .output()
        .map_err(|e| format!("执行 ps 失败: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "执行 ps 失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_process_line)
        .collect())
}

fn parse_process_line(line: &str) -> Option<ProcessIdentity> {
    let mut fields = line.split_whitespace();
    let pid = fields.next()?.parse().ok()?;
    let start_time = (0..5)
        .map(|_| fields.next())
        .collect::<Option<Vec<_>>>()?
        .join(" ");
    let command_line = fields.collect::<Vec<_>>().join(" ");
    if command_line.is_empty() {
        return None;
    }
    Some(ProcessIdentity {
        pid,
        start_time,
        command_line,
    })
}

fn listening_pids(port: u16) -> Result<Vec<u32>, String> {
    let query = format!("-iTCP:{port}");
    let output = Command::new("lsof")
        .args(["-nP", &query, "-sTCP:LISTEN", "-t"])
        .output()
        .map_err(|e| format!("执行 lsof 失败: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut pids: Vec<u32> = stdout
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect();
    pids.sort_unstable();
    pids.dedup();
    if pids.is_empty() && !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            return Err(format!("查询端口 {port} 失败: {}", stderr.trim()));
        }
    }
    Ok(pids)
}

/// 在 `dir` 下执行脚本，等待结束并检查退出码。
///
/// - 不注入任何组件专属变量：JAVA_HOME 等由调用方通过 `envs` 传入
/// - 继承当前进程其余环境
/// - 捕获 stdout / stderr，失败时返回可诊断的摘要
/// - 超时后终止整个受管进程组，不遗留子进程
pub fn run_script_in(
    dir: &Path,
    script: &str,
    args: &[&str],
    envs: &[(&str, &str)],
    timeout: Duration,
) -> Result<ScriptOutput, String> {
    let mut command = command_for(dir, script, args, envs)?;
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let mut child = command
        .spawn()
        .map_err(|e| format!("启动脚本 {} 失败: {e}", dir.join(script).display()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("无法捕获脚本输出: {}", dir.join(script).display()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("无法捕获脚本错误: {}", dir.join(script).display()))?;

    let stdout = std::thread::spawn(move || read_stream(stdout));
    let stderr = std::thread::spawn(move || read_stream(stderr));
    let started = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = join_output(stdout);
                let stderr = join_output(stderr);
                if !status.success() {
                    return Err(failure_message(dir, script, args, status, &stdout, &stderr));
                }
                return Ok(ScriptOutput {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) if started.elapsed() >= timeout => {
                terminate_child(&mut child);
                let stdout = join_output(stdout);
                let stderr = join_output(stderr);
                return Err(timeout_message(
                    dir, script, args, timeout, &stdout, &stderr,
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                terminate_child(&mut child);
                let _ = join_output(stdout);
                let _ = join_output(stderr);
                return Err(format!(
                    "等待脚本 {} 结束失败: {e}",
                    dir.join(script).display()
                ));
            }
        }
    }
}

fn read_stream(mut stream: impl Read) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn join_output(handle: std::thread::JoinHandle<std::io::Result<String>>) -> String {
    handle.join().ok().and_then(Result::ok).unwrap_or_default()
}

fn terminate_child(child: &mut Child) {
    #[cfg(unix)]
    {
        // 子进程以自身 PID 为进程组 ID 启动，负 PID 用于终止整个进程树。
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn failure_message(
    dir: &Path,
    script: &str,
    args: &[&str],
    status: ExitStatus,
    stdout: &str,
    stderr: &str,
) -> String {
    format!(
        "脚本 {} 执行失败（{status}）\n{}",
        command_label(dir, script, args),
        output_summary(stdout, stderr)
    )
}

fn timeout_message(
    dir: &Path,
    script: &str,
    args: &[&str],
    timeout: Duration,
    stdout: &str,
    stderr: &str,
) -> String {
    format!(
        "脚本 {} 执行超时（超过 {:?}）\n{}",
        command_label(dir, script, args),
        timeout,
        output_summary(stdout, stderr)
    )
}

fn command_label(dir: &Path, script: &str, args: &[&str]) -> String {
    let mut label = dir.join(script).display().to_string();
    if !args.is_empty() {
        label.push(' ');
        label.push_str(&args.join(" "));
    }
    label
}

fn output_summary(stdout: &str, stderr: &str) -> String {
    let mut parts = Vec::new();
    if !stdout.trim().is_empty() {
        parts.push(format!("stdout:\n{}", truncate_output(stdout)));
    }
    if !stderr.trim().is_empty() {
        parts.push(format!("stderr:\n{}", truncate_output(stderr)));
    }
    if parts.is_empty() {
        "无输出".to_string()
    } else {
        parts.join("\n")
    }
}

fn truncate_output(value: &str) -> String {
    let trimmed = value.trim();
    let mut chars = trimmed.chars();
    let mut out: String = chars.by_ref().take(MAX_OUTPUT_CHARS).collect();
    if chars.next().is_some() {
        out.push_str("\n...[输出已截断]");
    }
    out
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
        assert!(run_script_in(
            &dir,
            "no-such-script-solostack.sh",
            &[],
            &[],
            Duration::from_secs(1)
        )
        .is_err());
    }

    fn script_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("solostack-process-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn successful_script_returns_captured_output() {
        let dir = script_dir("success");
        std::fs::write(
            dir.join("test.sh"),
            "printf 'ok\\n'\nprintf 'warn\\n' >&2\n",
        )
        .unwrap();

        let output = run_script_in(&dir, "test.sh", &[], &[], Duration::from_secs(1)).unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, "ok\n");
        assert_eq!(output.stderr, "warn\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_zero_script_returns_stdout_and_stderr() {
        let dir = script_dir("failure");
        std::fs::write(
            dir.join("test.sh"),
            "printf 'partial-out\\n'\nprintf 'boom\\n' >&2\nexit 7\n",
        )
        .unwrap();

        let err = run_script_in(&dir, "test.sh", &[], &[], Duration::from_secs(1)).unwrap_err();
        assert!(err.contains("执行失败"), "{err}");
        assert!(err.contains("partial-out"), "{err}");
        assert!(err.contains("boom"), "{err}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_process_group_and_returns_partial_output() {
        let dir = script_dir("timeout");
        let pid_file = dir.join("child.pid");
        std::fs::write(
            dir.join("test.sh"),
            format!(
                "echo $$ > '{}'\nprintf 'starting\\n'\nsleep 30\n",
                pid_file.display()
            ),
        )
        .unwrap();

        let started = Instant::now();
        let err = run_script_in(&dir, "test.sh", &[], &[], Duration::from_millis(100)).unwrap_err();
        assert!(err.contains("执行超时"), "{err}");
        assert!(err.contains("starting"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(2), "超时未及时终止");

        let pid: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let alive = unsafe { libc::kill(pid, 0) == 0 };
        assert!(!alive, "超时后脚本进程仍然存活");

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn process(pid: u32, command_line: &str) -> ProcessIdentity {
        ProcessIdentity {
            pid,
            start_time: "Mon Sep 14 15:00:00 2026".into(),
            command_line: command_line.into(),
        }
    }

    #[test]
    fn service_is_running_only_when_owned_process_listens() {
        let specs = [ServiceSpec::new(
            "namenode",
            ["/managed/hadoop", "NameNode"],
            [9870, 8020],
        )];
        let processes = [process(42, "/managed/hadoop/bin/java NameNode")];

        let observed = classify_services(&specs, &processes, |_| Ok(vec![42])).unwrap();
        assert_eq!(observed[0].state, ServiceState::Running);
        assert_eq!(observed[0].process.as_ref().unwrap().pid, 42);
    }

    #[test]
    fn service_is_starting_when_process_exists_but_port_is_not_ready() {
        let specs = [ServiceSpec::new(
            "namenode",
            ["/managed/hadoop", "NameNode"],
            [9870],
        )];
        let processes = [process(42, "/managed/hadoop/bin/java NameNode")];

        let observed = classify_services(&specs, &processes, |_| Ok(Vec::new())).unwrap();
        assert_eq!(observed[0].state, ServiceState::Starting);
    }

    #[test]
    fn service_conflicts_when_port_belongs_to_another_process() {
        let specs = [ServiceSpec::new(
            "namenode",
            ["/managed/hadoop", "NameNode"],
            [9870],
        )];
        let processes = [process(42, "/managed/hadoop/bin/java NameNode")];

        let observed = classify_services(&specs, &processes, |_| Ok(vec![99])).unwrap();
        assert_eq!(observed[0].state, ServiceState::Conflict);
    }

    #[test]
    fn service_is_stopped_when_neither_process_nor_port_exists() {
        let specs = [ServiceSpec::new(
            "namenode",
            ["/managed/hadoop", "NameNode"],
            [9870],
        )];

        let observed = classify_services(&specs, &[], |_| Ok(Vec::new())).unwrap();
        assert_eq!(observed[0].state, ServiceState::Stopped);
    }
}
