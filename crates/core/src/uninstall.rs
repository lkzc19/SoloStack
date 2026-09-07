use crate::paths;

/// 纯净卸载一个组件实例（幂等，可对未安装实例安全调用）。
///
/// 删除范围：组件本体、配置副本、运行日志、进程记录；`keep_data=false` 时连持久数据一起删。
/// 下载缓存与 var/downloads、var/installs 不受影响。
pub fn uninstall(name: &str, version: &str, keep_data: bool) -> Result<(), String> {
    let _ = crate::app_log::append(
        crate::app_log::INFO,
        &format!("开始卸载 {name} v{version}（保留数据: {keep_data}）"),
    );
    // 1. 先探活：组件仍在运行时用停止脚本优雅关闭（不直接 kill）
    stop_running_processes(name, version);

    let instance = paths::instance_dir(name, version).map_err(|e| e.to_string())?;
    let etc = paths::etc_instance_dir(name, version).map_err(|e| e.to_string())?;
    let vlog = paths::var_log_instance_dir(name, version).map_err(|e| e.to_string())?;
    let run_file = paths::runtime_file(name, version).map_err(|e| e.to_string())?;
    let vdata = paths::var_data_instance_dir(name, version).map_err(|e| e.to_string())?;

    remove_dir_if_exists(&instance)?;
    remove_dir_if_exists(&etc)?;
    remove_dir_if_exists(&vlog)?;
    remove_file_if_exists(&run_file)?;
    if !keep_data {
        remove_dir_if_exists(&vdata)?;
    }

    // 2. 清理可能遗留的空父目录（`components/<组件>/`、`etc/<组件>/`、`var/.../<组件>/`）。
    for parent in [
        paths::component_dir(name).map_err(|e| e.to_string())?,
        paths::etc_dir().map_err(|e| e.to_string())?.join(name),
        paths::var_dir().map_err(|e| e.to_string())?.join(paths::VAR_LOG_DIR).join(name),
        paths::var_dir().map_err(|e| e.to_string())?.join(paths::VAR_DATA_DIR).join(name),
        paths::var_dir().map_err(|e| e.to_string())?.join(paths::VAR_RUN_DIR).join(name),
    ] {
        remove_dir_if_empty(&parent);
    }

    let _ = crate::app_log::append(crate::app_log::INFO, &format!("{name} v{version} 已卸载"));
    Ok(())
}

/// 卸载前停止组件：先按探活端口探活，存活才调用停止脚本优雅关闭（不直接 kill）。
fn stop_running_processes(name: &str, version: &str) {
    let ports = crate::config::read_detect_ports(name, version);
    let running = ports.iter().any(|p| crate::process::port_open(*p));
    if !running {
        return;
    }
    let _ = crate::app_log::append(crate::app_log::INFO, &format!("{name} 仍在运行，先停止"));
    if let Err(e) = crate::service::stop(name, version) {
        let _ = crate::app_log::append(crate::app_log::ERROR, &format!("停止 {name} 失败: {e}"));
    }
}

fn remove_dir_if_exists(dir: &std::path::Path) -> Result<(), String> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| format!("删除 {} 失败: {e}", dir.display()))?;
    }
    Ok(())
}

fn remove_file_if_exists(file: &std::path::Path) -> Result<(), String> {
    if file.exists() {
        std::fs::remove_file(file).map_err(|e| format!("删除 {} 失败: {e}", file.display()))?;
    }
    Ok(())
}

fn remove_dir_if_empty(dir: &std::path::Path) {
    if let Ok(mut rd) = std::fs::read_dir(dir) {
        if rd.next().is_none() {
            let _ = std::fs::remove_dir(dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uninstall_removes_all_expected_dirs() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-uninstall-test");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let instance = paths::instance_dir("hadoop", "3.5.0").unwrap();
        let etc = paths::etc_instance_dir("hadoop", "3.5.0").unwrap();
        let vdata = paths::var_data_instance_dir("hadoop", "3.5.0").unwrap();
        let vlog = paths::var_log_instance_dir("hadoop", "3.5.0").unwrap();
        let run = paths::runtime_file("hadoop", "3.5.0").unwrap();
        for dir in [&instance, &etc, &vdata, &vlog] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::create_dir_all(run.parent().unwrap()).unwrap();
        std::fs::write(&run, r#"{"pid": 1}"#).unwrap();

        uninstall("hadoop", "3.5.0", false).unwrap();

        assert!(!instance.exists());
        assert!(!etc.exists());
        assert!(!vdata.exists());
        assert!(!vlog.exists());
        assert!(!run.exists());
        assert!(!paths::component_dir("hadoop").unwrap().exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn uninstall_keep_data_preserves_var_data() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-uninstall-keepdata");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let vdata = paths::var_data_instance_dir("hadoop", "3.5.0").unwrap();
        std::fs::create_dir_all(vdata.join("name")).unwrap();

        uninstall("hadoop", "3.5.0", true).unwrap();

        assert!(vdata.exists());
        assert!(!paths::instance_dir("hadoop", "3.5.0").unwrap().exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
