//! SoloStack 数据布局版本门控。
//!
//! `v0.2.0` 是最低支持版本，它确定了当前磁盘布局基线。这里只保留版本标记、
//! 降级保护和未来迁移步骤的入口，不包含任何旧版本迁移实现。
//!
//! 设计说明见 `docs/Migration-Design.md`。

mod version;

/// 一步迁移：目标布局版本 + 处理函数。
type MigrationStep = (u32, fn() -> Result<(), String>);

/// 当前程序支持的最新数据布局版本。
pub const CURRENT_LAYOUT_VERSION: u32 = 1;

/// 已发布的布局迁移步骤，按目标版本升序排列。
///
/// 未来布局变化时在末尾追加步骤，并同步提升 `CURRENT_LAYOUT_VERSION`。
/// 不要修改已经发布步骤的目标版本。
const STEPS: &[MigrationStep] = &[];

/// 执行数据布局版本门控和待应用迁移。
pub fn run() -> Result<(), String> {
    let stored_version = version::read()?;
    let current = stored_version.unwrap_or(version::BASELINE_LAYOUT_VERSION);

    if current > CURRENT_LAYOUT_VERSION {
        return Err(format!(
            "数据布局版本为 {current}，高于当前程序支持的 {CURRENT_LAYOUT_VERSION}；\
             可能是用更新版程序写过数据后又在旧版打开，请升级程序后再试"
        ));
    }

    if current == CURRENT_LAYOUT_VERSION {
        if stored_version.is_none() {
            version::write(CURRENT_LAYOUT_VERSION)?;
        }
        return Ok(());
    }

    for (target, step) in STEPS {
        if current < *target {
            step()?;
        }
    }
    version::write(CURRENT_LAYOUT_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::paths;
    use crate::test_util::HOME_LOCK;
    use std::path::PathBuf;

    fn setup(name: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("solostack-migration-{name}"));
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);
        tmp
    }

    #[test]
    fn run_initializes_missing_version_marker() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("baseline");
        let marker = paths::migration_version_file().unwrap();

        run().unwrap();
        assert_eq!(
            std::fs::read_to_string(&marker).unwrap().trim(),
            CURRENT_LAYOUT_VERSION.to_string()
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn invalid_version_marker_is_reset_to_current_version() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("invalid");
        let marker = paths::migration_version_file().unwrap();
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(&marker, "invalid\n").unwrap();

        run().unwrap();
        assert_eq!(
            std::fs::read_to_string(&marker).unwrap().trim(),
            CURRENT_LAYOUT_VERSION.to_string()
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rejects_data_newer_than_program() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("future");
        let marker = paths::migration_version_file().unwrap();
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(&marker, "99\n").unwrap();

        assert!(run().is_err());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
