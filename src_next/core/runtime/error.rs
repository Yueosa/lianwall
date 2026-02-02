use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("调度器未启动")]
    NotStarted,

    #[error("调度器已在运行")]
    AlreadyRunning,

    #[error("无效的配置: {field} = {value}, 原因: {reason}")]
    InvalidConfig {
        field: String,
        value: String,
        reason: String,
    },

    #[error("回调执行失败: {operation}, 原因: {reason}")]
    CallbackFailed { operation: String, reason: String },
}
