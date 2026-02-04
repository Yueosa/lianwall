//! rocm-smi 后端
//!
//! 使用 rocm-smi 命令查询 AMD 显卡显存信息

use std::process::Command;

use super::error::GpuError;
use super::r#struct::VramInfo;

/// 查询显存信息
///
/// 执行命令:
/// ```bash
/// rocm-smi --showmeminfo vram --csv
/// ```
///
/// 输出格式（CSV，跳过标题行）:
/// ```csv
/// device,VRAM Total Memory (B),VRAM Total Used Memory (B)
/// card0,8589934592,1073741824
/// ```
pub fn query() -> Result<VramInfo, GpuError> {
    let output = Command::new("rocm-smi")
        .args(["--showmeminfo", "vram", "--csv"])
        .output()
        .map_err(|e| GpuError::CommandFailed {
            command: "rocm-smi".to_string(),
            message: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GpuError::CommandFailed {
            command: "rocm-smi".to_string(),
            message: stderr.to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_output(&stdout)
}

/// 解析 rocm-smi 输出
fn parse_output(output: &str) -> Result<VramInfo, GpuError> {
    // 跳过标题行，取第一个数据行
    let data_line = output
        .lines()
        .skip(1) // 跳过标题
        .next()
        .ok_or_else(|| GpuError::ParseFailed {
            command: "rocm-smi".to_string(),
            message: "No data line found".to_string(),
        })?;

    let parts: Vec<&str> = data_line.split(',').map(|s| s.trim()).collect();

    if parts.len() < 3 {
        return Err(GpuError::ParseFailed {
            command: "rocm-smi".to_string(),
            message: format!("Expected 3 columns, got {}: {}", parts.len(), data_line),
        });
    }

    // rocm-smi 输出的是字节，需要转换为 MB
    let total_bytes: u64 = parts[1].parse().map_err(|_| GpuError::ParseFailed {
        command: "rocm-smi".to_string(),
        message: format!("Invalid total value: {}", parts[1]),
    })?;

    let used_bytes: u64 = parts[2].parse().map_err(|_| GpuError::ParseFailed {
        command: "rocm-smi".to_string(),
        message: format!("Invalid used value: {}", parts[2]),
    })?;

    let total_mb = total_bytes / (1024 * 1024);
    let used_mb = used_bytes / (1024 * 1024);

    Ok(VramInfo::new(total_mb, used_mb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_output_single_gpu() {
        let output = "device,VRAM Total Memory (B),VRAM Total Used Memory (B)\n\
                      card0,8589934592,1073741824\n";
        let info = parse_output(output).unwrap();
        assert_eq!(info.total_mb, 8192); // 8589934592 / 1024 / 1024
        assert_eq!(info.used_mb, 1024); // 1073741824 / 1024 / 1024
    }

    #[test]
    fn test_parse_output_multi_gpu() {
        let output = "device,VRAM Total Memory (B),VRAM Total Used Memory (B)\n\
                      card0,8589934592,1073741824\n\
                      card1,8589934592,2147483648\n";
        let info = parse_output(output).unwrap();
        // 取第一个 GPU
        assert_eq!(info.total_mb, 8192);
        assert_eq!(info.used_mb, 1024);
    }

    #[test]
    fn test_parse_output_empty() {
        let output = "device,VRAM Total Memory (B),VRAM Total Used Memory (B)\n";
        assert!(parse_output(output).is_err());
    }
}
