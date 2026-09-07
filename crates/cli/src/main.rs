use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "solostack", about = "SoloStack local big data stack manager")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 安装组件（下载 + 解压 + 配置，用默认源与 JDK）
    Install { component: String },
    /// 初始化组件的配置副本
    Config { component: String },
    /// 启动组件
    Start { component: String },
    /// 停止组件
    Stop { component: String },
    /// 查看状态
    Status,
}

fn main() {
    // 启动即迁移旧目录布局（幂等）
    let _ = solostack_core::paths::migrate_legacy_layout();
    let cli = Cli::parse();
    match cli.command {
        Commands::Install { component } => install(&component),
        Commands::Config { component } => config(&component),
        Commands::Start { component } => start(&component),
        Commands::Stop { component } => stop(&component),
        Commands::Status => status(),
    }
}

/// 启动组件。
fn start(component: &str) {
    match solostack_core::instances::resolve(component) {
        Ok(i) => match solostack_core::service::start(&i.name, &i.version) {
            Ok(()) => println!("{} 启动命令已执行，正在拉起进程...", i.name),
            Err(e) => println!("启动失败: {e}"),
        },
        Err(e) => println!("{e}"),
    }
}

/// 停止组件。
fn stop(component: &str) {
    match solostack_core::instances::resolve(component) {
        Ok(i) => match solostack_core::service::stop(&i.name, &i.version) {
            Ok(()) => println!("{} 停止命令已执行", i.name),
            Err(e) => println!("停止失败: {e}"),
        },
        Err(e) => println!("{e}"),
    }
}

/// 初始化组件的配置副本。
fn config(component: &str) {
    match solostack_core::instances::resolve(component) {
        Ok(i) => match solostack_core::config::prepare_config(&i.name, &i.version) {
            Ok(dest) => println!("配置副本已就位: {}", dest.display()),
            Err(e) => println!("初始化配置副本失败: {e}"),
        },
        Err(e) => println!("{e}"),
    }
}

/// 安装组件：取 config json 首源、首版本，JDK 用支持列表自动挑。
fn install(component: &str) {
    let cfg = match solostack_core::config_defs::component(component) {
        Ok(c) => c,
        Err(e) => {
            println!("{e}");
            return;
        }
    };
    let source_id = cfg.source.first().map(|s| s.name.clone()).unwrap_or_default();
    let version = cfg
        .source
        .first()
        .and_then(|s| s.version.keys().next())
        .cloned()
        .unwrap_or_default();
    if source_id.is_empty() || version.is_empty() {
        println!("组件 {component} 没有可用的下载源");
        return;
    }
    println!("安装 {component} v{version}（源: {source_id}）...");
    let progress: solostack_core::install::InstallProgress = Box::new(|ev| {
        if let solostack_core::install::ProgressEvent::Downloading(bytes, _) = ev {
            if bytes % (5 * 1024 * 1024) < 64 * 1024 {
                println!("  已下载 {:.1} MB", bytes as f64 / 1024.0 / 1024.0);
            }
        }
    });
    let install_cfg = solostack_core::install_config::InstallConfig {
        component: component.to_string(),
        version: version.clone(),
        source_id,
        jdk_version: String::new(),
        namenode_web: 0,
        yarn_rm: 0,
        history_enabled: false,
        history_web_port: 0,
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    match solostack_core::install::install(&install_cfg, Some(progress), &cancel) {
        Ok(instance) => println!("安装完成: {}", instance.display()),
        Err(e) => println!("安装失败: {e}"),
    }
}

/// 状态命令：展示架构、根目录、内置组件、JDK 与已装组件运行状态。
fn status() {
    println!("arch: {}", solostack_core::arch::Arch::current());
    match solostack_core::paths::root_dir() {
        Ok(root) => println!("root: {}", root.display()),
        Err(e) => println!("无法获取托管目录: {e}"),
    }

    // 可安装组件（config json 内置）
    match solostack_core::config_defs::load_all() {
        Ok(configs) => {
            let names: Vec<String> = configs
                .iter()
                .map(|c| {
                    let v = c.source.first().and_then(|s| s.version.keys().next()).cloned().unwrap_or_default();
                    format!("{} v{v}", c.display_name)
                })
                .collect();
            println!("components: {}", if names.is_empty() { "(无)".into() } else { names.join(", ") });
        }
        Err(e) => println!("读取组件配置失败: {e}"),
    }

    let jdks = solostack_core::jdk::scan();
    if jdks.is_empty() {
        println!("jdks: (本机未检测到)");
    } else {
        let list: Vec<String> = jdks.iter().map(|j| format!("{} v{}", j.name, j.version)).collect();
        println!("jdks: {}", list.join(", "));
    }

    let installed = solostack_core::instances::list_installed();
    if installed.is_empty() {
        println!("installed: (无)");
    } else {
        for i in &installed {
            let label = match solostack_core::service::component_status(&i.name, &i.version) {
                solostack_core::service::Status::Running => "运行中",
                solostack_core::service::Status::Stopped => "已停止",
                solostack_core::service::Status::Partial => "部分运行",
                solostack_core::service::Status::Error(e) => {
                    println!("  {} v{}: 异常 ({e})", i.name, i.version);
                    continue;
                }
            };
            println!("  {} v{}: {}", i.name, i.version, label);
        }
    }
}
