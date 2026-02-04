//! # Engine 模块
//!
//! 壁纸引擎生命周期管理，支持 mpvpaper（视频）和 swww（图片）。
//!
//! ## 生命周期策略
//! - **mpvpaper**: 每次切换壁纸时 kill 并重启（避免内存泄漏）
//! - **swww-daemon**: 检测是否已运行，未运行则启动并持有句柄
//!
//! ## 导出接口
//! - 状态: `EngineState`
//! - 操作: `init`, `set_wallpaper`, `switch_mode`, `shutdown`
//! - 检测: `detect`

mod error;
mod mpvpaper;
mod r#struct;
mod swww;

pub use error::EngineError;
pub use r#struct::{DetectOutput, EngineState};

use crate::config::{Config, WallMode};
use std::path::Path;

/// 初始化引擎状态
///
/// daemon 启动时调用，根据配置启动对应引擎
pub fn init(config: &Config) -> Result<EngineState, EngineError> {
    // 检测引擎可用性
    let detect_result = detect();
    if !detect_result.mpvpaper_available && config.paths.mode == WallMode::Video {
        return Err(EngineError::NotFound {
            engine: "mpvpaper".to_string(),
        });
    }
    if !detect_result.swww_available && config.paths.mode == WallMode::Image {
        return Err(EngineError::NotFound {
            engine: "swww".to_string(),
        });
    }

    let mut state = EngineState::new(config.paths.mode);

    // 如果是 Image 模式，确保 swww-daemon 运行
    if config.paths.mode == WallMode::Image {
        swww::ensure_daemon(&mut state)?;
    }

    Ok(state)
}

/// 设置壁纸（统一入口）
///
/// 根据当前模式调用对应引擎
pub fn set_wallpaper(
    state: &mut EngineState,
    path: &Path,
    config: &Config,
) -> Result<(), EngineError> {
    match state.mode {
        WallMode::Video => {
            // kill 旧进程 -> 启动新进程
            mpvpaper::stop(&mut state.mpvpaper)?;
            state.mpvpaper = Some(mpvpaper::start(path, &config.video_engine)?);
        }
        WallMode::Image => {
            // 确保 daemon 运行 -> 设置图片
            swww::ensure_daemon(state)?;
            swww::set_image(path, &config.image_engine)?;
        }
    }

    state.current = Some(path.to_path_buf());
    Ok(())
}

/// 切换模式
///
/// Video -> Image: kill mpvpaper, 启动 swww-daemon
/// Image -> Video: kill swww-daemon（如果是我们启动的）
pub fn switch_mode(
    state: &mut EngineState,
    new_mode: WallMode,
    _config: &Config,
) -> Result<(), EngineError> {
    if state.mode == new_mode {
        return Ok(());
    }

    match (state.mode, new_mode) {
        (WallMode::Video, WallMode::Image) => {
            // 停止 mpvpaper
            mpvpaper::stop(&mut state.mpvpaper)?;
            // 启动 swww-daemon
            swww::ensure_daemon(state)?;
        }
        (WallMode::Image, WallMode::Video) => {
            // 停止 swww-daemon（仅当我们启动的）
            swww::stop_daemon(state)?;
        }
        // 相同模式，已在函数开头处理
        _ => unreachable!(),
    }

    state.mode = new_mode;
    state.current = None;

    // 检测新模式引擎是否可用
    let detect_result = detect();
    match new_mode {
        WallMode::Video if !detect_result.mpvpaper_available => {
            return Err(EngineError::NotFound {
                engine: "mpvpaper".to_string(),
            });
        }
        WallMode::Image if !detect_result.swww_available => {
            return Err(EngineError::NotFound {
                engine: "swww".to_string(),
            });
        }
        _ => {}
    }

    Ok(())
}

/// 停止所有引擎
///
/// daemon 退出时调用
pub fn shutdown(state: &mut EngineState) -> Result<(), EngineError> {
    mpvpaper::stop(&mut state.mpvpaper)?;
    swww::stop_daemon(state)?;
    Ok(())
}

/// 检测引擎可用性
pub fn detect() -> DetectOutput {
    DetectOutput {
        mpvpaper_available: mpvpaper::is_available(),
        swww_available: swww::is_available(),
    }
}
