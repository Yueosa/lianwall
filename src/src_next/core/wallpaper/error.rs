use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WallpaperError {
    #[error("目录不存在: {path}")]
    DirectoryNotFound { path: PathBuf },

    #[error("扫描目录失败: {path}, 原因: {source}")]
    ScanFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无效的时间段格式: {input}, 原因: {reason}")]
    InvalidTimeRange { input: String, reason: String },

    #[error("未找到壁纸: {path}")]
    NoWallpapersFound { path: PathBuf },

    #[error("IO 错误: {operation} at '{path}', 原因: {source}")]
    Io {
        operation: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
