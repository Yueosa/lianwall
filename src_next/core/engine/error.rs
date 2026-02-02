use std::path::PathBuf;
use thiserror::Error;

use crate::core::engine::r#struct::EngineType;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("引擎不可用: {engine:?}, 原因: {reason}")]
    Unavailable {
        engine: EngineType,
        reason: String,
    },

    #[error("壁纸文件无效: {path}, 原因: {reason}")]
    InvalidWallpaper { path: PathBuf, reason: String },

    #[error("设置壁纸失败 ({engine:?}): {path}, 原因: {reason}")]
    SetFailed {
        engine: EngineType,
        path: PathBuf,
        reason: String,
    },

    #[error("启动引擎失败 ({engine:?}): {source}")]
    StartFailed {
        engine: EngineType,
        #[source]
        source: std::io::Error,
    },

    #[error("停止引擎失败 ({engine:?}): {reason}")]
    StopFailed {
        engine: EngineType,
        reason: String,
    },

    #[error("命令执行失败: {command}, 原因: {source}")]
    CommandFailed {
        command: String,
        #[source]
        source: std::io::Error,
    },

    #[error("显示器检测失败: {reason}")]
    MonitorDetectFailed { reason: String },
}
