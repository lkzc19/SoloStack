//! 组件脚本执行：把「Java 组件要注入什么」组装好，再交给 `platform` 拉起进程。
//!
//! 分工：`platform::process` 只会拉进程/取输出，组件相关的环境注入（JAVA_HOME、
//! 以及调用方给的 HADOOP_CONF_DIR 之类）在这一层完成。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::Component;
use crate::app::app_log;
use crate::app::paths;
use crate::package::manifest;
use crate::platform::{jdk, process};

/// 在组件实例目录下执行脚本并检查结果。
///
/// 注入 JAVA_HOME 与调用方给的组件环境变量；非零退出或超时时返回错误。
pub fn run_checked(
    environment_id: &str,
    comp: &dyn Component,
    version: &str,
    script: &str,
    args: &[&str],
    envs: &[(&str, &str)],
    timeout: Duration,
) -> Result<process::ScriptOutput, String> {
    let (dir, envs) = prepare(environment_id, comp, version, envs)?;
    let refs = as_refs(&envs);
    let context = app_log::current_context();
    let args_label = app_log::redact_args(args).join(" ");
    let _ = app_log::debug(&format!(
        "执行脚本 {script} {args_label}（超时 {} ms）",
        timeout.as_millis()
    ));

    let observer_context = context.clone();
    let observer: process::ScriptOutputObserver = Arc::new(move |stream, text| {
        app_log::log_script_output(
            observer_context.as_ref(),
            stream == process::ScriptStream::Stdout,
            text,
        );
    });
    let started = std::time::Instant::now();
    let result =
        process::run_script_in_with_observer(&dir, script, args, &refs, timeout, Some(observer));
    match &result {
        Ok(output) => {
            let exit = output
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "none".to_string());
            let _ = app_log::debug(&format!(
                "脚本 {script} 执行完成（exit={exit}，耗时 {} ms）",
                started.elapsed().as_millis()
            ));
        }
        Err(error) => {
            let _ = app_log::error(&format!(
                "脚本 {script} 执行失败：{error}（耗时 {} ms）",
                started.elapsed().as_millis()
            ));
        }
    }
    result
}

/// 执行脚本并返回标准输出（如 `kafka-storage.sh random-uuid`）。
pub fn run_to_string(
    environment_id: &str,
    comp: &dyn Component,
    version: &str,
    script: &str,
    args: &[&str],
    envs: &[(&str, &str)],
    timeout: Duration,
) -> Result<String, String> {
    Ok(run_checked(environment_id, comp, version, script, args, envs, timeout)?.stdout)
}

/// 解析实例目录 + 组装环境变量（含按需注入的 JAVA_HOME）。
fn prepare(
    environment_id: &str,
    comp: &dyn Component,
    version: &str,
    envs: &[(&str, &str)],
) -> Result<(PathBuf, Vec<(String, String)>), String> {
    let name = comp.component();
    let dir = paths::instance_dir(environment_id, name, version).map_err(|e| e.to_string())?;
    let mut resolved: Vec<(String, String)> = envs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    // 仅 Java 类组件注入 JAVA_HOME；自包含二进制组件不依赖本机 JDK
    if manifest::needs_java(name) {
        resolved.push((
            "JAVA_HOME".to_string(),
            resolve_java_home(environment_id, comp, version)?,
        ));
    }
    Ok((dir, resolved))
}

fn as_refs(envs: &[(String, String)]) -> Vec<(&str, &str)> {
    envs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
}

/// 解析用户显式指定的 JDK（目录名或版本号），找不到时报错。
///
/// 「显式指定」的语义只在这里实现一份：安装面板选中的 JDK、配置页填的 JDK
/// 都走它，避免各处对同一串字符给出不同解释。
pub fn resolve_requested_jdk(requested: &str) -> Result<String, String> {
    let requested = requested.trim();
    if requested.is_empty() {
        return Err("请选择 JDK".to_string());
    }
    jdk::find_by_name(requested)
        .or_else(|| jdk::find_matching(requested))
        .map(|j| j.path.display().to_string())
        .ok_or_else(|| format!("本机未找到 JDK {requested}"))
}

/// JDK 选择策略（**全项目唯一一份**），按可靠性依次回退：
/// 1. `requested`：用户显式指定（安装时选的 JDK）
/// 2. 组件声明的 Java 环境文件里的 JAVA_HOME（精确读；hadoop 是官方 hadoop-env.sh，
///    kafka 是 SoloStack 生成的 solostack-env.sh）——路径已不存在则跳过
/// 3. manifest `java_support[version]` 支持版本里本机已装的
/// 4. 仅当 `allow_any`：任意本机 JDK
///
/// `allow_any` 是安装期与运行期的**唯一**差别：安装时找不到匹配 JDK 就报错
/// （不能让组件装在一个跑不起来的 JDK 上），运行期则必须尽力起来。
fn select_jdk(
    environment_id: &str,
    comp: &dyn Component,
    version: &str,
    requested: Option<&str>,
    allow_any: bool,
) -> Result<String, String> {
    let name = comp.component();
    if let Some(req) = requested {
        return resolve_requested_jdk(req);
    }

    if let Some(home) = super::fields::read_java_home(environment_id, comp, version) {
        // 环境文件里记的 JDK 可能已被卸载，路径不在就继续往下找
        if std::path::Path::new(&home).is_dir() {
            return Ok(home);
        }
    }

    // 只扫一次文件系统：find_matching 每次都会重新扫描，逐个候选调用代价很高
    let jdks = jdk::scan();
    let supported = crate::package::manifest::by_component(name)
        .map(|c| c.java_support.get(version).cloned().unwrap_or_default())
        .unwrap_or_default();
    if let Some(j) = jdks
        .iter()
        .find(|j| supported.iter().any(|v| v.to_string() == j.version))
    {
        return Ok(j.path.display().to_string());
    }
    if allow_any {
        if let Some(j) = jdks.first() {
            return Ok(j.path.display().to_string());
        }
    }
    Err(format!("本机未找到 {name} v{version} 可用的 JDK"))
}

/// 运行期注入用：允许兜底到任意本机 JDK（组件必须能起来）。
pub fn resolve_java_home(
    environment_id: &str,
    comp: &dyn Component,
    version: &str,
) -> Result<String, String> {
    select_jdk(environment_id, comp, version, None, true)
}

/// 安装期用：用户指定 → manifest 支持版本，找不到即报错（不静默用任意 JDK）。
pub fn resolve_jdk_for_install(
    environment_id: &str,
    comp: &dyn Component,
    version: &str,
    requested: &str,
) -> Result<String, String> {
    select_jdk(
        environment_id,
        comp,
        version,
        (!requested.trim().is_empty()).then_some(requested),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::registry;

    #[test]
    fn resolve_java_home_falls_back_to_local_jdk() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-exec-java-home");
        std::env::set_var("HOME", &tmp);

        // 环境文件不存在时走 manifest / 本机 JDK 回退；
        // 本机没有任何 JDK 时允许报错，但不得 panic
        let kafka = registry::by_component("kafka").expect("kafka 应已注册");
        let r = resolve_java_home("Env00001", kafka, "4.3.1");
        if let Ok(home) = r {
            assert!(!home.is_empty());
        }
    }
}
