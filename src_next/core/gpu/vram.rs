use std::process::Command;

use crate::core::gpu::r#struct::{
    GpuType, VramCheckLowInput, VramCheckLowOutput, VramCheckRecoveredInput,
    VramCheckRecoveredOutput, VramDetectInput, VramDetectOutput, VramGetInfoInput,
    VramGetInfoOutput, VramInfo,
};

/// VRAM 检测后端
#[derive(Debug, Clone)]
enum VramBackend {
    #[cfg(feature = "nvml")]
    NvmlNative,   // NVIDIA 原生库
    NvidiaSmi,    // NVIDIA 命令行工具
    AmdSmi,       // AMD 命令行工具
    Unsupported,
}

/// 检测可用的 VRAM 后端
fn detect_backend() -> VramBackend {
    // 优先尝试原生库（仅在启用 feature 时）
    #[cfg(feature = "nvml")]
    {
        if nvml_wrapper::Nvml::init().is_ok() {
            return VramBackend::NvmlNative;
        }
    }

    // 回退到命令行工具
    if is_command_available("nvidia-smi") {
        return VramBackend::NvidiaSmi;
    }

    if is_command_available("rocm-smi") {
        return VramBackend::AmdSmi;
    }

    VramBackend::Unsupported
}

/// 检测 GPU 类型和依赖可用性
pub fn detect(_input: VramDetectInput) -> VramDetectOutput {
    match detect_backend() {
        #[cfg(feature = "nvml")]
        VramBackend::NvmlNative => VramDetectOutput {
            gpu_type: GpuType::Nvidia,
            available: true,
            reason: Some("使用 NVML 原生库".to_string()),
        },
        VramBackend::NvidiaSmi => VramDetectOutput {
            gpu_type: GpuType::Nvidia,
            available: true,
            reason: Some("使用 nvidia-smi 命令".to_string()),
        },
        VramBackend::AmdSmi => VramDetectOutput {
            gpu_type: GpuType::Amd,
            available: true,
            reason: Some("使用 rocm-smi 命令".to_string()),
        },
        VramBackend::Unsupported => VramDetectOutput {
            gpu_type: GpuType::Unknown,
            available: false,
            reason: Some("未检测到支持的 GPU 工具".to_string()),
        },
    }
}

/// 获取显存使用信息
pub fn get_info(_input: VramGetInfoInput) -> VramGetInfoOutput {
    match detect_backend() {
        #[cfg(feature = "nvml")]
        VramBackend::NvmlNative => match get_nvidia_vram_native() {
            Some(info) => VramGetInfoOutput {
                info: Some(info),
                success: true,
                error: None,
            },
            None => VramGetInfoOutput {
                info: None,
                success: false,
                error: Some("NVML 获取显存信息失败".to_string()),
            },
        },
        VramBackend::NvidiaSmi => match get_nvidia_vram_smi() {
            Some(info) => VramGetInfoOutput {
                info: Some(info),
                success: true,
                error: None,
            },
            None => VramGetInfoOutput {
                info: None,
                success: false,
                error: Some("nvidia-smi 获取显存信息失败".to_string()),
            },
        },
        VramBackend::AmdSmi => match get_amd_vram() {
            Some(info) => VramGetInfoOutput {
                info: Some(info),
                success: true,
                error: None,
            },
            None => VramGetInfoOutput {
                info: None,
                success: false,
                error: Some("rocm-smi 获取显存信息失败".to_string()),
            },
        },
        VramBackend::Unsupported => VramGetInfoOutput {
            info: None,
            success: false,
            error: Some("未检测到支持的 GPU 工具".to_string()),
        },
    }
}

/// 使用 NVML 原生库获取 NVIDIA 显存信息
#[cfg(feature = "nvml")]
fn get_nvidia_vram_native() -> Option<VramInfo> {
    let nvml = nvml_wrapper::Nvml::init().ok()?;
    let device = nvml.device_by_index(0).ok()?;
    let memory = device.memory_info().ok()?;

    let total_mb = (memory.total / (1024 * 1024)) as u32;
    let used_mb = (memory.used / (1024 * 1024)) as u32;
    let free_mb = (memory.free / (1024 * 1024)) as u32;

    Some(VramInfo {
        total_mb,
        used_mb,
        free_mb,
        usage_percent: (used_mb as f32 / total_mb as f32) * 100.0,
        free_percent: (free_mb as f32 / total_mb as f32) * 100.0,
    })
}

/// 使用 nvidia-smi 命令获取 NVIDIA 显存信息
fn get_nvidia_vram_smi() -> Option<VramInfo> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    let parts: Vec<&str> = line.split(", ").collect();

    if parts.len() != 2 {
        return None;
    }

    let used_mb: u32 = parts[0].trim().parse().ok()?;
    let total_mb: u32 = parts[1].trim().parse().ok()?;

    if total_mb == 0 {
        return None;
    }

    let free_mb = total_mb - used_mb;
    let usage_percent = (used_mb as f32 / total_mb as f32) * 100.0;
    let free_percent = (free_mb as f32 / total_mb as f32) * 100.0;

    Some(VramInfo {
        total_mb,
        used_mb,
        free_mb,
        usage_percent,
        free_percent,
    })
}

/// 检查显存是否紧张（低于阈值）
pub fn check_low(input: VramCheckLowInput) -> VramCheckLowOutput {
    let info_result = get_info(VramGetInfoInput {});

    if let Some(info) = info_result.info {
        VramCheckLowOutput {
            is_low: info.free_percent < input.threshold_percent,
            current_percent: Some(info.free_percent),
            threshold_percent: input.threshold_percent,
        }
    } else {
        // 无法获取显存信息时，默认不触发降级
        VramCheckLowOutput {
            is_low: false,
            current_percent: None,
            threshold_percent: input.threshold_percent,
        }
    }
}

/// 检查显存是否已恢复（高于阈值）
pub fn check_recovered(input: VramCheckRecoveredInput) -> VramCheckRecoveredOutput {
    let info_result = get_info(VramGetInfoInput {});

    if let Some(info) = info_result.info {
        VramCheckRecoveredOutput {
            is_recovered: info.free_percent >= input.recovery_percent,
            current_percent: Some(info.free_percent),
            recovery_percent: input.recovery_percent,
        }
    } else {
        // 无法获取显存信息时，默认不触发恢复
        VramCheckRecoveredOutput {
            is_recovered: false,
            current_percent: None,
            recovery_percent: input.recovery_percent,
        }
    }
}

// --- 内部实现 ---

/// 检查命令是否可用
fn is_command_available(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}



/// AMD 显卡：通过 rocm-smi 获取显存信息
fn get_amd_vram() -> Option<VramInfo> {
    let output = Command::new("rocm-smi")
        .args(["--showmeminfo", "vram"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    let mut used_mb: Option<u32> = None;
    let mut total_mb: Option<u32> = None;

    for line in stdout.lines() {
        let line_lower = line.to_lowercase();
        if line_lower.contains("used") {
            if let Some(num) = extract_mb_value(line) {
                used_mb = Some(num);
            }
        } else if line_lower.contains("total") {
            if let Some(num) = extract_mb_value(line) {
                total_mb = Some(num);
            }
        }
    }

    let used = used_mb?;
    let total = total_mb?;

    if total == 0 {
        return None;
    }

    let free = total - used;
    let usage_percent = (used as f32 / total as f32) * 100.0;
    let free_percent = (free as f32 / total as f32) * 100.0;

    Some(VramInfo {
        total_mb: total,
        used_mb: used,
        free_mb: free,
        usage_percent,
        free_percent,
    })
}

/// 从字符串中提取 MB 值
fn extract_mb_value(s: &str) -> Option<u32> {
    let num_str: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    let value: u32 = num_str.parse().ok()?;

    // 如果原字符串包含 "GB"，转换为 MB
    if s.to_lowercase().contains("gb") {
        Some(value * 1024)
    } else {
        Some(value)
    }
}
