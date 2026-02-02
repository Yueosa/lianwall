use std::process::Command;

use crate::core::gpu::r#struct::{
    GpuType, VramCheckLowInput, VramCheckLowOutput, VramCheckRecoveredInput,
    VramCheckRecoveredOutput, VramDetectInput, VramDetectOutput, VramGetInfoInput,
    VramGetInfoOutput, VramInfo,
};

/// 检测 GPU 类型和依赖可用性
pub fn detect(_input: VramDetectInput) -> VramDetectOutput {
    // 优先检测 NVIDIA
    if is_command_available("nvidia-smi") {
        return VramDetectOutput {
            gpu_type: GpuType::Nvidia,
            available: true,
            reason: None,
        };
    }

    // 检测 AMD (ROCm)
    if is_command_available("rocm-smi") {
        return VramDetectOutput {
            gpu_type: GpuType::Amd,
            available: true,
            reason: None,
        };
    }

    // Intel 暂不支持
    // TODO: 未来可添加 intel_gpu_top 等工具支持

    VramDetectOutput {
        gpu_type: GpuType::Unknown,
        available: false,
        reason: Some("未检测到支持的 GPU 工具（nvidia-smi 或 rocm-smi）".to_string()),
    }
}

/// 获取显存使用信息
pub fn get_info(_input: VramGetInfoInput) -> VramGetInfoOutput {
    let detect_result = detect(VramDetectInput {});

    if !detect_result.available {
        return VramGetInfoOutput {
            info: None,
            success: false,
            error: detect_result.reason,
        };
    }

    match detect_result.gpu_type {
        GpuType::Nvidia => match get_nvidia_vram() {
            Some(info) => VramGetInfoOutput {
                info: Some(info),
                success: true,
                error: None,
            },
            None => VramGetInfoOutput {
                info: None,
                success: false,
                error: Some("nvidia-smi 执行失败或输出格式异常".to_string()),
            },
        },
        GpuType::Amd => match get_amd_vram() {
            Some(info) => VramGetInfoOutput {
                info: Some(info),
                success: true,
                error: None,
            },
            None => VramGetInfoOutput {
                info: None,
                success: false,
                error: Some("rocm-smi 执行失败或输出格式异常".to_string()),
            },
        },
        _ => VramGetInfoOutput {
            info: None,
            success: false,
            error: Some("不支持的 GPU 类型".to_string()),
        },
    }
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

/// NVIDIA 显卡：通过 nvidia-smi 获取显存信息
fn get_nvidia_vram() -> Option<VramInfo> {
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

    let used_mb: u64 = parts[0].trim().parse().ok()?;
    let total_mb: u64 = parts[1].trim().parse().ok()?;

    if total_mb == 0 {
        return None;
    }

    let usage_percent = (used_mb as f32 / total_mb as f32) * 100.0;
    let free_percent = 100.0 - usage_percent;

    Some(VramInfo {
        used_mb,
        total_mb,
        usage_percent,
        free_percent,
    })
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

    let mut used_mb: Option<u64> = None;
    let mut total_mb: Option<u64> = None;

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

    let usage_percent = (used as f32 / total as f32) * 100.0;
    let free_percent = 100.0 - usage_percent;

    Some(VramInfo {
        used_mb: used,
        total_mb: total,
        usage_percent,
        free_percent,
    })
}

/// 从字符串中提取 MB 值
fn extract_mb_value(s: &str) -> Option<u64> {
    let num_str: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    let value: u64 = num_str.parse().ok()?;

    // 如果原字符串包含 "GB"，转换为 MB
    if s.to_lowercase().contains("gb") {
        Some(value * 1024)
    } else {
        Some(value)
    }
}
