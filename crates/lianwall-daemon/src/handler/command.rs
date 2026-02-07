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

use lianwall_core::socket::{Request, Response, ErrorCode, WallpaperTrigger};
use lianwall_core::config::WallMode;
use lianwall_core::algorithm::select_next;
use lianwall_core::wallpaper::{export_to_persisted, save_weights, WeightsFile};

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
        
        Request::Next { trigger_hint } => {
            let trigger = trigger_hint.unwrap_or(WallpaperTrigger::ManualNext);
            handle_next(state, event_bus, trigger).await
        }
        Request::Prev { trigger_hint } => {
            let trigger = trigger_hint.unwrap_or(WallpaperTrigger::ManualPrev);
            handle_prev(state, event_bus, trigger).await
        }
        
        Request::SetWallpaper { path } => handle_set_wallpaper(state, event_bus, path, WallpaperTrigger::ManualSet).await,
        Request::SetMode { mode } => handle_set_mode(state, event_bus, mode).await,
        
        Request::Lock { path } => handle_lock(state, event_bus, path).await,
        Request::Unlock { path } => handle_unlock(state, event_bus, path).await,
        Request::ToggleLock { path } => handle_toggle_lock(state, event_bus, path).await,
        
        Request::SetConfig { key, value } => handle_set_config(state, event_bus, key, value).await,
        Request::ReloadConfig => handle_reload_config(state, event_bus).await,
        
        Request::Rescan => handle_rescan(state, event_bus).await,
        
        Request::Shutdown => handle_shutdown(state, event_bus).await,
        
        // Query 请求不应该到这里
        _ => Response::error(ErrorCode::InvalidRequest, "Not a command request"),
    }
}

/// 切换到下一张壁纸（浏览器式前进）
///
/// 行为：
/// - 光标在末尾：通过算法选出新壁纸，追加到历史，光标指向末尾
/// - 光标不在末尾：光标前进一步，播放光标指向的壁纸
async fn handle_next(state: &Arc<SharedState>, event_bus: &EventBus, trigger: WallpaperTrigger) -> Response {
    let mode = *state.engine.mode.read().await;
    
    // 检查历史光标位置
    let is_at_end = state.playback_history.read().await.is_at_end();
    
    if !is_at_end {
        // 光标不在末尾：前进一步，播放历史记录
        let path = {
            let mut history = state.playback_history.write().await;
            match history.forward() {
                Some(p) => p,
                None => return Response::error(ErrorCode::NoHistory, "Cannot forward in history"),
            }
        };
        
        // 检测壁纸类型决定模式
        let detected_mode = detect_mode(&path);
        
        // 应用壁纸
        if let Err(e) = apply_wallpaper(state, &path, detected_mode).await {
            return Response::error(ErrorCode::EngineError, format!("Failed to apply wallpaper: {}", e));
        }
        
        // 更新模式和当前壁纸
        *state.engine.mode.write().await = detected_mode;
        *state.engine.current.write().await = Some(path.clone());
        
        // 发布事件
        event_bus.publish(Event::WallpaperChanged { path, mode: detected_mode, trigger });
        
        return Response::ok();
    }
    
    // 光标在末尾：通过算法选出新壁纸
    let path = match mode {
        WallMode::Video => {
            let mut space = state.video_space.write().await;
            match select_next(&mut space) {
                Some(output) => space.items[output.index].path.clone(),
                None => return Response::error(ErrorCode::EmptySpace, "No available video wallpapers"),
            }
        }
        WallMode::Image => {
            let mut space = state.image_space.write().await;
            match select_next(&mut space) {
                Some(output) => space.items[output.index].path.clone(),
                None => return Response::error(ErrorCode::EmptySpace, "No available image wallpapers"),
            }
        }
    };
    
    // 追加到播放历史
    state.playback_history.write().await.push(path.clone());
    
    // 应用壁纸
    if let Err(e) = apply_wallpaper(state, &path, mode).await {
        return Response::error(ErrorCode::EngineError, format!("Failed to apply wallpaper: {}", e));
    }
    
    // 更新当前壁纸
    *state.engine.current.write().await = Some(path.clone());
    
    // 发布事件
    event_bus.publish(Event::WallpaperChanged { path, mode, trigger });
    
    Response::ok()
}

/// 切换到上一张壁纸（浏览器式后退）
///
/// 光标后退一步，播放光标指向的壁纸
async fn handle_prev(state: &Arc<SharedState>, event_bus: &EventBus, trigger: WallpaperTrigger) -> Response {
    // 从播放历史后退
    let path = {
        let mut history = state.playback_history.write().await;
        match history.backward() {
            Some(p) => p,
            None => return Response::error(ErrorCode::NoHistory, "No previous wallpaper in history"),
        }
    };
    
    // 检测壁纸类型决定模式
    let detected_mode = detect_mode(&path);
    
    // 应用壁纸（如果文件不存在，apply_wallpaper 会返回错误）
    if let Err(e) = apply_wallpaper(state, &path, detected_mode).await {
        return Response::error(ErrorCode::EngineError, format!("Failed to apply wallpaper: {}", e));
    }
    
    // 更新模式和当前壁纸
    *state.engine.mode.write().await = detected_mode;
    *state.engine.current.write().await = Some(path.clone());
    
    // 发布事件
    event_bus.publish(Event::WallpaperChanged { path, mode: detected_mode, trigger });
    
    Response::ok()
}

/// 从文件扩展名检测壁纸模式
fn detect_mode(path: &PathBuf) -> WallMode {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    
    if matches!(ext.as_str(), "mp4" | "webm" | "mkv" | "avi" | "mov" | "gif") {
        WallMode::Video
    } else {
        WallMode::Image
    }
}

/// 设置指定壁纸
async fn handle_set_wallpaper(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    path: PathBuf,
    trigger: WallpaperTrigger,
) -> Response {
    // 检测壁纸类型
    let mode = detect_mode(&path);
    
    // 应用壁纸
    if let Err(e) = apply_wallpaper(state, &path, mode).await {
        return Response::error(ErrorCode::EngineError, format!("Failed to apply wallpaper: {}", e));
    }
    
    // 追加到播放历史（非导航触发，截断光标之后的前进历史）
    state.playback_history.write().await.push(path.clone());
    
    // 更新模式和当前壁纸
    *state.engine.mode.write().await = mode;
    *state.engine.current.write().await = Some(path.clone());
    
    // 发布事件
    event_bus.publish(Event::WallpaperChanged { path, mode, trigger });
    
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
    
    // 尝试恢复新模式空间中的当前壁纸；若无，则选一张新的
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
    
    let path = match path {
        Some(p) => p,
        None => {
            // current_index 为 None（首次进入该模式），通过 select_next 选一张
            match mode {
                WallMode::Video => {
                    let mut space = state.video_space.write().await;
                    match select_next(&mut space) {
                        Some(output) => space.items[output.index].path.clone(),
                        None => {
                            // 空间为空，只发布模式变更事件
                            event_bus.publish(Event::ModeChanged { from: old_mode, to: mode });
                            return Response::ok();
                        }
                    }
                }
                WallMode::Image => {
                    let mut space = state.image_space.write().await;
                    match select_next(&mut space) {
                        Some(output) => space.items[output.index].path.clone(),
                        None => {
                            event_bus.publish(Event::ModeChanged { from: old_mode, to: mode });
                            return Response::ok();
                        }
                    }
                }
            }
        }
    };
    
    if let Err(e) = apply_wallpaper(state, &path, mode).await {
        return Response::error(ErrorCode::EngineError, format!("Failed to apply wallpaper: {}", e));
    }
    
    // 追加到播放历史（模式切换属于非导航触发）
    state.playback_history.write().await.push(path.clone());
    
    *state.engine.current.write().await = Some(path.clone());
    
    // 发布模式变更 + 壁纸切换事件
    event_bus.publish(Event::ModeChanged { from: old_mode, to: mode });
    event_bus.publish(Event::WallpaperChanged { path, mode, trigger: WallpaperTrigger::ModeSwitch });
    
    Response::ok()
}

/// 锁定操作类型
enum LockAction {
    Lock,
    Unlock,
    Toggle,
}

/// 修改壁纸锁定状态的统一处理函数
///
/// 同时搜索 video_space 和 image_space，允许跨模式锁定
/// 如果壁纸不在任何空间中，返回 NotFound 错误
async fn modify_lock_state(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    path: PathBuf,
    action: LockAction,
) -> Response {
    let current_mode = *state.engine.mode.read().await;
    
    // 同时在两个空间中搜索并修改锁定状态
    let mut found_in_video = false;
    let mut found_in_image = false;
    
    // 搜索 video_space
    {
        let mut space = state.video_space.write().await;
        if let Some(item) = space.items.iter_mut().find(|w| w.path == path) {
            match action {
                LockAction::Lock => item.locked = true,
                LockAction::Unlock => item.locked = false,
                LockAction::Toggle => item.locked = !item.locked,
            }
            found_in_video = true;
        }
    }
    
    // 搜索 image_space
    {
        let mut space = state.image_space.write().await;
        if let Some(item) = space.items.iter_mut().find(|w| w.path == path) {
            match action {
                LockAction::Lock => item.locked = true,
                LockAction::Unlock => item.locked = false,
                LockAction::Toggle => item.locked = !item.locked,
            }
            found_in_image = true;
        }
    }
    
    if !found_in_video && !found_in_image {
        return Response::error(ErrorCode::NotFound, format!("Wallpaper not found in any space: {:?}", path));
    }
    
    // 立即保存锁定状态到持久化文件
    {
        let video_space = state.video_space.read().await;
        let image_space = state.image_space.read().await;
        
        let weights = WeightsFile {
            version: 1,
            video: export_to_persisted(&video_space),
            image: export_to_persisted(&image_space),
        };
        
        if let Err(e) = save_weights(&weights) {
            tracing::warn!("Failed to save lock state: {}", e);
        }
    }
    
    // 发布空间更新事件
    // 如果找到的空间与当前模式一致，发布当前模式的事件
    // 否则发布找到壁纸的空间的事件
    let event_mode = if found_in_video && current_mode == WallMode::Video {
        WallMode::Video
    } else if found_in_image && current_mode == WallMode::Image {
        WallMode::Image
    } else if found_in_video {
        WallMode::Video
    } else {
        WallMode::Image
    };
    
    publish_space_updated_event(state, event_bus, event_mode, SpaceUpdateReason::LockChange).await;
    
    Response::ok()
}

/// 发布空间更新事件的辅助函数
async fn publish_space_updated_event(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    mode: WallMode,
    reason: SpaceUpdateReason,
) {
    let (total, available, locked, in_cooldown) = {
        let space = if mode == WallMode::Video {
            state.video_space.read().await
        } else {
            state.image_space.read().await
        };
        let total = space.items.len();
        let locked = space.items.iter().filter(|w| w.locked).count();
        let in_cooldown = space.cooldown_queue.len();
        let available = total.saturating_sub(locked).saturating_sub(in_cooldown);
        (total, available, locked, in_cooldown)
    };
    event_bus.publish(Event::SpaceUpdated {
        reason,
        mode,
        total,
        available,
        locked,
        in_cooldown,
    });
}

/// 锁定壁纸
async fn handle_lock(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    path: PathBuf,
) -> Response {
    modify_lock_state(state, event_bus, path, LockAction::Lock).await
}

/// 解锁壁纸
async fn handle_unlock(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    path: PathBuf,
) -> Response {
    modify_lock_state(state, event_bus, path, LockAction::Unlock).await
}

/// 切换锁定状态
async fn handle_toggle_lock(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    path: PathBuf,
) -> Response {
    modify_lock_state(state, event_bus, path, LockAction::Toggle).await
}

/// 获取配置键的当前值（用于事件通知）
fn get_config_value(config: &lianwall_core::config::Config, key: &str) -> serde_json::Value {
    use serde_json::json;
    
    match key {
        // paths
        "paths.mode" => json!(format!("{:?}", config.paths.mode)),
        "paths.video_dir" => json!(config.paths.video_dir.display().to_string()),
        "paths.image_dir" => json!(config.paths.image_dir.display().to_string()),
        // video_engine
        "video_engine.interval" => json!(config.video_engine.interval),
        "video_engine.display" => json!(&config.video_engine.display),
        "video_engine.mpvpaper_args" => json!(&config.video_engine.mpvpaper_args),
        "video_engine.mpv_args" => json!(&config.video_engine.mpv_args),
        // image_engine
        "image_engine.interval" => json!(config.image_engine.interval),
        "image_engine.outputs" => json!(&config.image_engine.outputs),
        "image_engine.swww_args" => json!(&config.image_engine.swww_args),
        // vram
        "vram.enabled" => json!(config.vram.enabled),
        "vram.threshold_percent" => json!(config.vram.threshold_percent),
        "vram.recovery_percent" => json!(config.vram.recovery_percent),
        "vram.check_interval" => json!(config.vram.check_interval),
        "vram.cooldown_seconds" => json!(config.vram.cooldown_seconds),
        // daemon
        "daemon.socket_path" => json!(config.daemon.socket_path.display().to_string()),
        "daemon.pid_path" => json!(config.daemon.pid_path.display().to_string()),
        "daemon.log_level" => json!(&config.daemon.log_level),
        _ => serde_json::Value::Null,
    }
}

/// 设置配置
async fn handle_set_config(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    key: String,
    value: serde_json::Value,
) -> Response {
    let mut config = state.config.write().await;
    
    // 先获取旧值
    let old_value = get_config_value(&config, &key);
    
    match key.as_str() {
        // ==================== paths ====================
        "paths.mode" => {
            if let Some(v) = value.as_str() {
                match v {
                    "Video" => config.paths.mode = WallMode::Video,
                    "Image" => config.paths.mode = WallMode::Image,
                    _ => return Response::error(ErrorCode::InvalidRequest, "Invalid mode value, expected 'Video' or 'Image'"),
                }
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid mode value");
            }
        }
        "paths.video_dir" => {
            if let Some(v) = value.as_str() {
                config.paths.video_dir = lianwall_core::config::expand_path(v);
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid video_dir value");
            }
        }
        "paths.image_dir" => {
            if let Some(v) = value.as_str() {
                config.paths.image_dir = lianwall_core::config::expand_path(v);
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid image_dir value");
            }
        }
        
        // ==================== video_engine ====================
        "video_engine.interval" => {
            if let Some(v) = value.as_u64() {
                if v < 10 || v > 86400 {
                    return Response::error(ErrorCode::InvalidRequest, "interval must be between 10 and 86400");
                }
                config.video_engine.interval = v;
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid interval value");
            }
        }
        "video_engine.display" => {
            if let Some(v) = value.as_str() {
                config.video_engine.display = v.to_string();
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid display value");
            }
        }
        "video_engine.mpvpaper_args" => {
            if let Some(arr) = value.as_array() {
                let args: Result<Vec<String>, _> = arr.iter()
                    .map(|v| v.as_str().map(|s| s.to_string()).ok_or("array item must be string"))
                    .collect();
                match args {
                    Ok(a) => config.video_engine.mpvpaper_args = a,
                    Err(e) => return Response::error(ErrorCode::InvalidRequest, e),
                }
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid mpvpaper_args value, expected array");
            }
        }
        "video_engine.mpv_args" => {
            if let Some(arr) = value.as_array() {
                let args: Result<Vec<String>, _> = arr.iter()
                    .map(|v| v.as_str().map(|s| s.to_string()).ok_or("array item must be string"))
                    .collect();
                match args {
                    Ok(a) => config.video_engine.mpv_args = a,
                    Err(e) => return Response::error(ErrorCode::InvalidRequest, e),
                }
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid mpv_args value, expected array");
            }
        }
        
        // ==================== image_engine ====================
        "image_engine.interval" => {
            if let Some(v) = value.as_u64() {
                if v < 10 || v > 86400 {
                    return Response::error(ErrorCode::InvalidRequest, "interval must be between 10 and 86400");
                }
                config.image_engine.interval = v;
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid interval value");
            }
        }
        "image_engine.outputs" => {
            if let Some(v) = value.as_str() {
                config.image_engine.outputs = v.to_string();
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid outputs value");
            }
        }
        "image_engine.swww_args" => {
            if let Some(arr) = value.as_array() {
                let args: Result<Vec<String>, _> = arr.iter()
                    .map(|v| v.as_str().map(|s| s.to_string()).ok_or("array item must be string"))
                    .collect();
                match args {
                    Ok(a) => config.image_engine.swww_args = a,
                    Err(e) => return Response::error(ErrorCode::InvalidRequest, e),
                }
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid swww_args value, expected array");
            }
        }
        
        // ==================== vram ====================
        "vram.enabled" => {
            if let Some(v) = value.as_bool() {
                config.vram.enabled = v;
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid enabled value, expected boolean");
            }
        }
        "vram.threshold_percent" => {
            if let Some(v) = value.as_f64() {
                if v < 5.0 || v > 50.0 {
                    return Response::error(ErrorCode::InvalidRequest, "threshold_percent must be between 5.0 and 50.0");
                }
                config.vram.threshold_percent = v as f32;
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid threshold_percent value");
            }
        }
        "vram.recovery_percent" => {
            if let Some(v) = value.as_f64() {
                if v < 20.0 || v > 80.0 {
                    return Response::error(ErrorCode::InvalidRequest, "recovery_percent must be between 20.0 and 80.0");
                }
                config.vram.recovery_percent = v as f32;
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid recovery_percent value");
            }
        }
        "vram.check_interval" => {
            if let Some(v) = value.as_u64() {
                if v < 1 || v > 60 {
                    return Response::error(ErrorCode::InvalidRequest, "check_interval must be between 1 and 60");
                }
                config.vram.check_interval = v;
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid check_interval value");
            }
        }
        "vram.cooldown_seconds" => {
            if let Some(v) = value.as_u64() {
                if v < 10 || v > 600 {
                    return Response::error(ErrorCode::InvalidRequest, "cooldown_seconds must be between 10 and 600");
                }
                config.vram.cooldown_seconds = v;
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid cooldown_seconds value");
            }
        }
        
        // ==================== daemon ====================
        "daemon.socket_path" => {
            if let Some(v) = value.as_str() {
                config.daemon.socket_path = PathBuf::from(v);
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid socket_path value");
            }
        }
        "daemon.pid_path" => {
            if let Some(v) = value.as_str() {
                config.daemon.pid_path = PathBuf::from(v);
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid pid_path value");
            }
        }
        "daemon.log_level" => {
            if let Some(v) = value.as_str() {
                match v {
                    "error" | "warn" | "info" | "debug" | "trace" => {
                        config.daemon.log_level = v.to_string();
                    }
                    _ => return Response::error(ErrorCode::InvalidRequest, "Invalid log_level, expected: error/warn/info/debug/trace"),
                }
            } else {
                return Response::error(ErrorCode::InvalidRequest, "Invalid log_level value");
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
    
    // 发布配置变更事件
    event_bus.publish(Event::ConfigChanged {
        key: key.clone(),
        old_value,
        new_value: value,
    });
    
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
            // 整体重载时 key 为 "all"，old/new 为 null
            event_bus.publish(Event::ConfigChanged {
                key: "all".to_string(),
                old_value: serde_json::Value::Null,
                new_value: serde_json::Value::Null,
            });
            Response::ok()
        }
        Err(e) => Response::error(ErrorCode::ConfigError, format!("Failed to reload config: {}", e)),
    }
}

/// 重新扫描壁纸目录
///
/// 设计决策：异步执行 + 进度事件通知
/// 会根据当前时间过滤活跃壁纸
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
        
        // 收集所有壁纸（用于时间点）
        let mut all_wallpapers = Vec::new();
        let video_wallpapers = video_result.as_ref().map(|r| r.wallpapers.clone()).unwrap_or_default();
        let image_wallpapers = image_result.as_ref().map(|r| r.wallpapers.clone()).unwrap_or_default();
        all_wallpapers.extend(video_wallpapers.clone());
        all_wallpapers.extend(image_wallpapers.clone());
        
        // 保存原始扫描总数（过滤前）到 state，供 status 查询
        *state.scanned_counts.write().await = (video_wallpapers.len(), image_wallpapers.len());
        
        // 更新时间点缓存（在过滤前收集）
        let time_points = lianwall_core::wallpaper::collect_time_points(&all_wallpapers);
        tracing::info!("Found {} time points after rescan", time_points.len());
        state.set_time_points(time_points).await;
        
        // 根据当前时间过滤活跃壁纸
        let active_videos = lianwall_core::wallpaper::filter_active(&video_wallpapers);
        let active_images = lianwall_core::wallpaper::filter_active(&image_wallpapers);
        
        tracing::info!(
            "Time filter: {}/{} videos active, {}/{} images active",
            active_videos.len(), video_wallpapers.len(),
            active_images.len(), image_wallpapers.len()
        );
        
        // 更新视频空间（保留历史状态）
        {
            let active_videos_owned: Vec<_> = active_videos.into_iter().cloned().collect();
            let mut video_space = state.video_space.write().await;
            *video_space = lianwall_core::wallpaper::rebuild_space(
                active_videos_owned,
                Some(&video_space),
                None,
                0,
            );
        }
        
        // 更新图片空间（保留历史状态）
        {
            let active_images_owned: Vec<_> = active_images.into_iter().cloned().collect();
            let mut image_space = state.image_space.write().await;
            *image_space = lianwall_core::wallpaper::rebuild_space(
                active_images_owned,
                Some(&image_space),
                None,
                0,
            );
        }
        
        // 发布完成事件（分别发布视频和图片空间的更新）
        {
            let video_space = state.video_space.read().await;
            let total = video_space.items.len();
            let locked = video_space.items.iter().filter(|w| w.locked).count();
            let in_cooldown = video_space.cooldown_queue.len();
            let available = total.saturating_sub(locked).saturating_sub(in_cooldown);
            event_bus.publish(Event::SpaceUpdated {
                reason: SpaceUpdateReason::Rescan,
                mode: WallMode::Video,
                total,
                available,
                locked,
                in_cooldown,
            });
        }
        
        {
            let image_space = state.image_space.read().await;
            let total = image_space.items.len();
            let locked = image_space.items.iter().filter(|w| w.locked).count();
            let in_cooldown = image_space.cooldown_queue.len();
            let available = total.saturating_sub(locked).saturating_sub(in_cooldown);
            event_bus.publish(Event::SpaceUpdated {
                reason: SpaceUpdateReason::Rescan,
                mode: WallMode::Image,
                total,
                available,
                locked,
                in_cooldown,
            });
        }
        
        // 检查当前壁纸是否仍在空间中，如果不在则自动选择新壁纸
        let current_mode = *state.engine.mode.read().await;
        let current_path = state.engine.current.read().await.clone();
        
        let current_valid = if let Some(ref path) = current_path {
            let space = if current_mode == WallMode::Video {
                state.video_space.read().await
            } else {
                state.image_space.read().await
            };
            space.items.iter().any(|w| &w.path == path)
        } else {
            false
        };
        
        if !current_valid && current_path.is_some() {
            // 当前壁纸不在新空间中，但不清除 engine.current
            // 这样屏幕会继续显示旧壁纸，等待下次 interval 或用户手动 next
            tracing::warn!(
                "Current wallpaper {:?} no longer in space after rescan, will switch on next interval",
                current_path
            );
            // current_index 在 rebuild_space 中已经是 None 了（因为找不到该路径）
        }
        
        let video_count = state.video_space.read().await.len();
        let image_count = state.image_space.read().await.len();
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

/// 应用壁纸
///
/// 注意：此函数会检查文件存在性，如果文件不存在会返回错误
async fn apply_wallpaper(
    state: &SharedState,
    path: &PathBuf,
    mode: WallMode,
) -> anyhow::Result<()> {
    // 检查文件是否存在
    if !path.exists() {
        anyhow::bail!("Wallpaper file not found: {:?}", path);
    }
    
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
