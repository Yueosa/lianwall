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
//! - 检测: `detect_backend`, `query_vram`
//! - 监控: `init`, `check`
//! - 类型: `GpuBackend`, `VramInfo`, `VramState`, `VramAction`

mod error;
mod monitor;
mod nvidia_smi;
mod rocm_smi;
mod r#struct;

pub use error::GpuError;
pub use monitor::{check, init};
pub use r#struct::{GpuBackend, VramAction, VramInfo, VramState};

use std::process::Command;

/// 检测可用的 GPU 后端
///
/// 优先级: NVML > nvidia-smi > rocm-smi > None
pub fn detect_backend() -> GpuBackend {
    // 检查 nvidia-smi
    if is_command_available("nvidia-smi") {
        return GpuBackend::NvidiaSmi;
    }

    // 检查 rocm-smi
    if is_command_available("rocm-smi") {
        return GpuBackend::RocmSmi;
    }

    GpuBackend::None
}

/// 查询显存信息
pub fn query_vram(backend: GpuBackend) -> Result<VramInfo, GpuError> {
    match backend {
        GpuBackend::NvidiaSmi => nvidia_smi::query(),
        GpuBackend::RocmSmi => rocm_smi::query(),
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
    fn test_detect_backend() {
        // 仅测试不会 panic
        let _ = detect_backend();
    }
}
