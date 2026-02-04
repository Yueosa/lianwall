//! swww 引擎操作
//!
//! swww 是一个高效的 Wayland 壁纸设置工具，支持平滑过渡动画
//! 官方文档: https://github.com/LGFae/swww

use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use crate::config::ImageEngineConfig;

use super::error::EngineError;
use super::r#struct::EngineState;

/// 检测 swww 是否可用
pub fn is_available() -> bool {
    Command::new("which")
        .arg("swww")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 检测 swww-daemon 是否已经在运行且可响应
///
/// 使用 `swww query` 而不是 pgrep，因为 pgrep 可能匹配到僵尸进程
/// 之前我通宵写代码, 测试的时候死活出问题, 然后他妈的发现是个僵尸进程
/// 在我的电脑里潜伏了 11 个小时, 十年磨一剑给我头都打歪了
fn is_daemon_running() -> bool {
    Command::new("swww")
        .arg("query")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 确保 swww-daemon 正在运行
///
/// 如果已经有外部启动的 daemon，标记为 external 并复用
/// 否则启动新的 daemon 并持有句柄
pub fn ensure_daemon(state: &mut EngineState) -> Result<(), EngineError> {
    // 检查我们是否已经持有句柄
    if let Some(ref mut child) = state.swww_daemon {
        if matches!(child.try_wait(), Ok(None)) {
            return Ok(()); // 我们的 daemon 仍在运行
        }
    }

    // 检查是否有外部 daemon 在运行
    if is_daemon_running() {
        state.swww_daemon = None;
        state.swww_daemon_external = true;
        return Ok(());
    }

    // 启动新的 daemon
    let child = Command::new("swww-daemon")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| EngineError::SpawnFailed {
            engine: "swww-daemon".to_string(),
            source: e,
        })?;

    state.swww_daemon = Some(child);
    state.swww_daemon_external = false;

    // 等待 daemon 初始化
    thread::sleep(Duration::from_millis(200));

    Ok(())
}

/// 清除 swww 壁纸
///
/// 使用 `swww clear` 命令将壁纸设置为透明/空
pub fn clear() -> Result<(), EngineError> {
    // 先检查 swww-daemon 是否在运行
    if !is_daemon_running() {
        return Ok(()); // daemon 不在运行，无需清除
    }

    let output = Command::new("swww")
        .arg("clear")
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| EngineError::SpawnFailed {
            engine: "swww".to_string(),
            source: e,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("[lianwall] swww clear 失败: {}", stderr.trim());
        // 不返回错误，继续后续操作
    }

    Ok(())
}

/// 停止 swww-daemon（仅当我们启动的时候）
///
/// 会先清除壁纸再停止 daemon
pub fn stop_daemon(state: &mut EngineState) -> Result<(), EngineError> {
    // 如果是外部启动的，只清除壁纸但不杀进程
    if state.swww_daemon_external {
        let _ = clear(); // 尝试清除壁纸
        state.swww_daemon_external = false;
        return Ok(());
    }

    // 先清除壁纸
    let _ = clear();

    if let Some(mut child) = state.swww_daemon.take() {
        if let Err(e) = child.kill() {
            if e.kind() != std::io::ErrorKind::InvalidInput {
                return Err(EngineError::StopFailed {
                    engine: "swww-daemon".to_string(),
                    source: e,
                });
            }
        }
        let _ = child.wait();
    }

    Ok(())
}

/// 设置壁纸
///
/// 命令格式: `swww img [--outputs <outputs>] [swww_args] <image_path>`
pub fn set_image(path: &Path, config: &ImageEngineConfig) -> Result<(), EngineError> {
    let mut cmd = Command::new("swww");
    cmd.arg("img");

    // 添加目标显示器
    if !config.outputs.is_empty() {
        cmd.arg("--outputs").arg(&config.outputs);
    }

    // 添加 swww 参数
    for arg in &config.swww_args {
        cmd.arg(arg);
    }

    // 图片路径
    cmd.arg(path);

    // 捕获 stderr 以获取详细错误信息
    cmd.stderr(Stdio::piped());

    let output = cmd.output().map_err(|e| EngineError::SpawnFailed {
        engine: "swww".to_string(),
        source: e,
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = if stderr.is_empty() {
            format!("swww img exited with code {:?}", output.status.code())
        } else {
            format!("swww img failed: {}", stderr.trim())
        };
        return Err(EngineError::SetFailed {
            engine: "swww".to_string(),
            message,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_available() {
        // 仅测试函数不会 panic
        let _ = is_available();
    }

    #[test]
    fn test_is_daemon_running() {
        // 仅测试函数不会 panic
        let _ = is_daemon_running();
    }
}
