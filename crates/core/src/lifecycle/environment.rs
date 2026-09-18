//! 环境级生命周期编排：停止旧环境、验证、切换活动环境。

use crate::app::environment::{self, Environment};
use crate::lifecycle::{lock, service};

/// 切换活动环境。
///
/// 硬约束：旧环境中的全部组件停止并验证通过后，才更新 active_environment_id。
pub fn switch_to(target_id: &str) -> Result<Environment, String> {
    switch_to_with(target_id, stop_all_components)
}

fn switch_to_with<F>(target_id: &str, stop_old_environment: F) -> Result<Environment, String>
where
    F: FnOnce(&Environment) -> Result<(), String>,
{
    let target = environment::load(target_id)?;
    let active_id = environment::active_id()?;
    if active_id.as_deref() == Some(target_id) {
        return Ok(target);
    }

    let _switch_guard = lock::begin_environment_switch()?;
    let old = active_id.as_deref().map(environment::load).transpose()?;

    if let Some(old) = old {
        stop_old_environment(&old)?;
    }

    let mut target = environment::load(target_id)?;
    target.mark_activated()?;
    environment::set_active_id(Some(target_id))?;
    Ok(target)
}

/// 停止一个环境中的全部组件，并确认最终状态均为 Stopped。
pub fn stop_all_components(environment: &Environment) -> Result<(), String> {
    let mut failures = Vec::new();
    for item in &environment.components {
        let before = service::component_status(&environment.id, &item.component, &item.version);
        if before == service::Status::Stopped {
            continue;
        }
        if let Err(error) = service::stop(&environment.id, &item.component, &item.version) {
            failures.push(format!("{} v{}: {error}", item.component, item.version));
            continue;
        }
        let after = service::component_status(&environment.id, &item.component, &item.version);
        if after != service::Status::Stopped {
            failures.push(format!(
                "{} v{}: 停止后状态仍为 {:?}",
                item.component, item.version, after
            ));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("旧环境组件未全部停止: {}", failures.join("；")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_to_same_environment_is_noop() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-switch-same");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let environment = environment::create("默认环境").unwrap();
        environment::set_active_id(Some(&environment.id)).unwrap();
        assert_eq!(switch_to(&environment.id).unwrap().id, environment.id);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn switch_updates_active_environment_when_old_environment_is_stopped() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-switch-target");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let old = environment::create("旧环境").unwrap();
        let target = environment::create("新环境").unwrap();
        environment::set_active_id(Some(&old.id)).unwrap();

        let switched = switch_to(&target.id).unwrap();
        assert_eq!(switched.id, target.id);
        assert_eq!(
            environment::active_id().unwrap().as_deref(),
            Some(target.id.as_str())
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn stop_failure_keeps_old_environment_active_and_releases_switch_lock() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-switch-stop-failure");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let mut old = environment::create("旧环境").unwrap();
        old.register_component("hadoop", "3.5.0").unwrap();
        let target = environment::create("新环境").unwrap();
        environment::set_active_id(Some(&old.id)).unwrap();

        let error = switch_to_with(&target.id, |_| Err("模拟停止失败".to_string())).unwrap_err();
        assert!(error.contains("模拟停止失败"), "{error}");
        assert_eq!(
            environment::active_id().unwrap().as_deref(),
            Some(old.id.as_str())
        );
        assert!(environment::load(&target.id)
            .unwrap()
            .last_activated_at
            .is_none());

        let released = lock::begin_environment_switch().unwrap();
        drop(released);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
