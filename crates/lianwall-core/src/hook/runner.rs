//! Hook 执行器

use std::process::Stdio;
use std::time::Duration;

/// 执行一条 hook 命令
///
/// 在子进程中通过 `sh -c` 执行，注入环境变量，带超时控制。
/// stdout 丢弃，stderr 截断后记录到 warn 日志。
pub async fn run_hook(
    name: &str,
    command: &str,
    env: &[(String, String)],
    timeout_secs: u64,
) {
    let result = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();

    match result {
        Ok(child) => {
            match tokio::time::timeout(
                Duration::from_secs(timeout_secs),
                child.wait_with_output(),
            )
            .await
            {
                Ok(Ok(output)) => {
                    if output.status.success() {
                        tracing::debug!("Hook '{}' completed successfully", name);
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let stderr_truncated = if stderr.len() > 200 {
                            format!("{}...", &stderr[..200])
                        } else {
                            stderr.to_string()
                        };
                        tracing::warn!(
                            "Hook '{}' exited with {}: {}",
                            name,
                            output.status,
                            stderr_truncated.trim()
                        );
                    }
                }
                Ok(Err(e)) => {
                    tracing::error!("Hook '{}' IO error: {}", name, e);
                }
                Err(_) => {
                    tracing::warn!(
                        "Hook '{}' timed out after {}s, killing",
                        name,
                        timeout_secs
                    );
                    // timeout 后 child 已被 drop,  tokio 会自动 kill
                }
            }
        }
        Err(e) => {
            tracing::error!("Hook '{}' failed to start: {}", name, e);
        }
    }
}
