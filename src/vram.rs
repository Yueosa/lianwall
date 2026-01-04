/// 显存监控模块
///
/// 当前仅强支持 NVIDIA 显卡（通过 nvidia-smi）
/// AMD 显卡基本支持（通过 rocm-smi），Intel 显卡暂不支持
///
/// TODO: 未来可考虑添加更多显卡支持
use std::process::Command;

/// 显存使用信息
#[derive(Debug, Clone)]
pub struct VramInfo {
    /// 已使用显存（MB）
    pub used_mb: u64,
    /// 总显存（MB）
    pub total_mb: u64,
    /// 使用率（0.0 - 100.0）
    pub usage_percent: f32,
    /// 剩余率（0.0 - 100.0）
    pub free_percent: f32,
}

/// GPU 类型
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum GpuType {
    Nvidia,
    Amd,
    Intel, // 暂不支持，预留
    Unknown,
}

/// 检测 GPU 类型
pub fn detect_gpu_type() -> GpuType {
    // 优先检测 NVIDIA
    if Command::new("which")
        .arg("nvidia-smi")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return GpuType::Nvidia;
    }

    // 检测 AMD (ROCm)
    if Command::new("which")
        .arg("rocm-smi")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return GpuType::Amd;
    }

    // Intel 暂不支持
    GpuType::Unknown
}

/// 获取显存使用信息
///
/// 返回 None 表示无法获取（不支持的显卡或命令失败）
pub fn get_vram_info() -> Option<VramInfo> {
    match detect_gpu_type() {
        GpuType::Nvidia => get_nvidia_vram(),
        GpuType::Amd => get_amd_vram(),
        _ => None,
    }
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
/// 注意：这是基本支持，可能不如 NVIDIA 精确
fn get_amd_vram() -> Option<VramInfo> {
    // rocm-smi --showmeminfo vram
    // 输出格式可能因版本不同而异，这里做基本解析
    let output = Command::new("rocm-smi")
        .args(["--showmeminfo", "vram"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // 尝试解析输出中的 "Used" 和 "Total" 行
    let mut used_mb: Option<u64> = None;
    let mut total_mb: Option<u64> = None;

    for line in stdout.lines() {
        let line_lower = line.to_lowercase();
        if line_lower.contains("used") {
            // 尝试提取数字（MB）
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
    // 查找数字
    let num_str: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    let value: u64 = num_str.parse().ok()?;

    // 如果原字符串包含 "GB"，转换为 MB
    if s.to_lowercase().contains("gb") {
        Some(value * 1024)
    } else {
        Some(value)
    }
}

/// 检查显存是否紧张（低于阈值）
pub fn is_vram_low(threshold_percent: f32) -> bool {
    if let Some(info) = get_vram_info() {
        info.free_percent < threshold_percent
    } else {
        // 无法获取显存信息时，不触发切换
        false
    }
}

/// 检查显存是否已恢复（高于恢复阈值）
pub fn is_vram_recovered(recovery_percent: f32) -> bool {
    if let Some(info) = get_vram_info() {
        info.free_percent >= recovery_percent
    } else {
        // 无法获取显存信息时，不触发恢复
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_gpu_type() {
        let gpu_type = detect_gpu_type();
        println!("检测到 GPU 类型: {:?}", gpu_type);
        // 不做断言，因为测试环境可能没有 GPU
    }

    #[test]
    fn test_get_vram_info() {
        if let Some(info) = get_vram_info() {
            println!(
                "显存使用: {} / {} MB ({:.1}%)",
                info.used_mb, info.total_mb, info.usage_percent
            );
            assert!(info.usage_percent >= 0.0 && info.usage_percent <= 100.0);
        } else {
            println!("无法获取显存信息（可能没有支持的 GPU）");
        }
    }
}
