//! # GPU 模块
//!
//! 显存监控与降级/恢复决策。
//!
//! ## 支持的后端
//! - **nvidia-smi**: NVIDIA 显卡（默认）
//! - **rocm-smi**: AMD 显卡
//! - **NVML**: NVIDIA 显卡（可选编译，feature = "nvml"）
//!
//! ## 核心功能
//! - 检测可用后端
//! - 查询显存使用情况
//! - 降级/恢复决策（含冷却机制）
//!
//! ## 导出接口
//! - 检测: `detect_backend`, `query_vram` (异步)
//! - 监控: `init`, `check`
//! - 类型: `GpuBackend`, `VramInfo`, `VramState`, `VramAction`

mod async_ops;
mod error;
mod monitor;
mod nvidia_smi;
mod rocm_smi;
mod r#struct;

pub use error::GpuError;
pub use monitor::{check, init, init_with_config};
pub use r#struct::{GpuBackend, VramAction, VramInfo, VramState};

// 异步 API（主要接口）
pub use async_ops::{detect_backend, query_vram};

// 同步检测（轻量级，内部使用）
use std::process::Command;

/// 同步检测可用的 GPU 后端（轻量级）
pub fn detect_backend_sync() -> GpuBackend {
    if is_command_available("nvidia-smi") {
        return GpuBackend::NvidiaSmi;
    }
    if is_command_available("rocm-smi") {
        return GpuBackend::RocmSmi;
    }
    GpuBackend::None
}

/// 同步查询显存（阻塞，仅内部使用）
pub fn query_vram_sync(backend: GpuBackend) -> Result<VramInfo, GpuError> {
    match backend {
        GpuBackend::NvidiaSmi => nvidia_smi::query(),
        GpuBackend::RocmSmi => rocm_smi::query(),
        GpuBackend::Custom { command } => {
            let output = Command::new("sh")
                .arg("-c")
                .arg(&command)
                .output()
                .map_err(|e| GpuError::CommandFailed {
                    command: command.clone(),
                    message: e.to_string(),
                })?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(GpuError::CommandFailed {
                    command: command.clone(),
                    message: stderr.to_string(),
                });
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            async_ops::parse_custom_output(&stdout)
        }
        GpuBackend::None => Err(GpuError::NoBackend),
    }
}

/// 检查命令是否可用
fn is_command_available(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_backend_sync() {
        // 仅测试不会 panic
        let _ = detect_backend_sync();
    }
}
