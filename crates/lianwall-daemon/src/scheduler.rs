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
    // 检查 GPU 使用率
    // TODO: 从 VramState 获取信息并检查
    
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
pub async fn gpu_monitor(state: Arc<SharedState>, event_bus: EventBus) {
    tracing::info!("GPU monitor started");
    
    let mut shutdown_rx = state.shutdown_receiver();
    let mut timer = interval(Duration::from_secs(5)); // 每 5 秒检查一次
    
    loop {
        tokio::select! {
            _ = timer.tick() => {
                // 获取 GPU 状态
                match lianwall_core::gpu::query_vram(lianwall_core::gpu::detect_backend().await).await {
                    Ok(vram_info) => {
                        let usage = Some(100.0 - vram_info.free_percent);
                        
                        // 发布事件
                        event_bus.publish(Event::GpuStateUpdated { usage });
                    }
                    Err(e) => {
                        tracing::debug!("GPU monitor error: {}", e);
                    }
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
