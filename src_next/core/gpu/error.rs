use thiserror::Error;

use crate::core::gpu::r#struct::GpuType;

#[derive(Debug, Error)]
pub enum VramError {
    #[error("检测 GPU 失败: {reason}")]
    DetectFailed { reason: String },

    #[error("获取显存信息失败 (GPU: {gpu_type:?}): {reason}")]
    GetInfoFailed {
        gpu_type: GpuType,
        reason: String,
    },

    #[error("命令执行失败: {command}, 原因: {source}")]
    CommandFailed {
        command: String,
        #[source]
        source: std::io::Error,
    },

    #[error("解析输出失败: {command}, 原因: {reason}")]
    ParseFailed { command: String, reason: String },

    #[error("显存检测不可用: {reason}")]
    Unavailable { reason: String },
}
