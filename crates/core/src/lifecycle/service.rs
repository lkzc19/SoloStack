use crate::component::{self, registry};
use crate::platform::process::{self, ServiceObservation, ServiceState};

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
    let operation = crate::app::app_log::Operation::begin(
        "start",
        name,
        version,
        &format!("启动组件 {name} v{version}"),
    );
    let result = (|| {
        component::prepare_config(name, version)?;
        let c = registry::by_component(name).ok_or_else(|| format!("不支持的组件: {name}"))?;
        c.init(version)?;
        c.start(version)
    })();
    operation.finish(&result);
    result
}

/// 停止组件。
pub fn stop(name: &str, version: &str) -> Result<(), String> {
    let operation = crate::app::app_log::Operation::begin(
        "stop",
        name,
        version,
        &format!("停止组件 {name} v{version}"),
    );
    let result = (|| {
        let c = registry::by_component(name).ok_or_else(|| format!("不支持的组件: {name}"))?;
        c.stop(version)
    })();
    operation.finish(&result);
    result
}

/// 组件整体运行状态：端口由组件从自己的配置文件**精确读**出，
/// 全部开放 Running / 部分 Partial / 全关 Stopped。
pub fn component_status(name: &str, version: &str) -> Status {
    let Some(component) = registry::by_component(name) else {
        return Status::Stopped;
    };
    let specs = component.service_specs(version);
    if !specs.is_empty() {
        return match process::inspect_services(&specs) {
            Ok(observations) => services_status(&observations),
            Err(error) => Status::Error(error),
        };
    }
    ports_status(&component.detect_ports(version))
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

fn services_status(observations: &[ServiceObservation]) -> Status {
    if observations
        .iter()
        .all(|service| service.state == ServiceState::Running)
    {
        return Status::Running;
    }

    let conflicts: Vec<&str> = observations
        .iter()
        .filter(|service| service.state == ServiceState::Conflict)
        .map(|service| service.key)
        .collect();
    if !conflicts.is_empty() {
        return Status::Error(format!("服务身份冲突: {}", conflicts.join(", ")));
    }

    if observations.iter().any(|service| {
        matches!(
            service.state,
            ServiceState::Running | ServiceState::Starting
        )
    }) {
        Status::Partial
    } else {
        Status::Stopped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{app_log, paths};

    fn observation(key: &'static str, state: ServiceState) -> ServiceObservation {
        ServiceObservation {
            key,
            state,
            process: None,
        }
    }

    fn setup_fake_hadoop(tmp: &std::path::Path) {
        std::env::set_var("HOME", tmp);
        let instance = paths::instance_dir("hadoop", "3.5.0").unwrap();
        let config = component::config_dir("hadoop", "3.5.0").unwrap();
        let bin = instance.join("bin");
        let jdk = tmp.join("fake-jdk");
        let name_dir = paths::var_data_instance_dir("hadoop", "3.5.0")
            .unwrap()
            .join("name");

        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&jdk).unwrap();
        std::fs::create_dir_all(name_dir.join("current")).unwrap();
        std::fs::write(name_dir.join("current/VERSION"), "namespaceID=1\n").unwrap();

        for file in [
            "core-site.xml",
            "hdfs-site.xml",
            "yarn-site.xml",
            "mapred-site.xml",
        ] {
            std::fs::write(
                config.join(file),
                "<?xml version=\"1.0\"?>\n<configuration>\n</configuration>\n",
            )
            .unwrap();
        }
        std::fs::write(
            config.join("hadoop-env.sh"),
            format!("export JAVA_HOME={}\n", jdk.display()),
        )
        .unwrap();
        std::fs::write(config.join("workers"), "localhost\n").unwrap();
        for script in ["hdfs", "yarn"] {
            std::fs::write(bin.join(script), "exit 0\n").unwrap();
        }
    }

    #[test]
    fn component_status_unknown_empty_stopped() {
        // 未知组件无端口 → Stopped（不依赖真实端口占用）
        let s = component_status("no-such-component", "0.0.0");
        assert_eq!(s, Status::Stopped);
    }

    #[test]
    fn service_observations_map_to_component_status() {
        assert_eq!(
            services_status(&[
                observation("namenode", ServiceState::Running),
                observation("datanode", ServiceState::Running),
            ]),
            Status::Running
        );
        assert_eq!(
            services_status(&[
                observation("namenode", ServiceState::Running),
                observation("datanode", ServiceState::Starting),
            ]),
            Status::Partial
        );
        assert_eq!(
            services_status(&[observation("namenode", ServiceState::Stopped)]),
            Status::Stopped
        );
        assert!(matches!(
            services_status(&[observation("namenode", ServiceState::Conflict)]),
            Status::Error(message) if message.contains("namenode")
        ));
    }

    #[test]
    fn start_and_stop_log_once() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-service-log-once");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_fake_hadoop(&tmp);

        start("hadoop", "3.5.0").unwrap();
        stop("hadoop", "3.5.0").unwrap();

        let logs = app_log::read_logs_for(&app_log::today()).unwrap();
        assert_eq!(logs.matches("启动组件 hadoop v3.5.0").count(), 1);
        assert_eq!(logs.matches("停止组件 hadoop v3.5.0").count(), 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn start_does_not_run_component_when_init_fails() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-service-init-failure");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_fake_hadoop(&tmp);

        let instance = paths::instance_dir("hadoop", "3.5.0").unwrap();
        let name_dir = paths::var_data_instance_dir("hadoop", "3.5.0")
            .unwrap()
            .join("name");
        let _ = std::fs::remove_file(name_dir.join("current/VERSION"));

        let init_log = tmp.join("init.log");
        let start_log = tmp.join("start.log");
        std::fs::write(
            instance.join("bin/hdfs"),
            format!(
                "printf '%s\\n' \"$*\" >> '{}'\nif [ \"$1\" = namenode ]; then exit 9; fi\n",
                init_log.display()
            ),
        )
        .unwrap();
        std::fs::write(
            instance.join("bin/yarn"),
            format!(
                "printf '%s\\n' \"$*\" >> '{}'\nexit 0\n",
                start_log.display()
            ),
        )
        .unwrap();

        let err = start("hadoop", "3.5.0").unwrap_err();
        assert!(err.contains("NameNode 格式化失败"), "{err}");
        assert!(std::fs::read_to_string(&init_log)
            .unwrap()
            .contains("namenode -format -force"));
        assert!(!start_log.exists(), "init 失败后不应执行任何组件启动命令");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
