use thiserror::Error;

use crate::api::ApiError;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("API 错误: {0}")]
    Api(#[from] ApiError),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("用户取消操作")]
    UserCancelled,

    #[error("无效的过滤器: {0}（有效值: all, active, locked）")]
    InvalidFilter(String),

    #[error("无效的路径: {0}")]
    InvalidPath(String),
}
