//! nvidia-smi 后端
//!
//! 使用 nvidia-smi 命令查询 NVIDIA 显卡显存信息

use std::process::Command;

use super::error::GpuError;
use super::r#struct::VramInfo;

/// 查询显存信息
///
/// 执行命令:
/// ```bash
/// nvidia-smi --query-gpu=memory.total,memory.used --format=csv,noheader,nounits
/// ```
///
/// 输出格式: `8192, 1234`（总量, 已用，单位 MiB）
pub fn query() -> Result<VramInfo, GpuError> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=memory.total,memory.used",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .map_err(|e| GpuError::CommandFailed {
            command: "nvidia-smi".to_string(),
            message: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GpuError::CommandFailed {
            command: "nvidia-smi".to_string(),
            message: stderr.to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_output(&stdout)
}

/// 解析 nvidia-smi 输出
///
/// 输入格式: `8192, 1234` 或多行（多 GPU 取第一个）
fn parse_output(output: &str) -> Result<VramInfo, GpuError> {
    // 取第一行（多 GPU 场景）
    let line = output.lines().next().ok_or_else(|| GpuError::ParseFailed {
        command: "nvidia-smi".to_string(),
        message: "Empty output".to_string(),
    })?;

    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();

    if parts.len() < 2 {
        return Err(GpuError::ParseFailed {
            command: "nvidia-smi".to_string(),
            message: format!("Expected 2 values, got {}: {}", parts.len(), line),
        });
    }

    let total_mb: u64 = parts[0].parse().map_err(|_| GpuError::ParseFailed {
        command: "nvidia-smi".to_string(),
        message: format!("Invalid total value: {}", parts[0]),
    })?;

    let used_mb: u64 = parts[1].parse().map_err(|_| GpuError::ParseFailed {
        command: "nvidia-smi".to_string(),
        message: format!("Invalid used value: {}", parts[1]),
    })?;

    Ok(VramInfo::new(total_mb, used_mb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_output_single_gpu() {
        let output = "8192, 1234\n";
        let info = parse_output(output).unwrap();
        assert_eq!(info.total_mb, 8192);
        assert_eq!(info.used_mb, 1234);
        assert_eq!(info.free_mb, 6958);
    }

    #[test]
    fn test_parse_output_multi_gpu() {
        let output = "8192, 1234\n8192, 5678\n";
        let info = parse_output(output).unwrap();
        // 取第一个 GPU
        assert_eq!(info.total_mb, 8192);
        assert_eq!(info.used_mb, 1234);
    }

    #[test]
    fn test_parse_output_with_spaces() {
        let output = "  8192  ,  1234  \n";
        let info = parse_output(output).unwrap();
        assert_eq!(info.total_mb, 8192);
        assert_eq!(info.used_mb, 1234);
    }

    #[test]
    fn test_parse_output_empty() {
        let output = "";
        assert!(parse_output(output).is_err());
    }

    #[test]
    fn test_parse_output_invalid() {
        let output = "abc, def\n";
        assert!(parse_output(output).is_err());
    }
}
