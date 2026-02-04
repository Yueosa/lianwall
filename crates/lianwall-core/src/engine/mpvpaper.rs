//! mpvpaper 引擎操作
//!
//! mpvpaper 是基于 mpv 的 Wayland 动态壁纸播放器
//! 官方文档: https://github.com/GhostNaN/mpvpaper

use std::path::Path;
use std::process::{Child, Command};

use crate::config::VideoEngineConfig;

use super::error::EngineError;

/// 检测 mpvpaper 是否可用
pub fn is_available() -> bool {
    Command::new("which")
        .arg("mpvpaper")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 启动 mpvpaper
///
/// 命令格式: `mpvpaper [mpvpaper_args] -o "mpv_args" <display> <video_path>`
pub fn start(path: &Path, config: &VideoEngineConfig) -> Result<Child, EngineError> {
    let mut cmd = Command::new("mpvpaper");

    // 添加 mpvpaper 自身参数
    for arg in &config.mpvpaper_args {
        cmd.arg(arg);
    }

    // 添加 mpv 参数（通过 -o 传递）
    if !config.mpv_args.is_empty() {
        let mpv_args_str = config.mpv_args.join(" ");
        cmd.arg("-o").arg(&mpv_args_str);
    }

    // 显示器
    cmd.arg(&config.display);

    // 视频路径
    cmd.arg(path);

    cmd.spawn().map_err(|e| EngineError::SpawnFailed {
        engine: "mpvpaper".to_string(),
        source: e,
    })
}

/// 停止 mpvpaper
pub fn stop(child: &mut Option<Child>) -> Result<(), EngineError> {
    if let Some(mut c) = child.take() {
        // 发送 SIGTERM
        if let Err(e) = c.kill() {
            // 如果进程已经退出，忽略错误
            if e.kind() != std::io::ErrorKind::InvalidInput {
                return Err(EngineError::StopFailed {
                    engine: "mpvpaper".to_string(),
                    source: e,
                });
            }
        }
        // 等待进程退出，回收资源
        let _ = c.wait();
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
}
