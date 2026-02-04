//! 引擎模块错误定义

use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    /// 引擎可执行文件未找到（致命错误）
    #[error("Engine not found: {engine}. Please install it first.")]
    NotFound {
        /// 引擎名称：mpvpaper 或 swww
        engine: String,
    },

    /// 引擎启动失败（致命错误）
    #[error("Failed to spawn {engine}: {source}")]
    SpawnFailed {
        /// 引擎名称
        engine: String,
        /// 底层 IO 错误
        #[source]
        source: io::Error,
    },

    /// 引擎停止失败（非致命，可忽略）
    #[error("Failed to stop {engine}: {source}")]
    StopFailed {
        /// 引擎名称
        engine: String,
        /// 底层 IO 错误
        #[source]
        source: io::Error,
    },

    /// 设置壁纸失败
    #[error("Failed to set wallpaper with {engine}: {message}")]
    SetFailed {
        /// 引擎名称
        engine: String,
        /// 错误信息
        message: String,
    },
}
