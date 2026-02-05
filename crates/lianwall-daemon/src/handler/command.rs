//! Command Handler - 修改操作处理
//!
//! 这些请求会修改状态，通过 CommandQueue 串行执行
//!
//! # 设计决策
//!
//! TODO: CLI 异步响应模式
//! 当前方案: 等待命令完成后返回 Ok
//! 未来方案: 立即返回 + 订阅事件确认（适用于需要快速响应的场景）
//! 切换原因: 目前 CLI 场景可以接受短暂等待，保持简单

use std::path::PathBuf;
use std::sync::Arc;

use lianwall_core::socket::{Request, Response, ErrorCode};
use lianwall_core::config::WallMode;

use crate::event::{Event, EventBus, SpaceUpdateReason};
use crate::state::SharedState;

/// 处理命令请求
pub async fn handle_command(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    request: Request,
) -> Response {
    match request {
        Request::Ping => Response::Pong {
            uptime_secs: state.uptime_secs(),
            protocol_version: lianwall_core::socket::PROTOCOL_VERSION,
        },
        
        Request::Next => handle_next(state, event_bus).await,
        Request::Prev => handle_prev(state, event_bus).await,
        
        Request::SetWallpaper { path } => handle_set_wallpaper(state, event_bus, path).await,
        Request::SetMode { mode } => handle_set_mode(state, event_bus, mode).await,
        
        Request::Lock { path } => handle_lock(state, event_bus, path).await,
        Request::Unlock { path } => handle_unlock(state, event_bus, path).await,
        Request::ToggleLock { path } => handle_toggle_lock(state, event_bus, path).await,
        
        Request::SetConfig { key, value } => handle_set_config(state, event_bus, key, value).await,
        Request::ReloadConfig => handle_reload_config(state, event_bus).await,
        
        Request::Rescan => handle_rescan(state, event_bus).await,
        
        Request::Shutdown => handle_shutdown(state, event_bus).await,
        Request::Restart => handle_restart(state, event_bus).await,
        
        // Query 请求不应该到这里
        _ => Response::error(ErrorCode::InvalidRequest, "Not a command request"),
    }
}

/// 切换到下一张壁纸
async fn handle_next(state: &Arc<SharedState>, event_bus: &EventBus) -> Response {
    let mode = *state.engine.mode.read().await;
    
    let path = match mode {
        WallMode::Video => {
            let mut space = state.video_space.write().await;
            if space.items.is_empty() {
                return Response::error(ErrorCode::EmptySpace, "No video wallpapers available");
            }
            let next_idx = space.current_index.map(|i| (i + 1) % space.items.len()).unwrap_or(0);
            space.current_index = Some(next_idx);
            space.items[next_idx].path.clone()
        }
        WallMode::Image => {
            let mut space = state.image_space.write().await;
            if space.items.is_empty() {
                return Response::error(ErrorCode::EmptySpace, "No image wallpapers available");
            }
            let next_idx = space.current_index.map(|i| (i + 1) % space.items.len()).unwrap_or(0);
            space.current_index = Some(next_idx);
            space.items[next_idx].path.clone()
        }
    };
    
    // 应用壁纸
    if let Err(e) = apply_wallpaper(state, &path, mode).await {
        return Response::error(ErrorCode::EngineError, format!("Failed to apply wallpaper: {}", e));
    }
    
    // 更新当前壁纸
    *state.engine.current.write().await = Some(path.clone());
    
    // 发布事件
    event_bus.publish(Event::WallpaperChanged { path, mode });
    
    Response::ok()
}

/// 切换到上一张壁纸
async fn handle_prev(state: &Arc<SharedState>, event_bus: &EventBus) -> Response {
    let mode = *state.engine.mode.read().await;
    
    let path = match mode {
        WallMode::Video => {
            let mut space = state.video_space.write().await;
            if space.items.is_empty() {
                return Response::error(ErrorCode::EmptySpace, "No video wallpapers available");
            }
            let prev_idx = space.current_index
                .map(|i| if i == 0 { space.items.len() - 1 } else { i - 1 })
                .unwrap_or(0);
            space.current_index = Some(prev_idx);
            space.items[prev_idx].path.clone()
        }
        WallMode::Image => {
            let mut space = state.image_space.write().await;
            if space.items.is_empty() {
                return Response::error(ErrorCode::EmptySpace, "No image wallpapers available");
            }
            let prev_idx = space.current_index
                .map(|i| if i == 0 { space.items.len() - 1 } else { i - 1 })
                .unwrap_or(0);
            space.current_index = Some(prev_idx);
            space.items[prev_idx].path.clone()
        }
    };
    
    // 应用壁纸
    if let Err(e) = apply_wallpaper(state, &path, mode).await {
        return Response::error(ErrorCode::EngineError, format!("Failed to apply wallpaper: {}", e));
    }
    
    // 更新当前壁纸
    *state.engine.current.write().await = Some(path.clone());
    
    // 发布事件
    event_bus.publish(Event::WallpaperChanged { path, mode });
    
    Response::ok()
}

/// 设置指定壁纸
async fn handle_set_wallpaper(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    path: PathBuf,
) -> Response {
    // 检测壁纸类型
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    
    let mode = if matches!(ext.as_str(), "mp4" | "webm" | "mkv" | "avi" | "mov" | "gif") {
        WallMode::Video
    } else {
        WallMode::Image
    };
    
    // 应用壁纸
    if let Err(e) = apply_wallpaper(state, &path, mode).await {
        return Response::error(ErrorCode::EngineError, format!("Failed to apply wallpaper: {}", e));
    }
    
    // 更新模式和当前壁纸
    *state.engine.mode.write().await = mode;
    *state.engine.current.write().await = Some(path.clone());
    
    // 发布事件
    event_bus.publish(Event::WallpaperChanged { path, mode });
    
    Response::ok()
}

/// 设置模式
async fn handle_set_mode(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    mode: WallMode,
) -> Response {
    let old_mode = *state.engine.mode.read().await;
    
    if old_mode == mode {
        return Response::ok(); // 模式未变
    }
    
    // 更新模式
    *state.engine.mode.write().await = mode;
    
    // 应用当前模式的壁纸
    let path = match mode {
        WallMode::Video => {
            let space = state.video_space.read().await;
            space.current_index.and_then(|i| space.items.get(i).map(|w| w.path.clone()))
        }
        WallMode::Image => {
            let space = state.image_space.read().await;
            space.current_index.and_then(|i| space.items.get(i).map(|w| w.path.clone()))
        }
    };
    
    if let Some(path) = path {
        if let Err(e) = apply_wallpaper(state, &path, mode).await {
            return Response::error(ErrorCode::EngineError, format!("Failed to apply wallpaper: {}", e));
        }
        *state.engine.current.write().await = Some(path);
    }
    
    // 发布事件
    event_bus.publish(Event::ModeChanged { from: old_mode, to: mode });
    
    Response::ok()
}

/// 锁定壁纸
async fn handle_lock(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    path: PathBuf,
) -> Response {
    let mode = *state.engine.mode.read().await;
    
    match mode {
        WallMode::Video => {
            let mut space = state.video_space.write().await;
            if let Some(item) = space.items.iter_mut().find(|w| w.path == path) {
                item.locked = true;
            }
        }
        WallMode::Image => {
            let mut space = state.image_space.write().await;
            if let Some(item) = space.items.iter_mut().find(|w| w.path == path) {
                item.locked = true;
            }
        }
    }
    
    // 发布事件
    let (video_count, image_count) = {
        (
            state.video_space.read().await.len(),
            state.image_space.read().await.len(),
        )
    };
    event_bus.publish(Event::SpaceUpdated {
        reason: SpaceUpdateReason::LockChange,
        video_count,
        image_count,
    });
    
    Response::ok()
}

/// 解锁壁纸
async fn handle_unlock(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    path: PathBuf,
) -> Response {
    let mode = *state.engine.mode.read().await;
    
    match mode {
        WallMode::Video => {
            let mut space = state.video_space.write().await;
            if let Some(item) = space.items.iter_mut().find(|w| w.path == path) {
                item.locked = false;
            }
        }
        WallMode::Image => {
            let mut space = state.image_space.write().await;
            if let Some(item) = space.items.iter_mut().find(|w| w.path == path) {
                item.locked = false;
            }
        }
    }
    
    // 发布事件
    let (video_count, image_count) = {
        (
            state.video_space.read().await.len(),
            state.image_space.read().await.len(),
        )
    };
    event_bus.publish(Event::SpaceUpdated {
        reason: SpaceUpdateReason::LockChange,
        video_count,
        image_count,
    });
    
    Response::ok()
}

/// 切换锁定状态
async fn handle_toggle_lock(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    path: PathBuf,
) -> Response {
    let mode = *state.engine.mode.read().await;
    
    match mode {
        WallMode::Video => {
            let mut space = state.video_space.write().await;
            if let Some(item) = space.items.iter_mut().find(|w| w.path == path) {
                item.locked = !item.locked;
            }
        }
        WallMode::Image => {
            let mut space = state.image_space.write().await;
            if let Some(item) = space.items.iter_mut().find(|w| w.path == path) {
                item.locked = !item.locked;
            }
        }
    }
    
    // 发布事件
    let (video_count, image_count) = {
        (
            state.video_space.read().await.len(),
            state.image_space.read().await.len(),
        )
    };
    event_bus.publish(Event::SpaceUpdated {
        reason: SpaceUpdateReason::LockChange,
        video_count,
        image_count,
    });
    
    Response::ok()
}

/// 设置配置
async fn handle_set_config(
    state: &Arc<SharedState>,
    _event_bus: &EventBus,
    key: String,
    value: serde_json::Value,
) -> Response {
    let mut config = state.config.write().await;
    
    match key.as_str() {
        "image_engine.interval" => {
            if let Some(v) = value.as_u64() {
                config.image_engine.interval = v;
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid interval value");
            }
        }
        "video_engine.interval" => {
            if let Some(v) = value.as_u64() {
                config.video_engine.interval = v;
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid interval value");
            }
        }
        "paths.video_dir" => {
            if let Some(v) = value.as_str() {
                config.paths.video_dir = PathBuf::from(v);
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid video_dir value");
            }
        }
        "paths.image_dir" => {
            if let Some(v) = value.as_str() {
                config.paths.image_dir = PathBuf::from(v);
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid image_dir value");
            }
        }
        _ => {
            return Response::error(ErrorCode::InvalidRequest, format!("Unknown config key: {}", key));
        }
    }
    
    // 保存配置到文件
    let config_to_save = config.clone();
    drop(config); // 释放写锁，避免持有锁时进行 IO
    
    if let Err(e) = lianwall_core::config::update(lianwall_core::config::ConfigUpdateInput {
        path: None,
        config: config_to_save,
    }) {
        tracing::error!("Failed to save config: {}", e);
        return Response::error(ErrorCode::ConfigError, format!("Failed to save config: {}", e));
    }
    
    tracing::info!("Config key '{}' updated and saved", key);
    Response::ok()
}

/// 重载配置
async fn handle_reload_config(state: &Arc<SharedState>, event_bus: &EventBus) -> Response {
    match lianwall_core::config::read(lianwall_core::config::ConfigReadInput {
        path: None,
    }) {
        Ok(result) => {
            *state.config.write().await = result.config;
            event_bus.publish(Event::ConfigReloaded);
            Response::ok()
        }
        Err(e) => Response::error(ErrorCode::ConfigError, format!("Failed to reload config: {}", e)),
    }
}

/// 重新扫描壁纸目录
///
/// 设计决策：异步执行 + 进度事件通知
async fn handle_rescan(state: &Arc<SharedState>, event_bus: &EventBus) -> Response {
    let config = state.get_config().await;
    let state = Arc::clone(state);
    let event_bus = event_bus.clone();
    
    // 在后台执行扫描
    tokio::spawn(async move {
        tracing::info!("Starting rescan...");
        
        // 扫描视频目录
        let video_result = lianwall_core::wallpaper::scan_directory_async(
            config.paths.video_dir.clone(),
            true, // is_video
        ).await;
        
        // 扫描图片目录  
        let image_result = lianwall_core::wallpaper::scan_directory_async(
            config.paths.image_dir.clone(),
            false, // is_video
        ).await;
        
        // 更新状态
        if let Ok(result) = video_result {
            let paths: Vec<_> = result.wallpapers.into_iter().map(|w| w.path).collect();
            let mut video_space = state.video_space.write().await;
            *video_space = lianwall_core::wallpaper::build_space(paths, 0);
        }
        
        if let Ok(result) = image_result {
            let paths: Vec<_> = result.wallpapers.into_iter().map(|w| w.path).collect();
            let mut image_space = state.image_space.write().await;
            *image_space = lianwall_core::wallpaper::build_space(paths, 0);
        }
        
        // 发布完成事件
        let (video_count, image_count) = {
            (
                state.video_space.read().await.len(),
                state.image_space.read().await.len(),
            )
        };
        
        event_bus.publish(Event::SpaceUpdated {
            reason: SpaceUpdateReason::Rescan,
            video_count,
            image_count,
        });
        
        tracing::info!("Rescan complete: {} videos, {} images", video_count, image_count);
    });
    
    // 立即返回
    Response::ok()
}

/// 关闭 daemon
async fn handle_shutdown(state: &Arc<SharedState>, event_bus: &EventBus) -> Response {
    tracing::info!("Shutdown requested");
    
    // 发布关闭事件
    event_bus.publish(Event::ShuttingDown);
    
    // 停止引擎进程
    state.engine.swww_daemon.kill().await;
    state.engine.mpvpaper.kill().await;
    
    // 触发关闭信号
    state.trigger_shutdown();
    
    Response::ok()
}

/// 重启 daemon
async fn handle_restart(state: &Arc<SharedState>, event_bus: &EventBus) -> Response {
    tracing::info!("Restart requested");
    
    // 发布关闭事件
    event_bus.publish(Event::ShuttingDown);
    
    // 停止引擎进程
    state.engine.swww_daemon.kill().await;
    state.engine.mpvpaper.kill().await;
    
    // TODO: 实际重启逻辑（可能需要 exec 或外部管理）
    // 目前只是停止
    state.trigger_shutdown();
    
    Response::ok()
}

/// 应用壁纸
async fn apply_wallpaper(
    state: &SharedState,
    path: &PathBuf,
    mode: WallMode,
) -> anyhow::Result<()> {
    let config = state.get_config().await;
    
    match mode {
        WallMode::Video => {
            // 停止 swww
            state.engine.swww_daemon.kill().await;
            
            // 启动 mpvpaper
            tracing::info!("Applying video wallpaper: {:?}", path);
            
            let mut cmd = tokio::process::Command::new("mpvpaper");
            
            // 添加 mpvpaper 自身参数
            for arg in &config.video_engine.mpvpaper_args {
                cmd.arg(arg);
            }
            
            // 添加 mpv 参数（通过 -o 传递）
            if !config.video_engine.mpv_args.is_empty() {
                let mpv_args_str = config.video_engine.mpv_args.join(" ");
                cmd.arg("-o").arg(&mpv_args_str);
            }
            
            // 显示器和视频路径
            cmd.arg(&config.video_engine.display);
            cmd.arg(path);
            
            // 抑制输出
            cmd.stdout(std::process::Stdio::null());
            cmd.stderr(std::process::Stdio::null());
            
            match cmd.spawn() {
                Ok(child) => {
                    // 先杀掉旧的 mpvpaper，再设置新的
                    state.engine.mpvpaper.set(child).await;
                }
                Err(e) => {
                    anyhow::bail!("Failed to start mpvpaper: {}", e);
                }
            }
        }
        WallMode::Image => {
            // 停止 mpvpaper
            state.engine.mpvpaper.kill().await;
            
            // 确保 swww-daemon 运行
            if !state.engine.swww_daemon.is_running().await {
                tracing::info!("Starting swww-daemon...");
                
                // 先杀死系统中可能存在的旧 swww-daemon
                let _ = tokio::process::Command::new("pkill")
                    .arg("-x")
                    .arg("swww-daemon")
                    .status()
                    .await;
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                
                // 启动新的 swww-daemon
                let child = tokio::process::Command::new("swww-daemon")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()?;
                
                state.engine.swww_daemon.set(child).await;
                
                // 等待 daemon 初始化
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
            
            // 设置壁纸
            tracing::info!("Applying image wallpaper: {:?}", path);
            
            let mut cmd = tokio::process::Command::new("swww");
            cmd.arg("img");
            
            // 添加目标显示器
            if !config.image_engine.outputs.is_empty() {
                cmd.arg("--outputs").arg(&config.image_engine.outputs);
            }
            
            // 添加 swww 参数
            for arg in &config.image_engine.swww_args {
                cmd.arg(arg);
            }
            
            // 图片路径
            cmd.arg(path);
            
            // 捕获 stderr
            cmd.stderr(std::process::Stdio::piped());
            
            let output = cmd.output().await?;
            
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("swww img failed: {}", stderr.trim());
            }
        }
    }
    
    Ok(())
}
