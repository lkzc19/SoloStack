//! SoloStack 旧目录布局迁移。
//!
//! 迁移分为应用级目录和多环境组件目录两部分；组件移动使用 staging、
//! 失败回滚和异常退出恢复，确保旧数据不会被半迁移状态吞掉。
//!
//! > **分层例外**：本模块是 `app` 层唯一了解组件实例磁盘布局（`components/`、
//! > `var/`、运行态文件）的地方 —— 旧布局迁移本质上就得按组件实例路径搬目录。
//! > 这是历史兼容的刻意例外，新代码不应据此在 `app` 层引入组件概念。
//!
//! # 模块
//!
//! | 文件 | 职责 |
//! |---|---|
//! | `app_layout.rs` | 应用级目录迁移（settings / 日志 / 下载缓存 / 废弃目录） |
//! | `environment_layout.rs` | 旧 `components/` + `var/` → 环境目录（编排 + 分组） |
//! | `legacy.rs` | 旧实例扫描 |
//! | `recovery.rs` | 失败回滚与异常退出的 staging 恢复 |
//! | `fs_ops.rs` | 文件移动原语（移动记录 / 恢复） |
//! | `version.rs` | 迁移版本标记（版本门控） |
//!
//! # 版本门控
//!
//! 入口是 `run`：按 `STEPS` 表依次执行所有「当前版本 < 目标版本」的步骤，成功
//! 后写回已应用版本；已到最新则直接返回，不再重跑历史迁移。版本号是**数据布局
//! 版本**（手动 +1），与 app 的 release 版本无关。删除某步迁移的条件是「不再支持
//! 从该步对应的旧布局升级」，而不是「我已经迁移过了」。
//!
//! 设计说明见 `docs/Migration-Design.md`。

mod app_layout;
mod environment_layout;
mod fs_ops;
mod legacy;
mod recovery;
mod version;

pub use app_layout::migrate_app_layout;
pub use environment_layout::migrate_environment_layout;

/// 一步迁移：目标版本 + 处理函数。
type MigrationStep = (u32, fn() -> Result<(), String>);

/// 布局迁移步骤表，按序执行所有 `当前版本 < 目标版本` 的步骤。
///
/// 新增布局变更时在末尾追加一行并把版本号 +1；**不要改动已发布步骤的版本号**
/// （老数据上记录的版本是按这些编号写下的）。注释里标一下该步由哪个 app 版本引入。
const STEPS: &[MigrationStep] = &[(1, migrate_to_v1)];

/// v1：旧全局布局 → 多环境布局（多环境改造引入）。
fn migrate_to_v1() -> Result<(), String> {
    migrate_app_layout().map_err(|error| format!("旧应用目录迁移失败: {error}"))?;
    migrate_environment_layout().map(|_| ())
}

/// 执行所有待应用的布局迁移（带版本门控）。
///
/// - 数据版本高于程序支持 → 报错（降级运行），不继续。
/// - 已到最新 → 直接返回。
/// - 否则按 `STEPS` 依次执行待应用步骤，全部成功后才写回最新版本；失败保留旧版本，
///   下次启动重试（各步骤幂等）。
pub fn run() -> Result<(), String> {
    let current = version::current()?;
    let latest = STEPS.last().map(|(target, _)| *target).unwrap_or(0);

    if current > latest {
        return Err(format!(
            "数据布局版本为 {current}，高于当前程序支持的 {latest}；\
             可能是用更新版程序写过数据后又在旧版打开，请升级程序后再试"
        ));
    }
    if current == latest {
        return Ok(());
    }

    for (target, step) in STEPS {
        if current < *target {
            step()?;
        }
    }
    version::write(latest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{environment, paths};
    use std::path::{Path, PathBuf};

    fn setup(name: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("solostack-migration-{name}"));
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);
        tmp
    }

    fn write_fake_hadoop(
        instance: &Path,
        jdk: &Path,
        namenode_web_port: Option<u16>,
        yarn_exit_code: u8,
    ) {
        let config = instance.join("etc/hadoop");
        let bin = instance.join("bin");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(jdk).unwrap();

        for file in [
            "core-site.xml",
            "yarn-site.xml",
            "mapred-site.xml",
            "workers",
        ] {
            let content = if file == "workers" {
                "localhost\n".to_string()
            } else {
                "<?xml version=\"1.0\"?>\n<configuration>\n</configuration>\n".to_string()
            };
            std::fs::write(config.join(file), content).unwrap();
        }
        let namenode_port = namenode_web_port
            .map(|port| {
                format!(
                    "  <property>\n    <name>dfs.namenode.http-address</name>\n    <value>localhost:{port}</value>\n  </property>\n"
                )
            })
            .unwrap_or_default();
        std::fs::write(
            config.join("hdfs-site.xml"),
            format!("<?xml version=\"1.0\"?>\n<configuration>\n{namenode_port}</configuration>\n"),
        )
        .unwrap();
        std::fs::write(
            config.join("hadoop-env.sh"),
            format!("export JAVA_HOME={}\n", jdk.display()),
        )
        .unwrap();
        std::fs::write(bin.join("hdfs"), "exit 0\n").unwrap();
        std::fs::write(bin.join("yarn"), format!("exit {yarn_exit_code}\n")).unwrap();
    }

    fn create_legacy_hadoop(root: &Path, jdk: &Path, version: &str) {
        let instance_name = format!("hadoop-{version}");
        let instance = root
            .join(paths::COMPONENTS_DIR)
            .join("hadoop")
            .join(&instance_name);
        write_fake_hadoop(&instance, jdk, None, 0);
        std::fs::create_dir_all(
            root.join("var/data/hadoop")
                .join(&instance_name)
                .join("name/current"),
        )
        .unwrap();
        std::fs::write(
            root.join("var/data/hadoop")
                .join(&instance_name)
                .join("name/current/VERSION"),
            format!("namespaceID={version}\n"),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("var/log/hadoop").join(&instance_name)).unwrap();
        std::fs::write(
            root.join("var/log/hadoop")
                .join(&instance_name)
                .join("hadoop.log"),
            "legacy log\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("var/run/hadoop").join(&instance_name)).unwrap();
        std::fs::write(
            root.join("var/run/hadoop").join(&instance_name).join("pid"),
            "legacy pid\n",
        )
        .unwrap();
    }

    #[test]
    fn migrates_app_level_layout_idempotently() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("app-layout");
        let root = tmp.join(paths::ROOT_DIR_NAME);

        std::fs::create_dir_all(root.join("downloads")).unwrap();
        std::fs::create_dir_all(root.join("var/downloads")).unwrap();
        std::fs::create_dir_all(root.join("var/solostack")).unwrap();
        std::fs::create_dir_all(root.join("installs")).unwrap();
        std::fs::create_dir_all(root.join("etc/hadoop/hadoop-3.5.0")).unwrap();
        std::fs::write(root.join("settings.json"), "{}").unwrap();
        std::fs::write(root.join("downloads/a.tgz"), "a").unwrap();
        std::fs::write(root.join("var/downloads/b.tgz"), "b").unwrap();
        std::fs::write(root.join("var/solostack/solostack.log"), "log").unwrap();

        migrate_app_layout().unwrap();

        assert!(paths::settings_file().unwrap().is_file());
        assert!(paths::downloads_dir().unwrap().join("a.tgz").is_file());
        assert!(paths::downloads_dir().unwrap().join("b.tgz").is_file());
        assert!(paths::app_log_dir()
            .unwrap()
            .join("solostack.log")
            .is_file());
        assert!(!root.join("etc").exists());
        assert!(!root.join("installs").exists());

        migrate_app_layout().unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn legacy_components_migrate_into_one_environment_when_versions_are_unique() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("legacy-unique");
        let root = tmp.join(paths::ROOT_DIR_NAME);
        std::fs::create_dir_all(root.join("components/hadoop/hadoop-3.5.0")).unwrap();
        std::fs::create_dir_all(root.join("components/kafka/kafka-4.3.1")).unwrap();
        std::fs::create_dir_all(root.join("var/data/hadoop/hadoop-3.5.0")).unwrap();
        std::fs::write(root.join("var/data/hadoop/hadoop-3.5.0/value"), "data").unwrap();

        let migrated = migrate_environment_layout().unwrap();
        assert_eq!(migrated.len(), 1);
        assert_eq!(migrated[0].components.len(), 2);
        assert!(paths::instance_dir(&migrated[0].id, "hadoop", "3.5.0")
            .unwrap()
            .is_dir());
        assert!(
            paths::var_data_instance_dir(&migrated[0].id, "hadoop", "3.5.0")
                .unwrap()
                .join("value")
                .is_file()
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn duplicate_legacy_component_versions_split_and_remain_manageable() {
        use crate::component::{schema, ConfigFieldUpdate};
        use crate::lifecycle::{service, uninstall};
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("legacy-duplicates");
        let root = tmp.join(paths::ROOT_DIR_NAME);
        let jdk = tmp.join("fake-jdk");
        create_legacy_hadoop(&root, &jdk, "3.5.0");
        create_legacy_hadoop(&root, &jdk, "3.4.1");

        let migrated = migrate_environment_layout().unwrap();
        assert_eq!(migrated.len(), 2);
        assert!(migrated
            .iter()
            .all(|environment| environment.components.len() == 1));

        for (index, environment) in migrated.iter().enumerate() {
            let version = environment.components[0].version.as_str();
            environment::set_active_id(Some(&environment.id)).unwrap();
            schema::save_fields(
                &environment.id,
                "hadoop",
                version,
                &[ConfigFieldUpdate {
                    id: "namenode_web_port".to_string(),
                    value: (19870 + index as u16).to_string(),
                }],
            )
            .unwrap();
            service::start(&environment.id, "hadoop", version).unwrap();
            service::stop(&environment.id, "hadoop", version).unwrap();
            uninstall::uninstall(&environment.id, "hadoop", version, false).unwrap();
            assert!(environment::load(&environment.id)
                .unwrap()
                .components
                .is_empty());
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn stale_migration_is_restored_instead_of_deleted() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("stale");
        let root = tmp.join(paths::ROOT_DIR_NAME);
        let staging = root.join("environments/.migration-test");
        let staged_component = staging.join("components/hadoop/hadoop-3.5.0");
        std::fs::create_dir_all(&staged_component).unwrap();
        std::fs::write(staged_component.join("marker"), "data").unwrap();

        super::recovery::recover_stale_migration(&root, &staging).unwrap();

        assert!(root.join("components/hadoop/hadoop-3.5.0/marker").is_file());
        assert!(!staging.exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn migrated_component_supports_config_start_stop_and_uninstall() {
        use crate::component::{schema, ConfigFieldUpdate};
        use crate::lifecycle::{service, uninstall};
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("lifecycle");
        let root = tmp.join(paths::ROOT_DIR_NAME);
        let jdk = tmp.join("fake-jdk");
        create_legacy_hadoop(&root, &jdk, "3.5.0");

        let migrated = migrate_environment_layout().unwrap();
        assert_eq!(migrated.len(), 1);
        let environment = &migrated[0];
        environment::set_active_id(Some(&environment.id)).unwrap();

        let migrated_instance = paths::instance_dir(&environment.id, "hadoop", "3.5.0").unwrap();
        assert!(migrated_instance.join("etc/hadoop/hadoop-env.sh").is_file());
        assert!(
            paths::var_data_instance_dir(&environment.id, "hadoop", "3.5.0")
                .unwrap()
                .join("name/current/VERSION")
                .is_file()
        );
        assert!(
            paths::var_log_instance_dir(&environment.id, "hadoop", "3.5.0")
                .unwrap()
                .join("hadoop.log")
                .is_file()
        );

        let fields = schema::list_fields(&environment.id, "hadoop", "3.5.0").unwrap();
        assert!(fields.iter().any(|field| field.id == "namenode_web_port"));
        schema::save_fields(
            &environment.id,
            "hadoop",
            "3.5.0",
            &[ConfigFieldUpdate {
                id: "namenode_web_port".to_string(),
                value: "19870".to_string(),
            }],
        )
        .unwrap();

        service::start(&environment.id, "hadoop", "3.5.0").unwrap();
        service::stop(&environment.id, "hadoop", "3.5.0").unwrap();
        uninstall::uninstall(&environment.id, "hadoop", "3.5.0", false).unwrap();

        assert!(environment::load(&environment.id)
            .unwrap()
            .components
            .is_empty());
        assert!(!migrated_instance.exists());
        assert!(
            !paths::var_data_instance_dir(&environment.id, "hadoop", "3.5.0")
                .unwrap()
                .exists()
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn version_gate_migrates_once_then_skips() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("version-gate");
        let root = tmp.join(paths::ROOT_DIR_NAME);

        // 无标记（旧布局）：run 执行迁移并写回当前版本
        std::fs::create_dir_all(root.join("components/hadoop/hadoop-3.5.0")).unwrap();
        run().unwrap();
        assert_eq!(environment::list().unwrap().len(), 1, "旧组件应已迁移成环境");
        let marker = paths::migration_version_file().unwrap();
        assert_eq!(std::fs::read_to_string(&marker).unwrap().trim(), "1");

        // 已应用后：即使又出现旧布局目录，也不再迁移
        let stray = root.join("components/kafka/kafka-4.3.1");
        std::fs::create_dir_all(&stray).unwrap();
        run().unwrap();
        assert!(stray.is_dir(), "已应用布局版本后不应再迁移旧目录");
        assert_eq!(environment::list().unwrap().len(), 1, "不应新增环境");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_data_newer_than_program() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("version-newer");
        paths::ensure_app_dirs().unwrap();
        std::fs::write(paths::migration_version_file().unwrap(), "99\n").unwrap();

        let error = run().unwrap_err();
        assert!(error.contains("高于当前程序支持"), "{error}");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
