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

    #[error("无效的模式: {0}（应为 video 或 image）")]
    InvalidMode(String),
}
