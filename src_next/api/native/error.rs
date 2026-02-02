use thiserror::Error;

use crate::core::manager::ManagerError;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("[api::{function}] → {source}")]
    Tracked {
        function: String,
        #[source]
        source: Box<ManagerError>,
    },

    #[error("[api] 未初始化：请先调用 init()")]
    NotInitialized,

    #[error("[api] 无效的配置键: {0}")]
    InvalidConfigKey(String),

    #[error("[api] 无效的配置值: {key} = {value}, 原因: {reason}")]
    InvalidConfigValue {
        key: String,
        value: String,
        reason: String,
    },

    #[error("[api] IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("[api] 序列化错误: {0}")]
    Json(#[from] serde_json::Error),
}

impl ApiError {
    /// 追踪错误来源
    pub fn track(err: ManagerError, function: &str) -> Self {
        Self::Tracked {
            function: function.to_string(),
            source: Box::new(err),
        }
    }
}
