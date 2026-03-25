//! 异步引擎操作
//!
//! 提供非阻塞的进程管理，实现无缝壁纸切换。
//!
//! ## 生命周期策略
//! **完全接管所有引擎进程的生命周期**
//!
//! - Image 模式启动 → swww-daemon + swww img
//! - Video 模式启动 → 只启动 mpvpaper（延迟启动 swww）
//! - 切换到 Video → swww clear（保留 daemon）+ 启动 mpvpaper
//! - 切换到 Image → 确保 swww-daemon + swww img + 后台杀 mpvpaper
//! - 关闭应用 → 杀死所有 swww-daemon 和 mpvpaper
//!
//! ## 核心优化
//! - **先启动再关闭**：新壁纸立即显示，旧进程后台清理
//! - **tokio::process**：非阻塞进程管理
//! - **无黑屏切换**：模式切换时无视觉中断

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::{Child, Command};
use tokio::time::sleep;

use crate::config::{Config, ImageEngineConfig, VideoEngineConfig, WallMode};

use super::error::EngineError;

/// 引擎状态
///
/// 完全接管 swww-daemon/awww-daemon 和 mpvpaper 的生命周期
pub struct EngineState {
    /// 当前运行模式
    pub mode: WallMode,
    /// 当前壁纸路径
    pub current: Option<PathBuf>,
    /// mpvpaper 进程句柄
    mpvpaper: Option<Child>,
    /// image daemon 进程句柄（由我们完全管理，可能是 awww-daemon 或 swww-daemon）
    swww_daemon: Option<Child>,
    /// 实际使用的图片引擎二进制名称（"awww" 或 "swww"）
    pub image_bin: String,
}

impl EngineState {
    /// 创建新的引擎状态
    pub fn new(mode: WallMode) -> Self {
        Self {
            mode,
            current: None,
            mpvpaper: None,
            swww_daemon: None,
            image_bin: String::new(),
        }
    }

    /// 检查 mpvpaper 是否正在运行
    pub fn is_mpvpaper_running(&mut self) -> bool {
        if let Some(ref mut child) = self.mpvpaper {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }

    /// 检查我们的 swww-daemon 是否正在运行
    pub fn is_swww_daemon_running(&mut self) -> bool {
        if let Some(ref mut child) = self.swww_daemon {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }
}

// ==================== 检测函数 ====================

/// 检测 mpvpaper 是否可用
pub async fn is_mpvpaper_available() -> bool {
    Command::new("which")
        .arg("mpvpaper")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 检测可用的图片引擎二进制名称
///
/// 优先使用 awww，降级到 swww，两者均不可用则返回 None
pub async fn detect_image_bin() -> Option<String> {
    for bin in &["awww", "swww"] {
        let ok = Command::new("which")
            .arg(bin)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return Some(bin.to_string());
        }
    }
    None
}

/// 检测 awww 或 swww 是否可用（任一即可）
pub async fn is_swww_available() -> bool {
    detect_image_bin().await.is_some()
}

/// 检测系统中是否有 image daemon 在运行（不管是谁启动的）
///
/// 优先尝试 awww query，再尝试 swww query
async fn is_any_swww_daemon_running() -> bool {
    for bin in &["awww", "swww"] {
        let ok = Command::new(bin)
            .arg("query")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return true;
        }
    }
    false
}

/// 异步检测引擎可用性
pub async fn detect() -> super::DetectOutput {
    let (mpvpaper, swww) = tokio::join!(is_mpvpaper_available(), is_swww_available());

    super::DetectOutput {
        mpvpaper_available: mpvpaper,
        swww_available: swww,
    }
}

// ==================== mpvpaper 操作 ====================

/// 异步启动 mpvpaper
async fn start_mpvpaper(path: &Path, config: &VideoEngineConfig) -> Result<Child, EngineError> {
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

    // 抑制输出
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());

    cmd.spawn().map_err(|e| EngineError::SpawnFailed {
        engine: "mpvpaper".to_string(),
        source: e,
    })
}

/// 异步停止 mpvpaper（后台执行，不等待）
fn stop_mpvpaper_background(mut child: Child) {
    tokio::spawn(async move {
        if let Err(e) = child.kill().await {
            if e.kind() != std::io::ErrorKind::InvalidInput {
                // 进程可能已经退出，忽略
            }
        }
        let _ = child.wait().await;
    });
}

/// 同步停止 mpvpaper（等待完成）
async fn stop_mpvpaper(child: &mut Option<Child>) {
    if let Some(mut c) = child.take() {
        let _ = c.kill().await;
        let _ = c.wait().await;
    }
}

// ==================== swww/awww 操作 ====================

/// 启动 image daemon（awww-daemon 或 swww-daemon）
///
/// 如果系统中已有同类 daemon 运行，先杀死它再启动我们自己的
async fn start_swww_daemon(state: &mut EngineState) -> Result<(), EngineError> {
    // 检查我们是否已经持有一个运行中的句柄
    if let Some(ref mut child) = state.swww_daemon {
        if matches!(child.try_wait(), Ok(None)) {
            return Ok(()); // 我们的 daemon 仍在运行
        }
    }

    // 如果系统中有其他 daemon 在运行，先杀死它
    if is_any_swww_daemon_running().await {
        // 同时尝试杀死 awww-daemon 和 swww-daemon
        for daemon in &["awww-daemon", "swww-daemon"] {
            let _ = Command::new("pkill")
                .arg("-x")
                .arg(daemon)
                .status()
                .await;
        }
        // 等待进程退出
        sleep(Duration::from_millis(100)).await;
    }

    // 确定要使用的二进制名称（如果 image_bin 已由 init 填充则直接用，否则重新检测）
    let bin = if !state.image_bin.is_empty() {
        state.image_bin.clone()
    } else {
        detect_image_bin().await.ok_or_else(|| EngineError::NotFound {
            engine: "awww or swww".to_string(),
        })?
    };
    let daemon_bin = format!("{}-daemon", bin);

    // 启动新的 daemon
    let child = Command::new(&daemon_bin)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| EngineError::SpawnFailed {
            engine: daemon_bin.clone(),
            source: e,
        })?;

    state.swww_daemon = Some(child);
    state.image_bin = bin;

    // 等待 daemon 初始化
    sleep(Duration::from_millis(200)).await;

    Ok(())
}

/// 确保 swww-daemon 运行（如果没有就启动）
async fn ensure_swww_daemon(state: &mut EngineState) -> Result<(), EngineError> {
    // 检查我们是否已经持有一个运行中的句柄
    if state.is_swww_daemon_running() {
        return Ok(());
    }

    // 启动新的 daemon
    start_swww_daemon(state).await
}

/// 异步设置壁纸（awww 或 swww）
async fn set_swww_image(bin: &str, path: &Path, config: &ImageEngineConfig) -> Result<(), EngineError> {
    let mut cmd = Command::new(bin);
    cmd.arg("img");

    // 添加目标显示器
    if !config.outputs.is_empty() {
        cmd.arg("--outputs").arg(&config.outputs);
    }

    // 添加 swww_args 参数
    for arg in &config.swww_args {
        cmd.arg(arg);
    }

    // 图片路径
    cmd.arg(path);

    // 捕获 stderr
    cmd.stderr(std::process::Stdio::piped());

    let output = cmd.output().await.map_err(|e| EngineError::SpawnFailed {
        engine: bin.to_string(),
        source: e,
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = if stderr.is_empty() {
            format!("{} img exited with code {:?}", bin, output.status.code())
        } else {
            format!("{} img failed: {}", bin, stderr.trim())
        };
        return Err(EngineError::SetFailed {
            engine: bin.to_string(),
            message,
        });
    }

    Ok(())
}

/// 异步清除壁纸（不杀 daemon）
///
/// bin 为 "awww" 或 "swww"，为空时按优先级依次尝试
async fn clear_swww(bin: &str) -> Result<(), EngineError> {
    let bins: &[&str] = if bin.is_empty() {
        &["awww", "swww"]
    } else {
        std::slice::from_ref(&bin)
    };
    for b in bins {
        let ok = Command::new(b)
            .arg("clear")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            break;
        }
    }
    Ok(())
}

/// 停止 image daemon
async fn stop_swww_daemon(child: &mut Option<Child>, image_bin: &str) {
    // 先清除壁纸
    let _ = clear_swww(image_bin).await;

    if let Some(mut c) = child.take() {
        let _ = c.kill().await;
        let _ = c.wait().await;
    }
}

// ==================== 公开 API ====================

/// 异步初始化引擎
///
/// ## 策略
/// - Image 模式：启动 swww-daemon
/// - Video 模式：不启动 swww（延迟到第一次切换）
pub async fn init(config: &Config) -> Result<EngineState, EngineError> {
    let detect_result = detect().await;

    if !detect_result.mpvpaper_available && config.paths.mode == WallMode::Video {
        return Err(EngineError::NotFound {
            engine: "mpvpaper".to_string(),
        });
    }
    if !detect_result.swww_available && config.paths.mode == WallMode::Image {
        return Err(EngineError::NotFound {
            engine: "awww or swww".to_string(),
        });
    }

    let mut state = EngineState::new(config.paths.mode);

    // 检测并记录实际使用的图片引擎（Image 模式和 Video 模式都需要，以备后续切换）
    if let Some(bin) = detect_image_bin().await {
        state.image_bin = bin;
    }

    // 只有 Image 模式才立即启动 daemon
    // Video 模式延迟到第一次切换到 Image 时
    if config.paths.mode == WallMode::Image {
        start_swww_daemon(&mut state).await?;
    }

    Ok(state)
}

/// 异步设置壁纸
pub async fn set_wallpaper(
    state: &mut EngineState,
    path: &Path,
    config: &Config,
) -> Result<(), EngineError> {
    match state.mode {
        WallMode::Video => {
            // 先启动新的 mpvpaper
            let new_child = start_mpvpaper(path, &config.video_engine).await?;
            // 后台关闭旧的
            if let Some(old_child) = state.mpvpaper.take() {
                stop_mpvpaper_background(old_child);
            }
            state.mpvpaper = Some(new_child);
        }
        WallMode::Image => {
            ensure_swww_daemon(state).await?;
            set_swww_image(&state.image_bin.clone(), path, &config.image_engine).await?;
        }
    }

    state.current = Some(path.to_path_buf());
    Ok(())
}

/// 🔥 无缝切换模式
///
/// ## 策略
/// - **Video → Image**：
///   1. 确保 swww-daemon 运行
///   2. swww img 设置新壁纸（立即覆盖）
///   3. 后台杀死 mpvpaper
///
/// - **Image → Video**：
///   1. 启动 mpvpaper（立即覆盖）
///   2. swww clear 清除图片层（保留 daemon）
///
/// ## 优势
/// - 无黑屏：新壁纸直接覆盖旧壁纸
/// - 低延迟：用户无感知切换
/// - 干净的图层：切换到 Video 时清除底层图片
pub async fn switch_mode_seamless(
    state: &mut EngineState,
    new_mode: WallMode,
    first_wallpaper: &Path,
    config: &Config,
) -> Result<(), EngineError> {
    if state.mode == new_mode {
        return Ok(());
    }

    // 保存旧的 mpvpaper 句柄
    let old_mpvpaper = state.mpvpaper.take();

    match new_mode {
        WallMode::Video => {
            // Image → Video
            // 1. 先启动 mpvpaper（立即显示）
            let new_child = start_mpvpaper(first_wallpaper, &config.video_engine).await?;
            state.mpvpaper = Some(new_child);

            // 2. 清除壁纸（不杀 daemon，保留供下次使用）
            clear_swww(&state.image_bin).await?;

            // 3. 后台杀死 mpvpaper（从 Video 切换过来时）
            if let Some(child) = old_mpvpaper {
                stop_mpvpaper_background(child);
            }
        }
        WallMode::Image => {
            // Video → Image
            // 1. 确保 image daemon 运行
            ensure_swww_daemon(state).await?;

            // 2. 设置图片壁纸（立即覆盖 mpvpaper）
            set_swww_image(&state.image_bin.clone(), first_wallpaper, &config.image_engine).await?;

            // 3. 后台杀死 mpvpaper
            if let Some(child) = old_mpvpaper {
                stop_mpvpaper_background(child);
            }
        }
    }

    // 更新状态
    state.mode = new_mode;
    state.current = Some(first_wallpaper.to_path_buf());

    Ok(())
}

/// 异步停止所有引擎
///
/// 完全清理：杀死所有 swww-daemon 和 mpvpaper
pub async fn shutdown(state: &mut EngineState) -> Result<(), EngineError> {
    // 停止 mpvpaper
    stop_mpvpaper(&mut state.mpvpaper).await;

    // 停止 image daemon
    stop_swww_daemon(&mut state.swww_daemon, &state.image_bin.clone()).await;

    // 额外保险：杀死任何可能残留的进程
    let _ = Command::new("pkill")
        .arg("-x")
        .arg("mpvpaper")
        .status()
        .await;

    Ok(())
}

/// 异步清空壁纸（不切换模式）
pub async fn clear_wallpaper(
    state: &mut EngineState,
    _config: &Config,
) -> Result<(), EngineError> {
    match state.mode {
        WallMode::Video => {
            stop_mpvpaper(&mut state.mpvpaper).await;
        }
        WallMode::Image => {
            clear_swww(&state.image_bin).await?;
        }
    }
    state.current = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_detect() {
        // 仅测试不会 panic
        let result = detect().await;
        println!(
            "mpvpaper: {}, swww: {}",
            result.mpvpaper_available, result.swww_available
        );
    }

    #[tokio::test]
    async fn test_engine_state_new() {
        let state = EngineState::new(WallMode::Video);
        assert_eq!(state.mode, WallMode::Video);
        assert!(state.current.is_none());
    }
}
