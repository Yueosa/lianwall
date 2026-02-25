//! Scheduler - 壁纸切换调度器
//!
//! 负责：
//! - 定时触发壁纸切换
//! - 时间点调度（到达新时间段时重建向量空间）
//! - GPU 监控触发

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, sleep, Instant};

use lianwall_core::socket::{Request, WallpaperTrigger};
use lianwall_core::config::WallMode;
use lianwall_core::wallpaper::TimePoint;

use crate::command::CommandMsg;
use crate::event::{Event, EventBus};
use crate::state::SharedState;

/// 调度器配置
pub struct SchedulerConfig {
    /// 切换间隔（秒）
    pub interval_secs: u32,
    /// 是否启用 GPU 监控
    pub gpu_monitor: bool,
    /// GPU 使用率阈值（超过则暂停切换）
    pub gpu_threshold: f32,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            interval_secs: 300, // 5 分钟
            gpu_monitor: false,
            gpu_threshold: 80.0,
        }
    }
}

/// 运行调度器
pub async fn run(
    state: Arc<SharedState>,
    event_bus: EventBus,
    cmd_tx: mpsc::Sender<CommandMsg>,
) {
    tracing::info!("Scheduler started");
    
    let mut shutdown_rx = state.shutdown_receiver();
    let mut event_rx = event_bus.subscribe();
    
    // 初始间隔（从配置读取，根据当前模式选择）
    let config = state.get_config().await;
    let mode = *state.engine.mode.read().await;
    let mut current_interval = get_interval_for_mode(&config, mode);
    let mut timer = interval(current_interval);
    timer.tick().await; // 消耗立即触发的首次 tick，避免启动后立即切换
    
    // 设置下次切换时间（同步到 state 供 status 查询）
    state.set_next_switch((Instant::now() + current_interval).into()).await;
    
    // 计算下一个时间点的等待时间
    let mut time_point_sleep = create_time_point_sleep(&state).await;
    
    loop {
        tokio::select! {
            // 定时触发壁纸切换
            _ = timer.tick() => {
                // 检查是否需要切换
                if should_switch(&state, &event_bus).await {
                    // 发送 Next 命令到命令队列
                    let (response_tx, _response_rx) = tokio::sync::oneshot::channel();
                    let msg = CommandMsg {
                        request: Request::Next { trigger_hint: Some(WallpaperTrigger::Scheduled) },
                        response_tx,
                    };
                    
                    if cmd_tx.send(msg).await.is_err() {
                        tracing::warn!("Failed to send Next command to queue");
                    }
                }
                
                // 更新下次切换时间（同步到 state）
                state.set_next_switch((Instant::now() + current_interval).into()).await;
                
                // 发布 tick 事件（内部使用）
                event_bus.publish(Event::SchedulerTick);
            }
            
            // 时间点到达，重建向量空间
            _ = &mut time_point_sleep => {
                // 获取当前时间和下一个时间点
                let now = lianwall_core::wallpaper::TimePoint::now();
                let time_str = format!("{:02}:{:02}", now.hour, now.minute);
                
                tracing::info!("Time point {} reached, triggering rescan to rebuild space", time_str);
                
                // 发送 Rescan 命令重建向量空间
                let (response_tx, _) = tokio::sync::oneshot::channel();
                let _ = cmd_tx.send(CommandMsg {
                    request: Request::Rescan,
                    response_tx,
                }).await;
                
                // 计算下一个时间点用于事件推送
                let time_points = state.get_time_points().await;
                let next_tp = lianwall_core::wallpaper::next_key_point(&now, &time_points);
                let next_time_str = next_tp.map(|tp| format!("{:02}:{:02}", tp.hour, tp.minute));
                
                // 发布事件
                event_bus.publish(Event::TimePointReached {
                    time: time_str,
                    next_time: next_time_str,
                });
                
                // 重新计算下一个时间点
                time_point_sleep = create_time_point_sleep(&state).await;
            }
            
            // 监听事件
            result = event_rx.recv() => {
                match result {
                    Ok(Event::ConfigChanged { key, .. }) => {
                        // 配置变更，如果是 interval 相关的键或整体重载，更新间隔
                        if key == "all" || key.ends_with(".interval") {
                            let config = state.get_config().await;
                            let mode = *state.engine.mode.read().await;
                            let new_interval = get_interval_for_mode(&config, mode);
                            
                            if new_interval != current_interval {
                                current_interval = new_interval;
                                timer = interval(current_interval);
                                timer.tick().await; // 消耗立即触发的首次 tick
                                state.set_next_switch((Instant::now() + current_interval).into()).await;
                                tracing::info!("Scheduler interval updated to {:?} (config changed: {})", current_interval, key);
                            }
                        }
                        
                        // 整体重载时自动触发 rescan，使壁纸目录与新配置同步
                        if key == "all" {
                            tracing::info!("Full config reload detected, triggering rescan");
                            let (response_tx, _) = tokio::sync::oneshot::channel();
                            let _ = cmd_tx.send(CommandMsg {
                                request: Request::Rescan,
                                response_tx,
                            }).await;
                        }
                    }
                    Ok(Event::ModeChanged { to, .. }) => {
                        // 模式切换，Video/Image 有不同的 interval
                        let config = state.get_config().await;
                        let new_interval = get_interval_for_mode(&config, to);
                        
                        if new_interval != current_interval {
                            current_interval = new_interval;
                            timer = interval(current_interval);
                            timer.tick().await; // 消耗立即触发的首次 tick
                            state.set_next_switch((Instant::now() + current_interval).into()).await;
                            tracing::info!("Scheduler interval updated to {:?} (mode changed to {:?})", current_interval, to);
                        }
                    }
                    Ok(Event::SpaceUpdated { .. }) => {
                        // 空间更新后（如 Rescan），重新计算时间点
                        time_point_sleep = create_time_point_sleep(&state).await;
                    }
                    Ok(Event::WallpaperChanged { trigger, .. }) => {
                        // 手动切换 / 模式切换后重置调度器计时器，从此刻重新计时一个完整 interval
                        if matches!(trigger,
                            WallpaperTrigger::ManualNext |
                            WallpaperTrigger::ManualPrev |
                            WallpaperTrigger::ManualSet |
                            WallpaperTrigger::ModeSwitch
                        ) {
                            timer = interval(current_interval);
                            timer.tick().await; // 消耗立即触发的首次 tick
                            state.set_next_switch((Instant::now() + current_interval).into()).await;
                            tracing::debug!("Timer reset after manual switch ({:?})", trigger);
                        }
                    }
                    Ok(_) => {
                        // 忽略其他事件
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Scheduler missed {} events", n);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("Event bus closed, scheduler stopping");
                        break;
                    }
                }
            }
            
            // 收到关闭信号
            _ = shutdown_rx.recv() => {
                tracing::info!("Scheduler shutting down");
                break;
            }
        }
    }
    
    tracing::info!("Scheduler stopped");
}

/// 创建等待下一个时间点的 sleep future
async fn create_time_point_sleep(state: &SharedState) -> std::pin::Pin<Box<tokio::time::Sleep>> {
    let time_points = state.get_time_points().await;
    
    if time_points.is_empty() {
        // 没有时间点，返回一个很长的等待（实际上不会触发）
        tracing::debug!("No time points configured, time point scheduler disabled");
        return Box::pin(sleep(Duration::from_secs(86400 * 365))); // 1 年
    }
    
    let now = TimePoint::now();
    if let Some(next_point) = lianwall_core::wallpaper::next_key_point(&now, &time_points) {
        let wait_secs = now.seconds_until(&next_point);
        tracing::info!(
            "Next time point: {:02}:{:02}, waiting {} seconds",
            next_point.hour, next_point.minute, wait_secs
        );
        Box::pin(sleep(Duration::from_secs(wait_secs)))
    } else {
        // 不应该发生（如果 time_points 非空）
        Box::pin(sleep(Duration::from_secs(86400 * 365)))
    }
}

/// 根据模式获取切换间隔
fn get_interval_for_mode(config: &lianwall_core::config::Config, mode: WallMode) -> Duration {
    let secs = match mode {
        WallMode::Video => config.video_engine.interval,
        WallMode::Image => config.image_engine.interval,
    };
    Duration::from_secs(secs as u64)
}

/// 检查是否应该切换壁纸
async fn should_switch(state: &SharedState, _event_bus: &EventBus) -> bool {
    // 检查配置是否启用 VRAM 监控
    let config = state.get_config().await;
    
    if config.vram.enabled {
        // 获取 GPU 快照
        let gpu_snapshot = state.get_gpu_snapshot().await;
        
        // 如果已降级，不切换视频壁纸
        if gpu_snapshot.degraded {
            let mode = *state.engine.mode.read().await;
            if mode == WallMode::Video {
                tracing::debug!("Skip switch: VRAM degraded");
                return false;
            }
        }
        
        // 如果有 VRAM 信息，检查阈值
        if let Some(ref vram_info) = gpu_snapshot.vram_info {
            if vram_info.free_percent < config.vram.threshold_percent {
                tracing::debug!(
                    "Skip switch: VRAM low ({:.1}% free < {:.1}% threshold)",
                    vram_info.free_percent,
                    config.vram.threshold_percent
                );
                return false;
            }
        }
    }
    
    // 检查是否有壁纸可切换
    let mode = *state.engine.mode.read().await;
    
    match mode {
        WallMode::Video => {
            let space = state.video_space.read().await;
            !space.items.is_empty()
        }
        WallMode::Image => {
            let space = state.image_space.read().await;
            !space.items.is_empty()
        }
    }
}

/// GPU 监控 Task
///
/// 负责：
/// 1. 定期查询 VRAM 状态
/// 2. 调用 check() 做降级/升级决策（包含冷却逻辑）
/// 3. 根据决策执行模式切换 + 壁纸切换
/// 4. 更新状态快照供查询使用
pub async fn gpu_monitor(
    state: Arc<SharedState>,
    event_bus: EventBus,
    cmd_tx: mpsc::Sender<CommandMsg>,
) {
    tracing::info!("GPU monitor started");
    
    let mut shutdown_rx = state.shutdown_receiver();
    
    // 从配置获取检查间隔
    let config = state.get_config().await;
    let check_interval = config.vram.check_interval;
    let mut timer = interval(Duration::from_secs(check_interval));
    
    // 初始化 VramState（根据配置选择后端）
    {
        let vram_state = lianwall_core::gpu::init_with_config(&config.vram);
        tracing::info!("GPU backend detected: {:?}", vram_state.backend);
        *state.gpu_state.write().await = Some(vram_state);
    }

    // 验证自定义后端配置
    if config.vram.backend == lianwall_core::config::VramBackend::Custom {
        if config.vram.custom_command.trim().is_empty() {
            tracing::error!("vram.custom_command is empty while backend=custom, disabling GPU monitor");
            return;
        }
        let backend = lianwall_core::gpu::GpuBackend::Custom {
            command: config.vram.custom_command.clone(),
        };
        match lianwall_core::gpu::query_vram(backend).await {
            Ok(_) => tracing::info!("Custom VRAM command validated OK"),
            Err(e) => {
                tracing::error!("Custom VRAM command trial run failed: {}, disabling GPU monitor", e);
                return;
            }
        }
    }

    loop {
        tokio::select! {
            _ = timer.tick() => {
                // 手动覆盖状态下，跳过自动检测
                if state.vram_override.read().await.is_some() {
                    continue;
                }
                let config = state.get_config().await;
                
                // 使用 check() 函数做降级/升级决策
                let action = {
                    let mut gpu_state_guard = state.gpu_state.write().await;
                    if let Some(ref mut vram_state) = *gpu_state_guard {
                        match lianwall_core::gpu::check(vram_state, &config.vram) {
                            Ok(action) => action,
                            Err(e) => {
                                tracing::debug!("GPU check error: {}", e);
                                lianwall_core::gpu::VramAction::Keep
                            }
                        }
                    } else {
                        lianwall_core::gpu::VramAction::Keep
                    }
                };
                
                // 根据决策执行操作
                match action {
                    lianwall_core::gpu::VramAction::Downgrade => {
                        tracing::warn!("VRAM low, downgrading to image mode");
                        // 切换到图片模式
                        let current_mode = *state.engine.mode.read().await;
                        if current_mode == WallMode::Video {
                            *state.engine.mode.write().await = WallMode::Image;
                            event_bus.publish(Event::ModeChanged {
                                from: WallMode::Video,
                                to: WallMode::Image,
                            });
                            
                            // 发送 Next 命令切换到新模式的壁纸
                            let (response_tx, _) = tokio::sync::oneshot::channel();
                            let _ = cmd_tx.send(CommandMsg {
                                request: Request::Next {
                                    trigger_hint: Some(WallpaperTrigger::VramDowngrade),
                                },
                                response_tx,
                            }).await;
                        }
                    }
                    lianwall_core::gpu::VramAction::Upgrade => {
                        tracing::info!("VRAM recovered, upgrading to video mode");
                        // 切换回视频模式
                        let current_mode = *state.engine.mode.read().await;
                        if current_mode == WallMode::Image {
                            *state.engine.mode.write().await = WallMode::Video;
                            event_bus.publish(Event::ModeChanged {
                                from: WallMode::Image,
                                to: WallMode::Video,
                            });
                            
                            // 发送 Next 命令切换到新模式的壁纸
                            let (response_tx, _) = tokio::sync::oneshot::channel();
                            let _ = cmd_tx.send(CommandMsg {
                                request: Request::Next {
                                    trigger_hint: Some(WallpaperTrigger::VramUpgrade),
                                },
                                response_tx,
                            }).await;
                        }
                    }
                    lianwall_core::gpu::VramAction::Keep => {
                        // 保持当前状态，不做任何操作
                    }
                }
                
                // 更新 GPU 快照（供查询使用）
                let backend_for_query = {
                    let gpu_state = state.gpu_state.read().await;
                    gpu_state.as_ref().map(|s| s.backend.clone())
                        .unwrap_or(lianwall_core::gpu::GpuBackend::None)
                };
                if let Ok(vram_info) = lianwall_core::gpu::query_vram(backend_for_query.clone()).await {
                    let degraded = {
                        let gpu_state = state.gpu_state.read().await;
                        gpu_state.as_ref().map(|s| s.degraded).unwrap_or(false)
                    };

                    state.update_gpu_snapshot(vram_info.clone(), degraded, backend_for_query).await;
                    event_bus.publish(Event::GpuStateUpdated {
                        action,
                        vram_info: Some(vram_info),
                    });
                }
            }
            
            _ = shutdown_rx.recv() => {
                tracing::info!("GPU monitor shutting down");
                break;
            }
        }
    }
    
    tracing::info!("GPU monitor stopped");
}
