//! Hadoop 的运行期：首启 NameNode 格式化、启停序列、WebUI 入口。

use super::config::{self, Effective};
use super::{Hadoop, NAME};
use crate::app::paths;
use crate::component::exec;
use crate::component::{self, Runtime, WebUi};

impl Runtime for Hadoop {
    fn start(&self, version: &str) -> Result<(), String> {
        let _ = crate::app::app_log::append(
            crate::app::app_log::INFO,
            &format!("启动组件 {NAME} v{version}"),
        );
        format_namenode_if_needed(version)?;
        run(version, "sbin/start-dfs.sh", &[])?;
        run(version, "sbin/start-yarn.sh", &[])?;
        if config::history_enabled(version) {
            run(
                version,
                "bin/mapred",
                &["--daemon", "start", "historyserver"],
            )?;
        }
        Ok(())
    }

    fn stop(&self, version: &str) -> Result<(), String> {
        let _ = crate::app::app_log::append(
            crate::app::app_log::INFO,
            &format!("停止组件 {NAME} v{version}"),
        );
        if config::history_enabled(version) {
            run(
                version,
                "bin/mapred",
                &["--daemon", "stop", "historyserver"],
            )?;
        }
        run(version, "sbin/stop-yarn.sh", &[])?;
        run(version, "sbin/stop-dfs.sh", &[])?;
        Ok(())
    }

    fn web_uis(&self, version: &str) -> Vec<WebUi> {
        let eff = Effective::from_config(version);
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
fn format_namenode_if_needed(version: &str) -> Result<(), String> {
    let name_dir = config::namenode_dir(version)?;
    if name_dir.join("current/VERSION").exists() {
        return Ok(());
    }

    let instance = paths::instance_dir(NAME, version).map_err(|e| e.to_string())?;
    if !instance.join("bin/hdfs").exists() {
        return Err(format!(
            "未找到 hdfs 命令: {}",
            instance.join("bin/hdfs").display()
        ));
    }
    println!("首次使用，格式化 NameNode...");
    let _ = crate::app::app_log::append(
        crate::app::app_log::INFO,
        &format!("首次启动 {NAME} v{version}，格式化 NameNode"),
    );

    let mut child = run(version, "bin/hdfs", &["namenode", "-format", "-force"])?;
    let out = child.wait().map_err(|e| format!("等待格式化失败: {e}"))?;
    if !out.success() {
        return Err("NameNode 格式化失败".to_string());
    }
    Ok(())
}

/// 运行组件脚本（注入 JAVA_HOME + HADOOP_CONF_DIR，随子进程结束失效）。
fn run(version: &str, script: &str, args: &[&str]) -> Result<std::process::Child, String> {
    let conf = component::config_dir(NAME, version)?.display().to_string();
    exec::run_script(
        &Hadoop,
        version,
        script,
        args,
        &[("HADOOP_CONF_DIR", &conf)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 铺一份最小实例：官方配置文件存在（hdfs-site.xml 可选写自定义 name dir）。
    /// 调用方需持有 HOME_LOCK。
    fn setup(tmp: &std::path::Path, name_dir: Option<&std::path::Path>) {
        std::env::set_var("HOME", tmp);
        let dir = component::config_dir(NAME, "3.5.0").unwrap();
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
        let name_dir = config::namenode_dir("3.5.0").unwrap();
        std::fs::create_dir_all(name_dir.join("current")).unwrap();
        std::fs::write(name_dir.join("current/VERSION"), "namespaceID=1\n").unwrap();
        assert!(format_namenode_if_needed("3.5.0").is_ok());

        // 反之：没有该文件时会真的往格式化流程走（此处因无 hdfs 二进制而报错，
        // 报错内容即证明没有静默跳过）
        std::fs::remove_file(name_dir.join("current/VERSION")).unwrap();
        let err = format_namenode_if_needed("3.5.0").unwrap_err();
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

        assert_eq!(config::namenode_dir("3.5.0").unwrap(), custom);
        // 判断跟到自定义目录 → 不会误判为「未格式化」而重格式化
        assert!(format_namenode_if_needed("3.5.0").is_ok());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
