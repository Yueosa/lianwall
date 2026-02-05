//! Scheduler - 壁纸切换调度器
//!
//! 负责：
//! - 定时触发壁纸切换
//! - 时间点调度（如果配置了特定时间）
//! - GPU 监控触发

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, Instant};

use lianwall_core::socket::Request;
use lianwall_core::config::WallMode;

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
    
    // 初始间隔（从配置读取）
    let config = state.get_config().await;
    let mut current_interval = Duration::from_secs(config.image_engine.interval as u64);
    let mut timer = interval(current_interval);
    
    // 记录下次切换时间
    let mut _next_switch = Instant::now() + current_interval;
    
    loop {
        tokio::select! {
            // 定时触发
            _ = timer.tick() => {
                // 检查是否需要切换
                if should_switch(&state, &event_bus).await {
                    // 发送 Next 命令到命令队列
                    let (response_tx, _response_rx) = tokio::sync::oneshot::channel();
                    let msg = CommandMsg {
                        request: Request::Next,
                        response_tx,
                    };
                    
                    if cmd_tx.send(msg).await.is_err() {
                        tracing::warn!("Failed to send Next command to queue");
                    }
                }
                
                // 更新下次切换时间
                _next_switch = Instant::now() + current_interval;
                
                // 发布 tick 事件（内部使用）
                event_bus.publish(Event::SchedulerTick);
            }
            
            // 配置变更时更新间隔
            // TODO: 监听配置变更事件
            
            // 收到关闭信号
            _ = shutdown_rx.recv() => {
                tracing::info!("Scheduler shutting down");
                break;
            }
        }
        
        // 检查配置是否变更
        let new_config = state.get_config().await;
        let new_interval = Duration::from_secs(new_config.image_engine.interval as u64);
        
        if new_interval != current_interval {
            current_interval = new_interval;
            timer = interval(current_interval);
            _next_switch = Instant::now() + current_interval;
            tracing::info!("Scheduler interval updated to {} seconds", new_config.image_engine.interval);
        }
    }
    
    tracing::info!("Scheduler stopped");
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
/// 3. 根据决策执行模式切换
/// 4. 更新状态快照供查询使用
pub async fn gpu_monitor(state: Arc<SharedState>, event_bus: EventBus) {
    tracing::info!("GPU monitor started");
    
    let mut shutdown_rx = state.shutdown_receiver();
    
    // 从配置获取检查间隔
    let config = state.get_config().await;
    let check_interval = config.vram.check_interval;
    let mut timer = interval(Duration::from_secs(check_interval));
    
    // 初始化 VramState（使用 lianwall_core::gpu::init）
    {
        let vram_state = lianwall_core::gpu::init();
        tracing::info!("GPU backend detected: {:?}", vram_state.backend);
        *state.gpu_state.write().await = Some(vram_state);
    }
    
    loop {
        tokio::select! {
            _ = timer.tick() => {
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
                        }
                    }
                    lianwall_core::gpu::VramAction::Keep => {
                        // 保持当前状态，不做任何操作
                    }
                }
                
                // 更新 GPU 快照（供查询使用）
                if let Ok(vram_info) = lianwall_core::gpu::query_vram(
                    lianwall_core::gpu::detect_backend().await
                ).await {
                    let degraded = {
                        let gpu_state = state.gpu_state.read().await;
                        gpu_state.as_ref().map(|s| s.degraded).unwrap_or(false)
                    };
                    let backend = {
                        let gpu_state = state.gpu_state.read().await;
                        gpu_state.as_ref().map(|s| s.backend).unwrap_or(lianwall_core::gpu::GpuBackend::None)
                    };
                    
                    state.update_gpu_snapshot(vram_info.clone(), degraded, backend).await;
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
