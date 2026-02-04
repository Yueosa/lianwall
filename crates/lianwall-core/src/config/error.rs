//! 定义配置模块错误

use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    /// 文件 IO 错误
    #[error("IO error ({operation}) at {path}: {source}")]
    Io {
        /// 操作类型：read、write、create_dir、delete
        operation: String,
        /// 发生错误的文件或目录路径
        path: PathBuf,
        /// 底层 IO 错误
        #[source]
        source: std::io::Error,
    },

    /// TOML 解析错误（文件 -> 结构体）
    #[error("Parse error at {path}: {source}")]
    Parse {
        /// 解析失败的文件路径
        path: PathBuf,
        /// TOML 解析错误详情
        #[source]
        source: toml::de::Error,
    },

    /// TOML 序列化错误（结构体 -> 文件）
    #[error("Serialize error at {path}: {source}")]
    Serialize {
        /// 序列化目标文件路径
        path: PathBuf,
        /// TOML 序列化错误详情
        #[source]
        source: toml::ser::Error,
    },
}
