use thiserror::Error;
use std::path::PathBuf;

use crate::core::algorithm::AlgorithmError;
use crate::core::config::ConfigError;
use crate::core::engine::EngineError;
use crate::core::runtime::RuntimeError;
use crate::core::wallpaper::WallpaperError;

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("配置错误: {0}")]
    Config(#[from] ConfigError),

    #[error("壁纸扫描错误: {0}")]
    Wallpaper(#[from] WallpaperError),

    #[error("算法错误: {0}")]
    Algorithm(#[from] AlgorithmError),

    #[error("引擎错误: {0}")]
    Engine(#[from] EngineError),

    #[error("运行时错误: {0}")]
    Runtime(#[from] RuntimeError),

    #[error("缓存操作失败: {path}, 原因: {source}")]
    Cache {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("JSON 序列化失败: {path}, 原因: {source}")]
    JsonSerialize {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("模式未初始化: {mode:?}")]
    ModeNotInitialized {
        mode: crate::core::runtime::RunMode,
    },

    #[error("没有可用的壁纸")]
    NoWallpapersAvailable,
}
