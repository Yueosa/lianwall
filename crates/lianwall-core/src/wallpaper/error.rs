//! 定义壁纸模块错误

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WallpaperError {
    /// 文件 IO 错误
    #[error("IO error ({operation}) at {path}: {source}")]
    Io {
        /// 操作类型：read、write、read_dir、create_dir
        operation: String,
        /// 发生错误的文件或目录路径
        path: PathBuf,
        /// 底层 IO 错误
        #[source]
        source: std::io::Error,
    },

    /// JSON 解析错误（文件 -> 结构体）
    #[error("Parse error at {path}: {source}")]
    Parse {
        /// 解析失败的文件路径
        path: PathBuf,
        /// JSON 解析错误详情
        #[source]
        source: serde_json::Error,
    },

    /// JSON 序列化错误（结构体 -> 文件）
    #[error("Serialize error at {path}: {source}")]
    Serialize {
        /// 序列化目标文件路径
        path: PathBuf,
        /// JSON 序列化错误详情
        #[source]
        source: serde_json::Error,
    },

    /// 壁纸目录不存在
    #[error("Directory not found: {0}")]
    DirectoryNotFound(
        /// 不存在的目录路径
        PathBuf,
    ),

    /// 目录中没有找到支持的壁纸文件
    #[error("No wallpapers found in {0}")]
    NoWallpapers(
        /// 扫描的目录路径
        PathBuf,
    ),
}
