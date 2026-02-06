//! 命令处理器
//!
//! 每个 CLI 命令对应一个 handle_* 函数。
//! 
//! 模块结构:
//! - `lifecycle` - 生命周期命令 (start, stop, restart)
//! - `wallpaper` - 壁纸控制命令 (next, prev, switch, set, mode)
//! - `lock` - 锁定命令 (lock, unlock, toggle_lock)
//! - `directory` - 目录操作命令 (reload, rescan)
//! - `config` - 配置命令 (config show/get/set/reset)
//! - `query` - 状态查询命令 (status, space, time)

mod config;
mod directory;
mod lifecycle;
mod lock;
mod query;
mod wallpaper;

// Re-export all public handlers
pub use config::handle_config;
pub use directory::{handle_reload, handle_rescan};
pub use lifecycle::{handle_restart, handle_start, handle_stop};
pub use lock::{handle_lock, handle_toggle_lock, handle_unlock};
pub use query::{handle_space, handle_status, handle_time};
pub use wallpaper::{handle_mode, handle_next, handle_prev, handle_set, handle_switch};

use std::path::PathBuf;

use crate::client::{self, Client, ClientError};

/// 命令执行结果
pub type Result<T> = std::result::Result<T, HandlerError>;

/// 处理器错误
#[derive(Debug)]
pub enum HandlerError {
    /// Daemon 未运行
    DaemonNotRunning,
    /// 客户端错误
    Client(ClientError),
    /// 配置错误
    Config(lianwall_core::config::ConfigError),
    /// 其他错误
    Other(String),
}

impl std::fmt::Display for HandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DaemonNotRunning => {
                write!(f, "Daemon is not running. Start it with: lianwall start")
            }
            Self::Client(e) => write!(f, "{}", e),
            Self::Config(e) => write!(f, "Config error: {}", e),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

impl From<ClientError> for HandlerError {
    fn from(e: ClientError) -> Self {
        match e {
            ClientError::DaemonNotRunning => Self::DaemonNotRunning,
            _ => Self::Client(e),
        }
    }
}

impl From<lianwall_core::config::ConfigError> for HandlerError {
    fn from(e: lianwall_core::config::ConfigError) -> Self {
        Self::Config(e)
    }
}

/// 获取 socket 路径
pub(crate) fn get_socket_path() -> PathBuf {
    // 从配置文件读取，如果读取失败则使用默认值
    match lianwall_core::config::read(lianwall_core::config::ConfigReadInput { path: None }) {
        Ok(output) => output.config.daemon.socket_path,
        Err(_) => PathBuf::from("/tmp/lianwall.sock"),
    }
}

/// 检查 daemon 是否在运行
pub(crate) fn is_daemon_running() -> bool {
    client::is_running(&get_socket_path())
}

/// 连接到 daemon
pub(crate) fn connect() -> Result<Client> {
    Ok(Client::connect(&get_socket_path())?)
}

/// 规范化路径（相对路径转绝对路径）
pub(crate) fn normalize_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    }
}
