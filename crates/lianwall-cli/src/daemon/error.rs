//! 守护进程错误类型

use thiserror::Error;

/// 守护进程错误
#[derive(Debug, Error)]
pub enum DaemonError {
    /// Socket 错误
    #[error("Socket 错误: {0}")]
    Socket(#[from] lianwall_core::socket::SocketError),

    /// 引擎错误
    #[error("引擎错误: {0}")]
    Engine(#[from] lianwall_core::engine::EngineError),

    /// 配置错误
    #[error("配置错误: {0}")]
    Config(#[from] lianwall_core::config::ConfigError),

    /// 壁纸错误
    #[error("壁纸错误: {0}")]
    Wallpaper(#[from] lianwall_core::wallpaper::WallpaperError),

    /// IO 错误
    #[error("{0}: {1}")]
    Io(&'static str, #[source] std::io::Error),
}
