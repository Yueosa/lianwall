//! Scheduler - 壁纸切换调度器
//!
//! 负责：
//! - 定时触发壁纸切换
//! - 时间点调度（到达新时间段时重建向量空间）
//! - GPU 监控触发

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, sleep};

use lianwall_core::config::WallMode;
use lianwall_core::socket::{Request, WallpaperTrigger};
use lianwall_core::wallpaper::TimePoint;

use crate::command::CommandMsg;
use crate::event::{Event, EventBus};
use crate::state::SharedState;

/// 轮换计时器
///
/// interval=0 表示轮换已暂停：timer 永久 pending,不会触发切换
/// （tokio::time::interval 不允许零间隔,会 panic)
enum RotationTimer {
    Active(tokio::time::Interval),
    Paused,
}

impl RotationTimer {
    /// 重建计时器并消耗首次立即 tick;间隔为 0 时进入暂停态
    async fn reset(&mut self, dur: Duration) {
        if dur.is_zero() {
            *self = RotationTimer::Paused;
        } else {
            let mut t = interval(dur);
            t.tick().await; // 消耗立即触发的首次 tick
            *self = RotationTimer::Active(t);
        }
    }

    /// 等待下一次触发;暂停态永久 pending
    async fn tick(&mut self) {
        match self {
            RotationTimer::Active(t) => {
                t.tick().await;
            }
            RotationTimer::Paused => std::future::pending::<()>().await,
        }
    }
}

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
pub async fn run(state: Arc<SharedState>, event_bus: EventBus, cmd_tx: mpsc::Sender<CommandMsg>) {
    tracing::info!("Scheduler started");

    let mut shutdown_rx = state.shutdown_receiver();
    let mut event_rx = event_bus.subscribe();

    // 初始间隔（从配置读取，根据当前模式选择）
    let config = state.get_config().await;
    let mode = *state.engine.mode.read().await;
    let mut current_interval = get_interval_for_mode(&config, mode);
    let mut timer = RotationTimer::Paused;
    timer.reset(current_interval).await;

    // 设置下次切换时间（同步到 state 供 status 查询）
    update_rotation_deadline(&state, &event_bus, current_interval).await;

    // 计算下一个时间点的等待时间
    let mut time_point_sleep = create_time_point_sleep(&state).await;

    while !state.is_shutting_down() {
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
                update_rotation_deadline(&state, &event_bus, current_interval).await;

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
                                timer.reset(current_interval).await;
                                update_rotation_deadline(&state, &event_bus, current_interval).await;
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

                        current_interval = new_interval;
                        timer.reset(current_interval).await;
                        update_rotation_deadline(&state, &event_bus, current_interval).await;
                        tracing::info!("Scheduler interval updated to {:?} (mode changed to {:?})", current_interval, to);
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
                            timer.reset(current_interval).await;
                            update_rotation_deadline(&state, &event_bus, current_interval).await;
                            tracing::debug!("Timer reset after manual switch ({:?})", trigger);
                        }
                    }
                    Ok(_) => {
                        // 忽略其他事件
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Scheduler missed {} events; rebuilding schedule from current state", n);
                        let config = state.get_config().await;
                        let mode = *state.engine.mode.read().await;
                        current_interval = get_interval_for_mode(&config, mode);
                        timer.reset(current_interval).await;
                        update_rotation_deadline(&state, &event_bus, current_interval).await;
                        time_point_sleep = create_time_point_sleep(&state).await;
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
            next_point.hour,
            next_point.minute,
            wait_secs
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

/// Publish only after the displayed deadline matches the scheduler's timer.
async fn update_rotation_deadline(state: &SharedState, event_bus: &EventBus, duration: Duration) {
    let deadline = (!duration.is_zero()).then(|| std::time::Instant::now() + duration);
    state.set_next_switch(deadline).await;
    event_bus.publish(Event::SchedulerTick);
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
    use lianwall_core::gpu::{GpuBackend, VramAction, VramState};
    tracing::info!("GPU monitor started");
    let mut shutdown_rx = state.shutdown_receiver();
    let mut config_events = event_bus.subscribe();
    let mut timer = interval(Duration::from_secs(
        state.get_config().await.vram.check_interval.max(1),
    ));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    'monitor: while !state.is_shutting_down() {
        tokio::select! {
            _ = shutdown_rx.recv() => break,
            event = config_events.recv() => {
                if matches!(event, Ok(Event::ConfigChanged { ref key, .. }) if key == "all" || key.starts_with("vram.")) {
                    let config = state.get_config().await;
                    timer = interval(Duration::from_secs(config.vram.check_interval.max(1)));
                    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    if !config.vram.enabled {
                        *state.gpu_state.write().await = None;
                        *state.gpu_snapshot.write().await = crate::state::GpuSnapshot::empty();
                    }
                }
            }
            _ = timer.tick() => {
                let config = state.get_config().await;
                if !config.vram.enabled || state.vram_override.read().await.is_some() { continue; }
                let backend = match config.vram.backend {
                    lianwall_core::config::VramBackend::Auto => lianwall_core::gpu::detect_backend().await,
                    lianwall_core::config::VramBackend::Custom => GpuBackend::Custom { command: config.vram.custom_command.clone() },
                };
                let sample = tokio::select! {
                    _ = shutdown_rx.recv() => break 'monitor,
                    result = lianwall_core::gpu::query_vram(backend.clone()) => result,
                };
                let info = match sample {
                    Ok(info) => info,
                    Err(error) => { tracing::debug!(%error, "GPU query failed; retaining previous state"); continue; }
                };
                let latest = state.get_config().await;
                if !latest.vram.enabled || latest.vram.backend != config.vram.backend
                    || latest.vram.custom_command != config.vram.custom_command
                    || state.vram_override.read().await.is_some() { continue; }
                let (action, degraded) = {
                    let mut guard = state.gpu_state.write().await;
                    let vram = guard.get_or_insert_with(|| VramState::new(backend.clone()));
                    vram.backend = backend.clone();
                    let action = lianwall_core::gpu::decide(vram, &latest.vram, &info);
                    (action, vram.degraded)
                };
                state.update_gpu_snapshot(info.clone(), degraded, backend).await;
                event_bus.publish(Event::GpuStateUpdated { action, vram_info: Some(info) });
                let current = *state.engine.mode.read().await;
                let trigger = match action {
                    VramAction::Downgrade if current == WallMode::Video => Some(WallpaperTrigger::VramDowngrade),
                    VramAction::Upgrade if current == WallMode::Image => Some(WallpaperTrigger::VramUpgrade),
                    _ => None,
                };
                if let Some(trigger) = trigger {
                    let (response_tx, _) = tokio::sync::oneshot::channel();
                    let _ = cmd_tx.send(CommandMsg { request: Request::Next { trigger_hint: Some(trigger) }, response_tx }).await;
                }
            }
        }
    }
    tracing::info!("GPU monitor stopped");
}

#[cfg(test)]
mod gpu_regression_tests {
    use super::*;
    #[tokio::test]
    async fn monitoring_can_be_enabled_at_runtime_and_queries_only_once() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("queries");
        let mut config: lianwall_core::config::Config =
            toml::from_str(lianwall_core::config::DEFAULT_CONFIG_TOML).unwrap();
        config.vram.enabled = false;
        config.vram.check_interval = 60;
        config.vram.backend = lianwall_core::config::VramBackend::Custom;
        config.vram.custom_command = format!(
            "printf x >> '{}'; printf 'used_mb=1\ntotal_mb=1024\n'",
            marker.display()
        );
        let state = SharedState::init(config).await.unwrap();
        let events = EventBus::new(32);
        let (sender, _receiver) = tokio::sync::mpsc::channel(8);
        let task = tokio::spawn(gpu_monitor(state.clone(), events.clone(), sender));
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!marker.exists());
        state.config.write().await.vram.enabled = true;
        events.publish(Event::ConfigChanged {
            key: "vram.enabled".into(),
            old_value: serde_json::json!(false),
            new_value: serde_json::json!(true),
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if state.gpu_snapshot.read().await.vram_info.is_some() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(std::fs::read(&marker).unwrap(), b"x");
        state.config.write().await.vram.enabled = false;
        events.publish(Event::ConfigChanged {
            key: "vram.enabled".into(),
            old_value: serde_json::json!(true),
            new_value: serde_json::json!(false),
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if state.gpu_state.read().await.is_none() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(state.gpu_snapshot.read().await.vram_info.is_none());
        state.trigger_shutdown();
        task.await.unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fixture() -> (Arc<SharedState>, EventBus) {
        let mut config: lianwall_core::config::Config =
            toml::from_str(lianwall_core::config::DEFAULT_CONFIG_TOML).unwrap();
        config.video_engine.interval = 60;
        config.image_engine.interval = 60;
        (SharedState::init(config).await.unwrap(), EventBus::new(32))
    }

    async fn next_schedule(receiver: &mut tokio::sync::broadcast::Receiver<Event>) {
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if matches!(receiver.recv().await.unwrap(), Event::SchedulerTick) {
                    return;
                }
            }
        })
        .await
        .expect("scheduler did not publish the updated deadline");
    }

    #[tokio::test]
    async fn test_paused_schedule_has_no_deadline() {
        let (state, events) = fixture().await;
        let mut receiver = events.subscribe();
        update_rotation_deadline(&state, &events, Duration::ZERO).await;
        next_schedule(&mut receiver).await;
        assert_eq!(state.next_switch_remaining_secs().await, None);
    }

    #[tokio::test]
    async fn test_manual_switch_publishes_new_deadline() {
        let (state, events) = fixture().await;
        let mut receiver = events.subscribe();
        let (sender, _commands) = mpsc::channel(4);
        let task = tokio::spawn(run(state.clone(), events.clone(), sender));
        next_schedule(&mut receiver).await;
        state
            .set_next_switch(Some(std::time::Instant::now() + Duration::from_secs(5)))
            .await;
        events.publish(Event::WallpaperChanged {
            path: "/test.mp4".into(),
            mode: WallMode::Video,
            trigger: WallpaperTrigger::ManualNext,
        });
        next_schedule(&mut receiver).await;
        assert!(state.next_switch_remaining_secs().await.unwrap() >= 58);
        task.abort();
    }

    #[tokio::test]
    async fn test_mode_change_resets_equal_interval_schedule() {
        let (state, events) = fixture().await;
        let mut receiver = events.subscribe();
        let (sender, _commands) = mpsc::channel(4);
        let task = tokio::spawn(run(state.clone(), events.clone(), sender));
        next_schedule(&mut receiver).await;
        state
            .set_next_switch(Some(std::time::Instant::now() + Duration::from_secs(5)))
            .await;
        *state.engine.mode.write().await = WallMode::Image;
        events.publish(Event::ModeChanged {
            from: WallMode::Video,
            to: WallMode::Image,
        });
        next_schedule(&mut receiver).await;
        assert!(state.next_switch_remaining_secs().await.unwrap() >= 58);
        task.abort();
    }
}
