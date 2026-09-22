//! Hadoop 的运行期：首启 NameNode 格式化、启停序列、WebUI 入口。

use std::time::Duration;

use super::config::{self, Effective};
use super::{Hadoop, NAME};
use crate::app::paths;
use crate::component::exec;
use crate::component::{self, Runtime, WebUi};
use crate::platform::process::ServiceSpec;

const DAEMON_TIMEOUT: Duration = Duration::from_secs(60);
const FORMAT_TIMEOUT: Duration = Duration::from_secs(300);

impl Runtime for Hadoop {
    fn init(&self, environment_id: &str, version: &str) -> Result<(), String> {
        format_namenode_if_needed(environment_id, version)?;
        Ok(())
    }

    fn start(&self, environment_id: &str, version: &str) -> Result<(), String> {
        // 直接用 --daemon 启动各进程，绕过 start-dfs/start-yarn 内部的 SSH 依赖。
        // 顺序：namenode → datanode → resourcemanager → nodemanager → historyserver
        run_daemon(
            environment_id,
            version,
            "bin/hdfs",
            &["--daemon", "start", "namenode"],
        )?;
        run_daemon(
            environment_id,
            version,
            "bin/hdfs",
            &["--daemon", "start", "datanode"],
        )?;
        run_daemon(
            environment_id,
            version,
            "bin/yarn",
            &["--daemon", "start", "resourcemanager"],
        )?;
        run_daemon(
            environment_id,
            version,
            "bin/yarn",
            &["--daemon", "start", "nodemanager"],
        )?;
        if config::history_enabled(environment_id, version) {
            run_daemon(
                environment_id,
                version,
                "bin/mapred",
                &["--daemon", "start", "historyserver"],
            )?;
        }
        Ok(())
    }

    fn stop(&self, environment_id: &str, version: &str) -> Result<(), String> {
        // 逆序停止：historyserver → nodemanager → resourcemanager → datanode → namenode
        if config::history_enabled(environment_id, version) {
            run_daemon(
                environment_id,
                version,
                "bin/mapred",
                &["--daemon", "stop", "historyserver"],
            )?;
        }
        run_daemon(
            environment_id,
            version,
            "bin/yarn",
            &["--daemon", "stop", "nodemanager"],
        )?;
        run_daemon(
            environment_id,
            version,
            "bin/yarn",
            &["--daemon", "stop", "resourcemanager"],
        )?;
        run_daemon(
            environment_id,
            version,
            "bin/hdfs",
            &["--daemon", "stop", "datanode"],
        )?;
        run_daemon(
            environment_id,
            version,
            "bin/hdfs",
            &["--daemon", "stop", "namenode"],
        )?;
        Ok(())
    }

    fn service_specs(&self, environment_id: &str, version: &str) -> Vec<ServiceSpec> {
        let eff = Effective::from_config(environment_id, version);
        let root = paths::instance_dir(environment_id, NAME, version)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| NAME.to_string());
        let service = |key, class: &str, ports: Vec<u16>| {
            ServiceSpec::new(
                environment_id,
                key,
                vec![root.clone(), class.to_string()],
                ports,
            )
        };

        let mut specs = vec![
            service(
                "namenode",
                "org.apache.hadoop.hdfs.server.namenode.NameNode",
                vec![eff.nn_web, config::NAMENODE_RPC],
            ),
            service(
                "datanode",
                "org.apache.hadoop.hdfs.server.datanode.DataNode",
                vec![eff.dn_http],
            ),
            service(
                "resourcemanager",
                "org.apache.hadoop.yarn.server.resourcemanager.ResourceManager",
                vec![eff.rm_web],
            ),
            service(
                "nodemanager",
                "org.apache.hadoop.yarn.server.nodemanager.NodeManager",
                vec![eff.nm_web],
            ),
        ];
        if let Some(history) = eff.history {
            specs.push(service(
                "jobhistory",
                "org.apache.hadoop.mapreduce.v2.hs.JobHistoryServer",
                vec![history],
            ));
        }
        specs
    }

    fn web_uis(&self, environment_id: &str, version: &str) -> Vec<WebUi> {
        let eff = Effective::from_config(environment_id, version);
        let mut list = vec![
            WebUi {
                name: "HDFS".into(),
                url: format!("http://localhost:{}", eff.nn_web),
            },
            WebUi {
                name: "YARN".into(),
                url: format!("http://localhost:{}", eff.rm_web),
            },
        ];
        if let Some(port) = eff.history {
            list.push(WebUi {
                name: "JobHistory".into(),
                url: format!("http://localhost:{port}"),
            });
        }
        list
    }
}

/// 首次使用前格式化 NameNode（幂等，与 Kafka 的首启格式化同一套路）。
///
/// - **幂等信号用官方产物**：格式化后 Hadoop 会在 `dfs.namenode.name.dir/current/VERSION`
///   写下版本信息，它存在即已初始化 —— 不另造 `.formatted` 之类的标记文件。
/// - **目录从配置精确读**（`config::namenode_dir`）：若按默认路径判断，手改过
///   `dfs.namenode.name.dir` 后会判断错位，于是**每次启动都跑一次 `-format -force`**，
///   而 `-force` 会重格式化、抹掉命名空间数据。
fn format_namenode_if_needed(environment_id: &str, version: &str) -> Result<(), String> {
    let name_dir = config::namenode_dir(environment_id, version)?;
    if name_dir.join("current/VERSION").exists() {
        return Ok(());
    }

    let instance = paths::instance_dir(environment_id, NAME, version).map_err(|e| e.to_string())?;
    if !instance.join("bin/hdfs").exists() {
        return Err(format!(
            "未找到 hdfs 命令: {}",
            instance.join("bin/hdfs").display()
        ));
    }
    println!("首次使用，格式化 NameNode...");
    let _ = crate::app::app_log::info(&format!("首次启动 {NAME} v{version}，格式化 NameNode"));

    run_command(
        environment_id,
        version,
        "bin/hdfs",
        &["namenode", "-format", "-force"],
        FORMAT_TIMEOUT,
    )
    .map_err(|e| format!("NameNode 格式化失败: {e}"))?;
    Ok(())
}

/// 执行组件脚本并检查退出码。
fn run_command(
    environment_id: &str,
    version: &str,
    script: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<(), String> {
    let conf = component::config_io::config_dir(environment_id, NAME, version)?
        .display()
        .to_string();
    exec::run_checked(
        environment_id,
        &Hadoop,
        version,
        script,
        args,
        &[("HADOOP_CONF_DIR", &conf)],
        timeout,
    )
    .map(|_| ())
}

/// 以 `--daemon` 模式运行组件脚本：等待命令退出并检查退出码。
///
/// `hdfs/yarn/mapred --daemon start/stop` 会 fork 到后台后立即退出，
/// 不依赖 SSH，适合纯本机伪分布式场景。
fn run_daemon(
    environment_id: &str,
    version: &str,
    script: &str,
    args: &[&str],
) -> Result<(), String> {
    run_command(environment_id, version, script, args, DAEMON_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENV_ID: &str = "Env00001";

    /// 铺一份最小实例：官方配置文件存在（hdfs-site.xml 可选写自定义 name dir）。
    /// 调用方需持有 HOME_LOCK。
    fn setup(tmp: &std::path::Path, name_dir: Option<&std::path::Path>) {
        std::env::set_var("HOME", tmp);
        let dir = component::config_io::config_dir(ENV_ID, NAME, "3.5.0").unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let mut hdfs = String::from("<?xml version=\"1.0\"?>\n<configuration>\n");
        if let Some(p) = name_dir {
            hdfs.push_str(&format!(
                "  <property>\n    <name>dfs.namenode.name.dir</name>\n    <value>{}</value>\n  </property>\n",
                p.display()
            ));
        }
        hdfs.push_str("</configuration>\n");
        std::fs::write(dir.join("hdfs-site.xml"), hdfs).unwrap();
    }

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_util::HOME_LOCK.lock().unwrap()
    }

    #[test]
    fn format_is_skipped_when_official_version_file_exists() {
        let _guard = lock();
        let tmp = std::env::temp_dir().join("solostack-hadoop-format-skip");
        let _ = std::fs::remove_dir_all(&tmp);
        setup(&tmp, None);

        // 官方产物 current/VERSION 存在 → 已格式化，不执行任何脚本
        let name_dir = config::namenode_dir(ENV_ID, "3.5.0").unwrap();
        std::fs::create_dir_all(name_dir.join("current")).unwrap();
        std::fs::write(name_dir.join("current/VERSION"), "namespaceID=1\n").unwrap();
        assert!(format_namenode_if_needed(ENV_ID, "3.5.0").is_ok());

        // 反之：没有该文件时会真的往格式化流程走（此处因无 hdfs 二进制而报错，
        // 报错内容即证明没有静默跳过）
        std::fs::remove_file(name_dir.join("current/VERSION")).unwrap();
        let err = format_namenode_if_needed(ENV_ID, "3.5.0").unwrap_err();
        assert!(err.contains("未找到 hdfs 命令"), "实际报错: {err}");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn format_signal_follows_configured_name_dir() {
        let _guard = lock();
        let tmp = std::env::temp_dir().join("solostack-hadoop-format-custom-dir");
        let _ = std::fs::remove_dir_all(&tmp);
        // 配置把手改后的自定义目录作为 name dir（模拟用户手改配置）
        let custom = tmp.join("my-hdfs-name");
        setup(&tmp, Some(&custom));

        std::fs::create_dir_all(custom.join("current")).unwrap();
        std::fs::write(custom.join("current/VERSION"), "namespaceID=1\n").unwrap();

        assert_eq!(config::namenode_dir(ENV_ID, "3.5.0").unwrap(), custom);
        // 判断跟到自定义目录 → 不会误判为「未格式化」而重格式化
        assert!(format_namenode_if_needed(ENV_ID, "3.5.0").is_ok());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn run_daemon_checks_success_and_failure() {
        let _guard = lock();
        let tmp = std::env::temp_dir().join("solostack-hadoop-daemon-failure");
        let _ = std::fs::remove_dir_all(&tmp);
        setup(&tmp, None);

        let instance = paths::instance_dir(ENV_ID, NAME, "3.5.0").unwrap();
        let config = component::config_io::config_dir(ENV_ID, NAME, "3.5.0").unwrap();
        let bin = instance.join("bin");
        let jdk = tmp.join("fake-jdk");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&jdk).unwrap();
        std::fs::write(
            config.join("hadoop-env.sh"),
            format!("export JAVA_HOME={}\n", jdk.display()),
        )
        .unwrap();
        let script = bin.join("fake-daemon.sh");
        std::fs::write(&script, "printf 'daemon ok\\n'\nexit 0\n").unwrap();
        assert!(run_daemon(
            ENV_ID,
            "3.5.0",
            "bin/fake-daemon.sh",
            &["start", "namenode"]
        )
        .is_ok());

        std::fs::write(
            &script,
            "printf 'daemon stdout\\n'\nprintf 'daemon failed\\n' >&2\nexit 8\n",
        )
        .unwrap();

        let err = run_daemon(
            ENV_ID,
            "3.5.0",
            "bin/fake-daemon.sh",
            &["start", "namenode"],
        )
        .unwrap_err();
        assert!(err.contains("执行失败"), "{err}");
        assert!(err.contains("daemon stdout"), "{err}");
        assert!(err.contains("daemon failed"), "{err}");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn service_specs_cover_hadoop_daemons() {
        let _guard = lock();
        let tmp = std::env::temp_dir().join("solostack-hadoop-service-specs");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let specs = Hadoop.service_specs(ENV_ID, "3.5.0");
        let keys: Vec<&str> = specs.iter().map(|spec| spec.key).collect();
        assert_eq!(
            keys,
            vec!["namenode", "datanode", "resourcemanager", "nodemanager"]
        );
        assert!(specs[0].ports.contains(&8020));
        assert!(specs.iter().all(|spec| spec
            .process_needles
            .iter()
            .any(|needle| needle.contains("hadoop-3.5.0"))));
    }
}
