//! GPU 模块错误定义

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GpuError {
    /// 没有可用的 GPU 后端
    #[error("No GPU backend available (nvidia-smi or rocm-smi not found)")]
    NoBackend,

    /// 执行命令失败
    #[error("Failed to execute {command}: {message}")]
    CommandFailed {
        /// 执行的命令
        command: String,
        /// 错误信息
        message: String,
    },

    /// 解析输出失败
    #[error("Failed to parse {command} output: {message}")]
    ParseFailed {
        /// 执行的命令
        command: String,
        /// 错误信息
        message: String,
    },
}
