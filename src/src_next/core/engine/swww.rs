use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use crate::core::engine::error::EngineError;
use crate::core::engine::r#struct::{
    EngineDetectInput, EngineDetectOutput, EngineSetInput, EngineSetOutput, EngineStopInput,
    EngineStopOutput, EngineType,
};
use crate::core::engine::utils::{is_command_available, validate_wallpaper};

/// 检测 swww 是否可用
pub fn detect(_input: EngineDetectInput) -> Result<EngineDetectOutput, EngineError> {
    let available = is_command_available("swww");
    Ok(EngineDetectOutput { available })
}

/// 设置静态壁纸（swww）
pub fn set(input: EngineSetInput) -> Result<EngineSetOutput, EngineError> {
    // 1. 验证壁纸文件
    validate_wallpaper(&input.wallpaper_path).map_err(|reason| {
        EngineError::InvalidWallpaper {
            path: input.wallpaper_path.clone(),
            reason,
        }
    })?;

    // 2. 检查 swww-daemon 是否运行
    let daemon_running = is_daemon_running();

    // 3. 如果 daemon 未运行，先启动（使用 --no-cache 避免旧壁纸闪现）
    if !daemon_running {
        start_daemon()?;
    }

    // 4. 构建 swww img 命令
    let mut cmd = Command::new("swww");
    cmd.arg("img").arg(&input.wallpaper_path);

    // 5. 添加用户参数
    for arg in &input.extra_args {
        cmd.arg(arg);
    }

    // 6. 如果是首次启动且没有用户指定 transition-type，强制用 none 避免闪烁
    if !daemon_running && !input.extra_args.iter().any(|a| a.contains("transition-type")) {
        cmd.args(["--transition-type", "none"]);
    }

    // 7. 执行命令
    let status = cmd.status().map_err(|e| EngineError::CommandFailed {
        command: "swww img".to_string(),
        source: e,
    })?;

    if !status.success() {
        return Err(EngineError::SetFailed {
            engine: EngineType::Swww,
            path: input.wallpaper_path,
            reason: format!("swww 命令失败，退出码: {:?}", status.code()),
        });
    }

    Ok(EngineSetOutput { pid: None })
}

/// 停止 swww
pub fn stop(_input: EngineStopInput) -> Result<EngineStopOutput, EngineError> {
    if !is_daemon_running() {
        return Ok(EngineStopOutput {});
    }

    let _ = Command::new("swww")
        .arg("kill")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(EngineStopOutput {})
}

// --- 内部实现 ---

/// 检查 swww-daemon 是否运行
fn is_daemon_running() -> bool {
    Command::new("pgrep")
        .arg("-x")
        .arg("swww-daemon")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 启动 swww-daemon
fn start_daemon() -> Result<(), EngineError> {
    Command::new("swww-daemon")
        .arg("--no-cache")
        .spawn()
        .map_err(|e| EngineError::StartFailed {
            engine: EngineType::Swww,
            source: e,
        })?;

    // 等待 daemon 初始化
    thread::sleep(Duration::from_millis(500));
    Ok(())
}
