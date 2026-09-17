//! 真实组件生命周期 smoke test。
//!
//! 默认忽略，避免 `cargo test` 下载与启动 Hadoop/Kafka。显式运行：
//!
//! ```text
//! cargo test -p solostack-core --test lifecycle_smoke -- \
//!   --ignored --nocapture --test-threads=1
//! ```
//!
//! 测试使用独立临时 HOME，完整走安装、首启 init、状态、停止、配置修改、
//! 二次启动、卸载；结束后不会留下实例或常驻进程。

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use solostack_core::app::environment;
use solostack_core::app::paths;
use solostack_core::component::install_config::{InstallConfig, InstallParams};
use solostack_core::component::{instances, schema, ConfigFieldUpdate};
use solostack_core::lifecycle::install;
use solostack_core::lifecycle::service::{self, Status};
use solostack_core::lifecycle::uninstall;
use solostack_core::package::manifest;
use solostack_core::platform::jdk;
use solostack_core::platform::process;

const JDK_VERSION: &str = "17";
const KAFKA_CONTROLLER_PORT: u16 = 9093;
const STATUS_TIMEOUT: Duration = Duration::from_secs(180);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

#[test]
#[ignore = "下载并启动真实 Hadoop/Kafka，显式通过 ignored smoke test 运行"]
fn real_component_lifecycle_smoke() {
    let jdk = jdk::find_matching(JDK_VERSION)
        .unwrap_or_else(|| panic!("smoke test 需要本机 JDK {JDK_VERSION}"));
    std::env::set_var("JAVA_HOME", &jdk.path);

    let components =
        std::env::var("SOLOSTACK_SMOKE_COMPONENTS").unwrap_or_else(|_| "hadoop,kafka".to_string());

    for name in components
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        run_component(name).unwrap_or_else(|error| panic!("{name} smoke test 失败: {error}"));
    }
}

fn run_component(name: &str) -> Result<(), String> {
    let version = manifest::by_component(name)?
        .source
        .first()
        .and_then(|source| source.version.keys().next().cloned())
        .ok_or_else(|| format!("组件 {name} 没有可测版本"))?;
    let source_id =
        std::env::var("SOLOSTACK_SMOKE_SOURCE").unwrap_or_else(|_| "官方源".to_string());
    let mut case = SmokeCase::new(name, &version)?;

    let result = run_lifecycle(&case, &source_id);
    let cleanup = case.cleanup();
    result.and(cleanup)
}

fn run_lifecycle(case: &SmokeCase, source_id: &str) -> Result<(), String> {
    let name = case.name.as_str();
    let version = case.version.as_str();
    let environment_id = case.environment_id.as_str();
    check(
        !instances::is_installed(environment_id, name),
        "临时 HOME 不是干净状态",
    )?;

    let params = install_params(name)?;
    let cancel = AtomicBool::new(false);
    install::install(
        &InstallConfig {
            environment_id: environment_id.to_string(),
            component: name.to_string(),
            version: version.to_string(),
            source_id: source_id.to_string(),
            jdk_version: JDK_VERSION.to_string(),
            params,
        },
        None,
        &cancel,
    )?;
    check(
        instances::is_installed(environment_id, name),
        "安装后未发现组件实例",
    )?;
    check(
        !init_marker(environment_id, name, version)?.exists(),
        "首次启动前不应存在 init 产物",
    )?;

    service::start(environment_id, name, version)?;
    wait_for_status(
        environment_id,
        name,
        version,
        &Status::Running,
        STATUS_TIMEOUT,
    )?;
    let first_identity = read_identity(environment_id, name, version)?;

    service::stop(environment_id, name, version)?;
    wait_for_status(
        environment_id,
        name,
        version,
        &Status::Stopped,
        STATUS_TIMEOUT,
    )?;

    let changed_port = save_restart_change(environment_id, name, version)?;
    service::start(environment_id, name, version)?;
    wait_for_status(
        environment_id,
        name,
        version,
        &Status::Running,
        STATUS_TIMEOUT,
    )?;
    check(
        read_identity(environment_id, name, version)? == first_identity,
        "二次启动重新执行了 init，官方身份产物发生变化",
    )?;
    let changed = schema::list_fields(environment_id, name, version)?
        .into_iter()
        .find(|field| field.id == changed_port.0)
        .ok_or_else(|| format!("重启后未找到配置字段 {}", changed_port.0))?;
    check(
        changed.value == changed_port.1,
        format!(
            "配置修改未在重启后生效: {} 期望 {}，实际 {}",
            changed_port.0, changed_port.1, changed.value
        ),
    )?;

    service::stop(environment_id, name, version)?;
    wait_for_status(
        environment_id,
        name,
        version,
        &Status::Stopped,
        STATUS_TIMEOUT,
    )?;
    Ok(())
}

fn install_params(name: &str) -> Result<InstallParams, String> {
    match name {
        "hadoop" => {
            let mut params = InstallParams::new();
            params.insert("namenode_web_port".into(), free_port(&[]).to_string());
            params.insert("yarn_rm_web_port".into(), free_port(&[]).to_string());
            params.insert("history_enabled".into(), "false".into());
            Ok(params)
        }
        "kafka" => {
            let mut params = InstallParams::new();
            params.insert(
                "broker_port".into(),
                free_port(&[KAFKA_CONTROLLER_PORT]).to_string(),
            );
            params.insert("num_partitions".into(), "1".into());
            Ok(params)
        }
        _ => Err(format!("没有 {name} 的 smoke test 安装参数")),
    }
}

fn save_restart_change(
    environment_id: &str,
    name: &str,
    version: &str,
) -> Result<(String, String), String> {
    let (id, value) = match name {
        "hadoop" => ("yarn_rm_web_port", free_port(&[]).to_string()),
        "kafka" => (
            "broker_port",
            free_port(&[KAFKA_CONTROLLER_PORT]).to_string(),
        ),
        _ => return Err(format!("没有 {name} 的 smoke test 配置变更")),
    };
    schema::save_fields(
        environment_id,
        name,
        version,
        &[ConfigFieldUpdate {
            id: id.to_string(),
            value: value.clone(),
        }],
    )?;
    Ok((id.to_string(), value))
}

fn init_marker(environment_id: &str, name: &str, version: &str) -> Result<PathBuf, String> {
    match name {
        "hadoop" => Ok(paths::var_data_instance_dir(environment_id, name, version)
            .map_err(|e| e.to_string())?
            .join("name/current/VERSION")),
        "kafka" => Ok(paths::var_data_instance_dir(environment_id, name, version)
            .map_err(|e| e.to_string())?
            .join("kafka/meta.properties")),
        _ => Err(format!("没有 {name} 的 init 产物定义")),
    }
}

fn read_identity(environment_id: &str, name: &str, version: &str) -> Result<String, String> {
    let path = init_marker(environment_id, name, version)?;
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
    match name {
        "hadoop" => content
            .lines()
            .find_map(|line| line.strip_prefix("namespaceID="))
            .map(str::to_string)
            .ok_or_else(|| format!("{} 缺少 namespaceID", path.display())),
        "kafka" => content
            .lines()
            .find_map(|line| line.strip_prefix("cluster.id="))
            .map(str::to_string)
            .ok_or_else(|| format!("{} 缺少 cluster.id", path.display())),
        _ => Err(format!("没有 {name} 的身份产物定义")),
    }
}

fn wait_for_status(
    environment_id: &str,
    name: &str,
    version: &str,
    expected: &Status,
    timeout: Duration,
) -> Result<(), String> {
    let started = Instant::now();
    let mut last = service::component_status(environment_id, name, version);
    while started.elapsed() < timeout {
        if &last == expected {
            return Ok(());
        }
        std::thread::sleep(POLL_INTERVAL);
        last = service::component_status(environment_id, name, version);
    }
    Err(format!(
        "等待 {name} 状态 {expected:?} 超时（超过 {timeout:?}），最后状态: {last:?}"
    ))
}

fn free_port(excluded: &[u16]) -> u16 {
    for _ in 0..100 {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        if port >= 1024 && !excluded.contains(&port) && !process::port_open(port) {
            return port;
        }
    }
    panic!("无法分配空闲端口");
}

fn check(condition: bool, message: impl Into<String>) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

/// 临时 HOME + 生命周期清理守卫。
struct SmokeCase {
    name: String,
    version: String,
    environment_id: String,
    root: PathBuf,
    previous_home: Option<std::ffi::OsString>,
    cleaned: bool,
}

impl SmokeCase {
    fn new(name: &str, version: &str) -> Result<Self, String> {
        let previous_home = std::env::var_os("HOME");
        let root = std::env::temp_dir().join(format!(
            "solostack-lifecycle-smoke-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
        }
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        std::env::set_var("HOME", &root);
        let environment = environment::create("Smoke Test").map_err(|e| e.to_string())?;
        environment::set_active_id(Some(&environment.id)).map_err(|e| e.to_string())?;
        Ok(Self {
            name: name.to_string(),
            version: version.to_string(),
            environment_id: environment.id,
            root,
            previous_home,
            cleaned: false,
        })
    }

    fn cleanup(&mut self) -> Result<(), String> {
        if self.cleaned {
            return Ok(());
        }

        let mut errors = Vec::new();
        if instances::is_installed(&self.environment_id, &self.name) {
            let status = service::component_status(&self.environment_id, &self.name, &self.version);
            if status != Status::Stopped {
                if let Err(error) = service::stop(&self.environment_id, &self.name, &self.version) {
                    errors.push(format!("停止失败: {error}"));
                }
                if let Err(error) = wait_for_status(
                    &self.environment_id,
                    &self.name,
                    &self.version,
                    &Status::Stopped,
                    STATUS_TIMEOUT,
                ) {
                    errors.push(error);
                }
            }
            if let Err(error) =
                uninstall::uninstall(&self.environment_id, &self.name, &self.version, false)
            {
                errors.push(format!("卸载失败: {error}"));
            }
        }
        if instances::is_installed(&self.environment_id, &self.name) {
            errors.push("卸载后组件实例仍存在".to_string());
        }

        self.restore_home();
        if let Err(error) = remove_dir(&self.root) {
            errors.push(format!("删除临时 HOME 失败: {error}"));
        }
        self.cleaned = true;
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn restore_home(&self) {
        match &self.previous_home {
            Some(home) => std::env::set_var("HOME", home),
            None => std::env::remove_var("HOME"),
        }
    }
}

impl Drop for SmokeCase {
    fn drop(&mut self) {
        if !self.cleaned {
            let _ = self.cleanup();
        }
    }
}

fn remove_dir(path: &Path) -> Result<(), String> {
    if path.exists() {
        std::fs::remove_dir_all(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
