//! 生命周期操作并发保护。
//!
//! - 同一环境同一时间只允许一个生命周期操作。
//! - 环境切换期间拒绝所有普通生命周期操作。
//! - 冲突立即返回，不阻塞 UI 或后台线程。

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

struct State {
    switching: bool,
    busy_environments: HashSet<String>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(State {
            switching: false,
            busy_environments: HashSet::new(),
        })
    })
}

/// 一个环境生命周期操作的 RAII 锁。
pub struct EnvironmentOperationGuard {
    environment_id: String,
}

impl Drop for EnvironmentOperationGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = state().lock() {
            state.busy_environments.remove(&self.environment_id);
        }
    }
}

/// 开始一个普通生命周期操作。
pub fn begin_environment_operation(
    environment_id: &str,
) -> Result<EnvironmentOperationGuard, String> {
    let mut state = state().lock().map_err(|_| "生命周期锁已损坏".to_string())?;
    if state.switching {
        return Err("正在切换环境，请稍后再试".to_string());
    }
    if state.busy_environments.contains(environment_id) {
        return Err("当前环境正在执行其他操作".to_string());
    }
    state.busy_environments.insert(environment_id.to_string());
    Ok(EnvironmentOperationGuard {
        environment_id: environment_id.to_string(),
    })
}

/// 全局环境切换锁。
pub struct EnvironmentSwitchGuard;

impl Drop for EnvironmentSwitchGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = state().lock() {
            state.switching = false;
        }
    }
}

/// 开始环境切换。只要有任何环境存在普通操作，就拒绝切换。
pub fn begin_environment_switch() -> Result<EnvironmentSwitchGuard, String> {
    let mut state = state().lock().map_err(|_| "环境切换锁已损坏".to_string())?;
    if state.switching {
        return Err("正在切换环境".to_string());
    }
    if !state.busy_environments.is_empty() {
        return Err("当前有环境正在执行其他操作，请稍后切换".to_string());
    }
    state.switching = true;
    Ok(EnvironmentSwitchGuard)
}
