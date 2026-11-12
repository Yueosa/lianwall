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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use lianwall_core::algorithm::{SelectionConfig, commit_selection, preview_next_with_config};
use lianwall_core::config::WallMode;
use lianwall_core::socket::{ErrorCode, Request, Response, VramOverrideAction, WallpaperTrigger};
use lianwall_core::wallpaper::{WeightsFile, export_to_persisted, save_weights};

use crate::event::{Event, EventBus, SpaceUpdateReason};
use crate::state::SharedState;

fn publish_mode_changed_if_needed(event_bus: &EventBus, old_mode: WallMode, new_mode: WallMode) {
    if old_mode != new_mode {
        event_bus.publish(Event::ModeChanged {
            from: old_mode,
            to: new_mode,
        });
    }
}

async fn selection_config_from_state(state: &Arc<SharedState>) -> SelectionConfig {
    let config = state.config.read().await;
    SelectionConfig {
        bias_lambda: config.algorithm.bias_lambda,
        temperature: config.algorithm.temperature,
    }
}

async fn sync_space_current_index(state: &Arc<SharedState>, mode: WallMode, path: &PathBuf) {
    match mode {
        WallMode::Video => {
            let mut space = state.video_space.write().await;
            space.current_index = space.items.iter().position(|item| &item.path == path);
        }
        WallMode::Image => {
            let mut space = state.image_space.write().await;
            space.current_index = space.items.iter().position(|item| &item.path == path);
        }
    }
}

async fn commit_selected(state: &SharedState, mode: WallMode, path: &PathBuf) {
    let mut space = match mode {
        WallMode::Video => state.video_space.write().await,
        WallMode::Image => state.image_space.write().await,
    };
    if let Some(index) = space.items.iter().position(|item| &item.path == path) {
        commit_selection(&mut space, index);
    }
}

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

        Request::SetWallpaper { path } => {
            handle_set_wallpaper(state, event_bus, path, WallpaperTrigger::ManualSet).await
        }
        Request::SetMode { mode } => handle_set_mode(state, event_bus, mode).await,

        Request::Lock { path } => handle_lock(state, event_bus, path).await,
        Request::Unlock { path } => handle_unlock(state, event_bus, path).await,
        Request::ToggleLock { path } => handle_toggle_lock(state, event_bus, path).await,

        Request::SetConfig { key, value } => handle_set_config(state, event_bus, key, value).await,
        Request::ReloadConfig => handle_reload_config(state, event_bus).await,

        Request::Rescan => handle_rescan(state, event_bus).await,

        Request::ReloadHooks => handle_reload_hooks(state).await,

        Request::Shutdown => handle_shutdown(state, event_bus).await,

        Request::VramOverride { action } => handle_vram_override(state, event_bus, action).await,

        Request::PauseRotation => handle_pause_rotation(state, event_bus).await,
        Request::ResumeRotation => handle_resume_rotation(state, event_bus).await,

        // Query 请求不应该到这里
        _ => Response::error(ErrorCode::InvalidRequest, "Not a command request"),
    }
}

/// 暂停当前模式的自动轮换
///
/// 只改内存 config（不写盘）,saved interval 存 daemon 内存,重启丢失
async fn handle_pause_rotation(state: &Arc<SharedState>, event_bus: &EventBus) -> Response {
    let mode = *state.engine.mode.read().await;
    let (key, saved_lock) = match mode {
        WallMode::Video => ("video_engine.interval", &state.saved_video_interval),
        WallMode::Image => ("image_engine.interval", &state.saved_image_interval),
    };

    let mut config = state.config.write().await;
    let interval = match mode {
        WallMode::Video => config.video_engine.interval,
        WallMode::Image => config.image_engine.interval,
    };
    if interval == 0 {
        return Response::error(ErrorCode::InvalidRequest, "Rotation is already paused");
    }

    *saved_lock.write().await = Some(interval);
    match mode {
        WallMode::Video => config.video_engine.interval = 0,
        WallMode::Image => config.image_engine.interval = 0,
    }
    let old_value = serde_json::Value::from(interval);
    drop(config);

    // 同步清除 next_switch:scheduler 异步处理事件前,客户端查询不能读到旧值
    state.set_next_switch(None).await;

    // 触发 scheduler 重建 timer（interval=0 即暂停）
    event_bus.publish(Event::ConfigChanged {
        key: key.to_string(),
        old_value,
        new_value: serde_json::Value::from(0),
    });

    tracing::info!(
        "Rotation paused for {:?} (saved interval: {}s)",
        mode,
        interval
    );
    Response::ok()
}

/// 恢复当前模式的自动轮换
async fn handle_resume_rotation(state: &Arc<SharedState>, event_bus: &EventBus) -> Response {
    let mode = *state.engine.mode.read().await;
    let (key, saved_lock) = match mode {
        WallMode::Video => ("video_engine.interval", &state.saved_video_interval),
        WallMode::Image => ("image_engine.interval", &state.saved_image_interval),
    };

    let saved = saved_lock.write().await.take();
    let Some(saved) = saved else {
        let config = state.get_config().await;
        let interval = match mode {
            WallMode::Video => config.video_engine.interval,
            WallMode::Image => config.image_engine.interval,
        };
        let message = if interval == 0 {
            "Rotation is paused by configuration; set a positive interval to resume"
        } else {
            "Rotation is not paused"
        };
        return Response::error(ErrorCode::InvalidRequest, message);
    };

    let mut config = state.config.write().await;
    match mode {
        WallMode::Video => config.video_engine.interval = saved,
        WallMode::Image => config.image_engine.interval = saved,
    }
    drop(config);

    // 同步重置 next_switch:scheduler 异步重建 timer 前,客户端查询必须读到新值
    state
        .set_next_switch(Some(Instant::now() + Duration::from_secs(saved)))
        .await;

    event_bus.publish(Event::ConfigChanged {
        key: key.to_string(),
        old_value: serde_json::Value::from(0),
        new_value: serde_json::Value::from(saved),
    });

    tracing::info!("Rotation resumed for {:?} (interval: {}s)", mode, saved);
    Response::ok()
}

/// 切换到下一张壁纸（浏览器式前进）
///
/// 行为：
/// - 光标在末尾：通过算法选出新壁纸，追加到历史，光标指向末尾
/// - 光标不在末尾：光标前进一步，播放光标指向的壁纸
async fn handle_next(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    trigger: WallpaperTrigger,
) -> Response {
    match trigger {
        WallpaperTrigger::VramDowngrade => {
            return handle_set_mode_with_trigger(state, event_bus, WallMode::Image, trigger).await;
        }
        WallpaperTrigger::VramUpgrade => {
            return handle_set_mode_with_trigger(state, event_bus, WallMode::Video, trigger).await;
        }
        _ => (),
    }
    let mode = *state.engine.mode.read().await;
    let selection = selection_config_from_state(state).await;

    // 检查历史光标位置
    let is_at_end = state.playback_history.read().await.is_at_end();

    if !is_at_end {
        // 光标不在末尾：前进一步，播放历史记录
        let path = {
            let history = state.playback_history.read().await;
            match history.peek_forward() {
                Some(p) => p,
                None => return Response::error(ErrorCode::NoHistory, "Cannot forward in history"),
            }
        };

        // 检测壁纸类型决定模式
        let detected_mode = detect_mode(&path);

        // 应用壁纸
        if let Err(e) = apply_wallpaper(state, &path, detected_mode).await {
            return Response::error(
                ErrorCode::EngineError,
                format!("Failed to apply wallpaper: {}", e),
            );
        }

        state.playback_history.write().await.forward();

        // 更新模式和当前壁纸
        *state.engine.mode.write().await = detected_mode;
        *state.engine.current.write().await = Some(path.clone());
        sync_space_current_index(state, detected_mode, &path).await;

        publish_mode_changed_if_needed(event_bus, mode, detected_mode);

        // 发布事件
        event_bus.publish(Event::WallpaperChanged {
            path,
            mode: detected_mode,
            trigger,
        });

        return Response::ok();
    }

    // 光标在末尾：通过算法选出新壁纸
    let path = match mode {
        WallMode::Video => {
            let space = state.video_space.read().await;
            match preview_next_with_config(&space, selection) {
                Some(output) => space.items[output].path.clone(),
                None => {
                    return Response::error(ErrorCode::EmptySpace, "No available video wallpapers");
                }
            }
        }
        WallMode::Image => {
            let space = state.image_space.read().await;
            match preview_next_with_config(&space, selection) {
                Some(output) => space.items[output].path.clone(),
                None => {
                    return Response::error(ErrorCode::EmptySpace, "No available image wallpapers");
                }
            }
        }
    };

    // 应用壁纸
    if let Err(e) = apply_wallpaper(state, &path, mode).await {
        return Response::error(
            ErrorCode::EngineError,
            format!("Failed to apply wallpaper: {}", e),
        );
    }

    commit_selected(state, mode, &path).await;
    state.playback_history.write().await.push(path.clone());

    // 更新当前壁纸
    *state.engine.current.write().await = Some(path.clone());

    // 发布事件
    event_bus.publish(Event::WallpaperChanged {
        path,
        mode,
        trigger,
    });

    Response::ok()
}

/// 切换到上一张壁纸（浏览器式后退）
///
/// 光标后退一步，播放光标指向的壁纸
async fn handle_prev(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    trigger: WallpaperTrigger,
) -> Response {
    let old_mode = *state.engine.mode.read().await;

    // 从播放历史后退
    let path = {
        let history = state.playback_history.read().await;
        match history.peek_backward() {
            Some(p) => p,
            None => {
                return Response::error(ErrorCode::NoHistory, "No previous wallpaper in history");
            }
        }
    };

    // 检测壁纸类型决定模式
    let detected_mode = detect_mode(&path);

    // 应用壁纸（如果文件不存在，apply_wallpaper 会返回错误）
    if let Err(e) = apply_wallpaper(state, &path, detected_mode).await {
        return Response::error(
            ErrorCode::EngineError,
            format!("Failed to apply wallpaper: {}", e),
        );
    }

    state.playback_history.write().await.backward();

    // 更新模式和当前壁纸
    *state.engine.mode.write().await = detected_mode;
    *state.engine.current.write().await = Some(path.clone());
    sync_space_current_index(state, detected_mode, &path).await;

    publish_mode_changed_if_needed(event_bus, old_mode, detected_mode);

    // 发布事件
    event_bus.publish(Event::WallpaperChanged {
        path,
        mode: detected_mode,
        trigger,
    });

    Response::ok()
}

/// 从文件扩展名检测壁纸模式
fn detect_mode(path: &Path) -> WallMode {
    let ext = path
        .extension()
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
    let old_mode = *state.engine.mode.read().await;

    // 检测壁纸类型
    let mode = detect_mode(&path);

    // 应用壁纸
    if let Err(e) = apply_wallpaper(state, &path, mode).await {
        return Response::error(
            ErrorCode::EngineError,
            format!("Failed to apply wallpaper: {}", e),
        );
    }

    // 追加到播放历史（非导航触发，截断光标之后的前进历史）
    state.playback_history.write().await.push(path.clone());

    // 更新模式和当前壁纸
    *state.engine.mode.write().await = mode;
    *state.engine.current.write().await = Some(path.clone());
    sync_space_current_index(state, mode, &path).await;

    publish_mode_changed_if_needed(event_bus, old_mode, mode);

    // 发布事件
    event_bus.publish(Event::WallpaperChanged {
        path,
        mode,
        trigger,
    });

    Response::ok()
}

/// 设置模式
async fn handle_set_mode(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    mode: WallMode,
) -> Response {
    handle_set_mode_with_trigger(state, event_bus, mode, WallpaperTrigger::ModeSwitch).await
}

async fn handle_set_mode_with_trigger(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    mode: WallMode,
    trigger: WallpaperTrigger,
) -> Response {
    let selection = selection_config_from_state(state).await;
    let old_mode = *state.engine.mode.read().await;

    if old_mode == mode {
        return Response::ok(); // 模式未变
    }

    // 先选好壁纸路径，但不提交模式变更
    let path = match mode {
        WallMode::Video => {
            let space = state.video_space.read().await;
            space
                .current_index
                .and_then(|i| space.items.get(i).map(|w| w.path.clone()))
        }
        WallMode::Image => {
            let space = state.image_space.read().await;
            space
                .current_index
                .and_then(|i| space.items.get(i).map(|w| w.path.clone()))
        }
    };

    let path = match path {
        Some(p) => p,
        None => {
            // current_index 为 None（首次进入该模式），通过 select_next 选一张
            match mode {
                WallMode::Video => {
                    let space = state.video_space.read().await;
                    match preview_next_with_config(&space, selection) {
                        Some(output) => space.items[output].path.clone(),
                        None => {
                            // 空间为空，仍然切换模式并发布事件
                            *state.engine.mode.write().await = mode;
                            event_bus.publish(Event::ModeChanged {
                                from: old_mode,
                                to: mode,
                            });
                            return Response::ok();
                        }
                    }
                }
                WallMode::Image => {
                    let space = state.image_space.read().await;
                    match preview_next_with_config(&space, selection) {
                        Some(output) => space.items[output].path.clone(),
                        None => {
                            *state.engine.mode.write().await = mode;
                            event_bus.publish(Event::ModeChanged {
                                from: old_mode,
                                to: mode,
                            });
                            return Response::ok();
                        }
                    }
                }
            }
        }
    };

    // 先应用壁纸，成功后才提交模式变更
    if let Err(e) = apply_wallpaper(state, &path, mode).await {
        // apply 失败 → 不提交模式，保持 old_mode
        return Response::error(
            ErrorCode::EngineError,
            format!("Failed to apply wallpaper: {}", e),
        );
    }

    commit_selected(state, mode, &path).await;
    // 应用成功，提交模式变更
    *state.engine.mode.write().await = mode;

    // 追加到播放历史（模式切换属于非导航触发）
    state.playback_history.write().await.push(path.clone());

    *state.engine.current.write().await = Some(path.clone());

    // 发布模式变更 + 壁纸切换事件
    event_bus.publish(Event::ModeChanged {
        from: old_mode,
        to: mode,
    });
    event_bus.publish(Event::WallpaperChanged {
        path,
        mode,
        trigger,
    });

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
        return Response::error(
            ErrorCode::NotFound,
            format!("Wallpaper not found in any space: {:?}", path),
        );
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
        let available = space.ready_count();
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
async fn handle_lock(state: &Arc<SharedState>, event_bus: &EventBus, path: PathBuf) -> Response {
    modify_lock_state(state, event_bus, path, LockAction::Lock).await
}

/// 解锁壁纸
async fn handle_unlock(state: &Arc<SharedState>, event_bus: &EventBus, path: PathBuf) -> Response {
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

/// 设置配置
async fn handle_set_config(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    key: String,
    value: serde_json::Value,
) -> Response {
    let mut candidate = state.get_config().await;
    let old_value =
        lianwall_core::config::get_value(&candidate, &key).unwrap_or(serde_json::Value::Null);
    let value = match lianwall_core::config::set_value(&mut candidate, &key, value) {
        Ok(value) => value,
        Err(error) => return Response::error(ErrorCode::InvalidRequest, error),
    };
    // Temporary PauseRotation overrides must not leak into unrelated disk edits.
    let mut config_to_save = candidate.clone();
    if key != "video_engine.interval"
        && let Some(saved) = *state.saved_video_interval.read().await
    {
        config_to_save.video_engine.interval = saved;
    }
    if key != "image_engine.interval"
        && let Some(saved) = *state.saved_image_interval.read().await
    {
        config_to_save.image_engine.interval = saved;
    }
    if let Err(error) = lianwall_core::config::update(lianwall_core::config::ConfigUpdateInput {
        path: None,
        config: config_to_save,
    }) {
        return Response::error(
            ErrorCode::ConfigError,
            format!("Failed to save config: {error}"),
        );
    }
    // Commit only after persistence succeeds; errors leave memory and events unchanged.
    state.set_config(candidate).await;
    if key == "video_engine.interval" {
        *state.saved_video_interval.write().await = None;
    } else if key == "image_engine.interval" {
        *state.saved_image_interval.write().await = None;
    }

    // 当前模式的 interval 被修改时,同步更新 next_switch:
    // scheduler 异步重建 timer 前,客户端查询必须读到新值
    let mode = *state.engine.mode.read().await;
    let affects_rotation = matches!(
        (mode, key.as_str()),
        (WallMode::Video, "video_engine.interval") | (WallMode::Image, "image_engine.interval")
    );
    if affects_rotation
        && old_value != value
        && let Some(v) = value.as_u64()
    {
        let base = Instant::now();
        let next = (v > 0).then(|| base + Duration::from_secs(v));
        state.set_next_switch(next).await;
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
    match lianwall_core::config::read(lianwall_core::config::ConfigReadInput { path: None }) {
        Ok(result) => {
            *state.config.write().await = result.config;
            // 磁盘配置为准,清除悬空的轮换暂停态
            *state.saved_video_interval.write().await = None;
            *state.saved_image_interval.write().await = None;
            // 整体重载时 key 为 "all"，old/new 为 null
            event_bus.publish(Event::ConfigChanged {
                key: "all".to_string(),
                old_value: serde_json::Value::Null,
                new_value: serde_json::Value::Null,
            });
            Response::ok()
        }
        Err(e) => Response::error(
            ErrorCode::ConfigError,
            format!("Failed to reload config: {}", e),
        ),
    }
}

/// 重新加载 hooks.toml
async fn handle_reload_hooks(state: &Arc<SharedState>) -> Response {
    let guard = state.hook_handle.read().await;
    match guard.as_ref() {
        Some(handle) => match handle.reload().await {
            Ok(enabled) => {
                tracing::info!("Hooks reloaded: {} enabled", enabled);
                Response::ok()
            }
            Err(e) => Response::error(
                ErrorCode::ConfigError,
                format!("Failed to reload hooks: {}", e),
            ),
        },
        None => Response::error(ErrorCode::InternalError, "Hook system not initialized"),
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
        )
        .await;

        // 扫描图片目录
        let image_result = lianwall_core::wallpaper::scan_directory_async(
            config.paths.image_dir.clone(),
            false, // is_video
        )
        .await;

        // 收集所有壁纸（用于时间点）
        let mut all_wallpapers = Vec::new();
        let video_wallpapers = video_result
            .as_ref()
            .map(|r| r.wallpapers.clone())
            .unwrap_or_default();
        let image_wallpapers = image_result
            .as_ref()
            .map(|r| r.wallpapers.clone())
            .unwrap_or_default();
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
            active_videos.len(),
            video_wallpapers.len(),
            active_images.len(),
            image_wallpapers.len()
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
            let available = video_space.ready_count();
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
            let available = image_space.ready_count();
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
        tracing::info!(
            "Rescan complete: {} videos, {} images",
            video_count,
            image_count
        );
    });

    // 立即返回
    Response::ok()
}

/// 关闭 daemon
async fn handle_shutdown(state: &Arc<SharedState>, _event_bus: &EventBus) -> Response {
    tracing::info!("Shutdown requested");

    // main owns the shared hook/persistence/engine cleanup path.
    // 触发关闭信号
    state.trigger_shutdown();

    Response::ok()
}

async fn apply_wallpaper(state: &SharedState, path: &Path, mode: WallMode) -> anyhow::Result<()> {
    crate::backend::apply(state, path, mode).await
}

/// 手动覆盖 VRAM 状态并立即生效
async fn handle_vram_override(
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    action: VramOverrideAction,
) -> Response {
    let (mode, trigger, value) = match action {
        VramOverrideAction::Reset => {
            *state.vram_override.write().await = None;
            return Response::ok();
        }
        VramOverrideAction::Downgrade => (WallMode::Image, WallpaperTrigger::VramDowngrade, true),
        VramOverrideAction::Upgrade => (WallMode::Video, WallpaperTrigger::VramUpgrade, false),
    };
    let response = handle_set_mode_with_trigger(state, event_bus, mode, trigger).await;
    if matches!(response, Response::Ok) {
        *state.vram_override.write().await = Some(value);
    }
    response
}
