//! 异步 GPU 操作
//!
//! 使用 tokio::process 进行非阻塞 GPU 显存查询。
//!
//! ## 为什么需要异步
//! - nvidia-smi 命令执行 50-200ms
//! - 每隔几秒调用一次，同步会周期性阻塞 daemon
//! - 异步化后可以并发处理其他请求

use tokio::process::Command;

use super::error::GpuError;
use super::r#struct::{GpuBackend, VramInfo};

// ==================== 异步检测 ====================

/// 异步检测命令是否可用
async fn is_command_available_async(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 异步检测可用的 GPU 后端
pub async fn detect_backend() -> GpuBackend {
    // 并发检测
    let (nvidia, rocm) = tokio::join!(
        is_command_available_async("nvidia-smi"),
        is_command_available_async("rocm-smi")
    );

    if nvidia {
        GpuBackend::NvidiaSmi
    } else if rocm {
        GpuBackend::RocmSmi
    } else {
        GpuBackend::None
    }
}

// ==================== nvidia-smi ====================

/// 异步查询 nvidia-smi 显存信息
async fn query_nvidia_smi_async() -> Result<VramInfo, GpuError> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=memory.total,memory.used",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .await
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
    parse_nvidia_smi_output(&stdout)
}

/// 解析 nvidia-smi 输出
fn parse_nvidia_smi_output(output: &str) -> Result<VramInfo, GpuError> {
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

// ==================== rocm-smi ====================

/// 异步查询 rocm-smi 显存信息
async fn query_rocm_smi_async() -> Result<VramInfo, GpuError> {
    let output = Command::new("rocm-smi")
        .args(["--showmeminfo", "vram", "--csv"])
        .output()
        .await
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
    parse_rocm_smi_output(&stdout)
}

/// 解析 rocm-smi 输出
fn parse_rocm_smi_output(output: &str) -> Result<VramInfo, GpuError> {
    let data_line = output
        .lines()
        .nth(1) // 跳过标题
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

    // rocm-smi 输出的是字节
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

// ==================== 公开 API ====================

/// 解析自定义命令输出
///
/// 期望 stdout 包含（顺序不限，大小写不敏感）：
/// ```text
/// used_mb=1234
/// total_mb=8192
/// ```
pub(super) fn parse_custom_output(output: &str) -> Result<VramInfo, GpuError> {
    let mut used_mb: Option<u64> = None;
    let mut total_mb: Option<u64> = None;

    for line in output.lines() {
        let line = line.trim().to_lowercase();
        if let Some(val) = line.strip_prefix("used_mb=") {
            used_mb = val.trim().parse().ok();
        } else if let Some(val) = line.strip_prefix("total_mb=") {
            total_mb = val.trim().parse().ok();
        }
    }

    match (used_mb, total_mb) {
        (Some(used), Some(total)) => Ok(VramInfo::new(total, used)),
        _ => Err(GpuError::ParseFailed {
            command: "custom".to_string(),
            message: format!(
                "Expected 'used_mb=N' and 'total_mb=N' in output, got: {}",
                output.trim()
            ),
        }),
    }
}

/// 异步执行自定义命令并解析显存信息
async fn query_custom_async(command: &str) -> Result<VramInfo, GpuError> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .await
        .map_err(|e| GpuError::CommandFailed {
            command: command.to_string(),
            message: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GpuError::CommandFailed {
            command: command.to_string(),
            message: stderr.to_string(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_custom_output(&stdout)
}

/// 异步查询显存信息
pub async fn query_vram(backend: GpuBackend) -> Result<VramInfo, GpuError> {
    match backend {
        GpuBackend::NvidiaSmi => query_nvidia_smi_async().await,
        GpuBackend::RocmSmi => query_rocm_smi_async().await,
        GpuBackend::Custom { command } => query_custom_async(&command).await,
        GpuBackend::None => Err(GpuError::NoBackend),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_detect_backend() {
        // 仅测试不会 panic
        let backend = detect_backend().await;
        println!("Detected backend: {:?}", backend);
    }

    #[test]
    fn test_parse_nvidia_smi_output() {
        let output = "8192, 1234\n";
        let info = parse_nvidia_smi_output(output).unwrap();
        assert_eq!(info.total_mb, 8192);
        assert_eq!(info.used_mb, 1234);
    }

    #[test]
    fn test_parse_rocm_smi_output() {
        let output = "device,VRAM Total Memory (B),VRAM Total Used Memory (B)\n\
                      card0,8589934592,1073741824\n";
        let info = parse_rocm_smi_output(output).unwrap();
        assert_eq!(info.total_mb, 8192);
        assert_eq!(info.used_mb, 1024);
    }
}
