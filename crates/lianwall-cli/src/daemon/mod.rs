//! Daemon 模块
//!
//! 壁纸管理守护进程核心逻辑

mod error;
mod handler;
mod scheduler;
mod server;

pub use error::DaemonError;
pub use handler::DaemonState;

use std::path::PathBuf;
use lianwall_core::config::{read, ConfigReadInput};

/// 运行守护进程
pub fn run(config_path: Option<PathBuf>) -> Result<(), DaemonError> {
    // 初始化日志
    init_logging();

    // 加载配置
    let config = read(ConfigReadInput { path: config_path })
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

fn init_logging() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("lianwall=info,lianwall_core=info"));

    let _ = tracing_subscriber::registry()
        .with(fmt::layer().with_target(false).with_thread_ids(false))
        .with(filter)
        .try_init();
}
