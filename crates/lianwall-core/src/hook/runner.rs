//! Hook commands own a process group so timeout/cancellation can stop their descendants.
use std::io;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;
use tokio::io::AsyncReadExt;

const STDERR_LIMIT: usize = 4096;

struct ProcessGroup {
    id: i32,
    armed: bool,
}

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        if self.armed {
            // SAFETY: the positive pid came from our child, started with process_group(0).
            // A negative pid addresses that owned group, never the daemon's group.
            unsafe {
                libc::kill(-self.id, libc::SIGKILL);
            }
        }
    }
}

struct HookOutput {
    status: Option<ExitStatus>,
    stderr: Vec<u8>,
    truncated: bool,
}

async fn execute(
    command: &str,
    env: &[(String, String)],
    timeout: Duration,
) -> io::Result<HookOutput> {
    let mut child = tokio::process::Command::new("sh")
        .args(["-c", command])
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()?;
    let mut group = ProcessGroup {
        id: child.id().expect("spawned child has a pid") as i32,
        armed: true,
    };
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let mut preview = Vec::new();
    let mut truncated = false;
    let run = async {
        let mut buffer = [0u8; 4096];
        let mut readable = true;
        loop {
            tokio::select! {
                read = stderr.read(&mut buffer), if readable => {
                    match read {
                        Ok(0) | Err(_) => readable = false,
                        Ok(n) => {
                            let keep = n.min(STDERR_LIMIT - preview.len());
                            preview.extend_from_slice(&buffer[..keep]);
                            truncated |= keep < n;
                        }
                    }
                }
                status = child.wait() => return status,
            }
        }
    };
    let status = match tokio::time::timeout(timeout, run).await {
        Ok(Ok(status)) => {
            group.armed = false;
            // Read remaining diagnostics, without waiting for background jobs to close stderr.
            let drain = async {
                let mut buffer = [0u8; 4096];
                while let Ok(n) = stderr.read(&mut buffer).await {
                    if n == 0 {
                        break;
                    }
                    let keep = n.min(STDERR_LIMIT - preview.len());
                    preview.extend_from_slice(&buffer[..keep]);
                    truncated |= keep < n;
                }
            };
            let _ = tokio::time::timeout(Duration::from_millis(10), drain).await;
            Some(status)
        }
        Ok(Err(error)) => return Err(error),
        Err(_) => {
            // Kill the owned group before reaping the shell, including ordinary descendants.
            drop(group);
            let _ = child.kill().await;
            let _ = child.wait().await;
            None
        }
    };
    Ok(HookOutput {
        status,
        stderr: preview,
        truncated,
    })
}

/// stdout is discarded; at most 4 KiB of stderr is retained, while excess bytes are drained.
/// Timeout/cancellation stops the owned group; descendants that detach into another group
/// or session are outside that ownership boundary. Normal completion preserves background jobs.
pub async fn run_hook(name: &str, command: &str, env: &[(String, String)], timeout_secs: u64) {
    match execute(command, env, Duration::from_secs(timeout_secs)).await {
        Ok(output) => match output.status {
            Some(status) if status.success() => tracing::debug!("Hook '{}' completed", name),
            Some(status) => {
                let text = String::from_utf8_lossy(&output.stderr);
                let mut preview: String = text.chars().take(200).collect();
                if output.truncated || text.chars().count() > 200 {
                    preview.push_str("...");
                }
                tracing::warn!("Hook '{}' exited with {}: {}", name, status, preview.trim());
            }
            None => tracing::warn!(
                "Hook '{}' timed out after {}s; owned process group stopped",
                name,
                timeout_secs
            ),
        },
        Err(error) => tracing::error!("Hook '{}' failed: {}", name, error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unicode_stderr_does_not_panic() {
        let command = format!("printf '{}' >&2; exit 1", "中".repeat(100));
        run_hook("unicode", &command, &[], 2).await;
    }

    #[tokio::test]
    async fn stderr_is_bounded_without_breaking_large_output() {
        let command = "i=0; while [ \"$i\" -lt 5000 ]; do printf 'abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz' >&2; i=$((i+1)); done";
        let output = execute(command, &[], Duration::from_secs(5)).await.unwrap();
        assert!(output.status.unwrap().success());
        assert_eq!(output.stderr.len(), STDERR_LIMIT);
        assert!(output.truncated);
    }

    #[tokio::test]
    async fn timeout_prevents_late_side_effects() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("late");
        let env = vec![("MARKER".into(), marker.to_string_lossy().into_owned())];
        let output = execute(
            "sleep 0.2; printf late > \"$MARKER\"",
            &env,
            Duration::from_millis(30),
        )
        .await
        .unwrap();
        assert!(output.status.is_none());
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn cancellation_stops_background_descendants() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("late");
        let ready = directory.path().join("ready");
        let env = vec![
            ("MARKER".into(), marker.to_string_lossy().into_owned()),
            ("READY".into(), ready.to_string_lossy().into_owned()),
        ];
        let task = tokio::spawn(async move {
            run_hook(
                "cancel",
                "(sleep 0.3; printf late > \"$MARKER\") & printf ready > \"$READY\"; wait",
                &env,
                10,
            )
            .await;
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            while !ready.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        task.abort();
        let _ = task.await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(!marker.exists());
    }
    #[tokio::test]
    async fn ordinary_completion_preserves_redirected_background_jobs() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("background");
        let env = vec![("MARKER".into(), marker.to_string_lossy().into_owned())];
        let output = execute(
            "(sleep 0.1; printf done > \"$MARKER\") >/dev/null 2>&1 &",
            &env,
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        assert!(output.status.unwrap().success());
        tokio::time::timeout(Duration::from_secs(2), async {
            while !marker.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
}
