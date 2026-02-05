//! LianWall Daemon (lianwalld)
//!
//! 壁纸管理守护进程，通过 Unix Socket 接收命令

mod error;
mod handler;
mod scheduler;
mod server;

use clap::Parser;
use std::path::PathBuf;

use lianwall_core::config::{read, ConfigReadInput};

use error::DaemonError;
use handler::DaemonState;

/// LianWall 守护进程
#[derive(Parser, Debug)]
#[command(name = "lianwalld")]
#[command(version)]
#[command(about = "LianWall daemon - wallpaper management service")]
#[command(author = "Sakurine <https://github.com/Yueosa/lianwall>")]
struct Args {
    /// 配置文件路径
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// 强制启动（覆盖已存在的 socket）
    #[arg(short, long)]
    force: bool,
}

fn main() {
    let args = Args::parse();

    // 初始化日志
    if let Err(e) = init_logging() {
        eprintln!("日志初始化失败: {}", e);
        std::process::exit(1);
    }

    // 运行守护进程
    if let Err(e) = run(args) {
        tracing::error!("守护进程错误: {}", e);
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), DaemonError> {
    // 加载配置
    let config = read(ConfigReadInput { path: args.config })
        .map_err(DaemonError::Config)?
        .config;

    tracing::info!("LianWall Daemon v{}", env!("CARGO_PKG_VERSION"));
    tracing::info!("模式: {:?}", config.paths.mode);
    tracing::info!("Socket: {}", config.daemon.socket_path.display());

    // 初始化状态
    let state = DaemonState::init(config.clone())?;

    // 运行服务
    server::run(state, &config.daemon.socket_path)?;

    Ok(())
}

fn init_logging() -> Result<(), Box<dyn std::error::Error>> {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("lianwalld=info,lianwall_core=info"));

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(false).with_thread_ids(false))
        .with(filter)
        .init();

    Ok(())
}

