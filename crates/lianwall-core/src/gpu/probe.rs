//! Bounded command diagnostics shared by monitoring and the read-only test API.
use super::{GpuBackend, VramInfo};
use serde::{Deserialize, Serialize};
use std::{
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::{io::AsyncReadExt, process::Command};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VramProbe {
    pub schema_version: u32,
    pub backend: String,
    pub success: bool,
    pub total_mb: Option<u64>,
    pub used_mb: Option<u64>,
    pub free_mb: Option<u64>,
    pub free_percent: Option<f32>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub truncated: bool,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}
struct ProcessGroup(u32);
impl Drop for ProcessGroup {
    fn drop(&mut self) {
        // SAFETY: the child starts in its own process group, never the daemon's.
        unsafe {
            libc::kill(-(self.0 as i32), libc::SIGKILL);
        }
    }
}
async fn read_bounded(
    pipe: impl tokio::io::AsyncRead + Unpin,
    limit: usize,
) -> std::io::Result<Vec<u8>> {
    let mut data = Vec::new();
    pipe.take((limit + 1) as u64).read_to_end(&mut data).await?;
    Ok(data)
}
pub async fn probe_vram(backend: GpuBackend) -> VramProbe {
    probe(backend, Duration::from_secs(3)).await
}
async fn probe(backend: GpuBackend, timeout: Duration) -> VramProbe {
    let started = Instant::now();
    let name = match &backend {
        GpuBackend::NvidiaSmi => "nvidia-smi",
        GpuBackend::RocmSmi => "rocm-smi",
        GpuBackend::Custom { .. } => "custom",
        GpuBackend::None => "none",
    };
    let mut result = VramProbe {
        schema_version: 1,
        backend: name.into(),
        success: false,
        total_mb: None,
        used_mb: None,
        free_mb: None,
        free_percent: None,
        stdout: String::new(),
        stderr: String::new(),
        exit_code: None,
        timed_out: false,
        truncated: false,
        elapsed_ms: 0,
        error: None,
    };
    let mut command = match &backend {
        GpuBackend::NvidiaSmi => {
            let mut cmd = Command::new("nvidia-smi");
            cmd.args([
                "--query-gpu=memory.total,memory.used",
                "--format=csv,noheader,nounits",
            ]);
            cmd
        }
        GpuBackend::RocmSmi => {
            let mut cmd = Command::new("rocm-smi");
            cmd.args(["--showmeminfo", "vram", "--csv"]);
            cmd
        }
        GpuBackend::Custom { command } if !command.trim().is_empty() => {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", command]);
            cmd
        }
        _ => {
            result.error = Some(
                if name == "custom" {
                    "Custom command is empty"
                } else {
                    "No GPU monitoring backend is available"
                }
                .into(),
            );
            return result;
        }
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            result.error = Some(error.to_string());
            return result;
        }
    };
    let group = ProcessGroup(child.id().expect("spawned child has a pid"));
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    // Poll both bounded readers and wait concurrently: a full stderr pipe must
    // not block stdout, and grandchildren retaining pipes share the same deadline.
    let execution = tokio::time::timeout(timeout, async {
        tokio::try_join!(
            child.wait(),
            read_bounded(stdout, 16 * 1024),
            read_bounded(stderr, 4 * 1024)
        )
    })
    .await;
    drop(group);
    match execution {
        Err(_) => {
            result.timed_out = true;
            result.error = Some("GPU query exceeded its time limit".into());
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        Ok(Err(error)) => {
            result.error = Some(error.to_string());
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        Ok(Ok((status, mut stdout, mut stderr))) => {
            result.exit_code = status.code();
            result.truncated = stdout.len() > 16 * 1024 || stderr.len() > 4 * 1024;
            stdout.truncate(16 * 1024);
            stderr.truncate(4 * 1024);
            result.stdout = String::from_utf8_lossy(&stdout).into_owned();
            result.stderr = String::from_utf8_lossy(&stderr).into_owned();
            if result.truncated {
                result.error = Some("GPU query output exceeded its size limit".into());
            } else if !status.success() {
                result.error = Some(format!(
                    "GPU query exited unsuccessfully ({:?})",
                    status.code()
                ));
            } else {
                let parsed = match backend {
                    GpuBackend::NvidiaSmi => {
                        super::async_ops::parse_nvidia_smi_output(&result.stdout)
                    }
                    GpuBackend::RocmSmi => super::async_ops::parse_rocm_smi_output(&result.stdout),
                    _ => super::async_ops::parse_custom_output(&result.stdout),
                };
                match parsed {
                    Ok(info) if info.total_mb > 0 && info.used_mb <= info.total_mb => {
                        result.success = true;
                        result.total_mb = Some(info.total_mb);
                        result.used_mb = Some(info.used_mb);
                        result.free_mb = Some(info.free_mb);
                        result.free_percent = Some(info.free_percent);
                    }
                    Ok(_) => {
                        result.error = Some("Expected total_mb > 0 and used_mb <= total_mb".into())
                    }
                    Err(error) => result.error = Some(error.to_string()),
                }
            }
        }
    }
    result.elapsed_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
    result
}
impl VramProbe {
    pub fn info(&self) -> Option<VramInfo> {
        self.success
            .then(|| VramInfo::new(self.total_mb.unwrap_or(0), self.used_mb.unwrap_or(0)))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn custom(command: &str) -> GpuBackend {
        GpuBackend::Custom {
            command: command.into(),
        }
    }
    #[tokio::test]
    async fn test_probe_success_exit_parse_and_size_errors() {
        let report = probe_vram(custom("printf 'used_mb=100\ntotal_mb=1024\n'")).await;
        assert!(report.success);
        assert_eq!(report.free_mb, Some(924));
        let report = probe_vram(custom("printf fail >&2; exit 7")).await;
        assert!(!report.success);
        assert_eq!(report.exit_code, Some(7));
        assert_eq!(report.stderr, "fail");
        assert!(
            !probe_vram(custom("printf 'used_mb=10\ntotal_mb=0\n'"))
                .await
                .success
        );
        assert!(
            !probe_vram(custom("printf 'used_mb=20\ntotal_mb=10\n'"))
                .await
                .success
        );
        let report = probe_vram(custom("head -c 20000 /dev/zero")).await;
        assert!(report.truncated);
        assert!(report.stdout.len() <= 16384);
        assert!(!report.success);
    }
    #[tokio::test]
    async fn test_timeout_cleans_up_descendants_retaining_pipes() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("leaked");
        let command = format!("(sleep 0.2; touch '{}') & wait", marker.display());
        let report = probe(custom(&command), Duration::from_millis(40)).await;
        assert!(report.timed_out);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!marker.exists());
    }
}
