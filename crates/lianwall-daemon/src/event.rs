//! EventBus - 事件发布/订阅系统
//!
//! 使用 broadcast channel 实现一对多事件分发：
//! - 壁纸切换事件
//! - 模式变更事件
//! - 扫描进度/完成事件
//! - 错误事件

use std::path::PathBuf;
use tokio::sync::broadcast;

use lianwall_core::config::WallMode;
use lianwall_core::gpu::{VramAction, VramInfo};
use lianwall_core::socket::WallpaperTrigger;

/// 事件类型
#[derive(Debug, Clone)]
pub enum Event {
    /// 壁纸已切换
    WallpaperChanged {
        path: PathBuf,
        mode: WallMode,
        trigger: WallpaperTrigger,
    },
    
    /// 模式已变更
    ModeChanged {
        from: WallMode,
        to: WallMode,
    },
    
    /// 壁纸空间已更新
    SpaceUpdated {
        reason: SpaceUpdateReason,
        mode: WallMode,
        total: usize,
        available: usize,
        locked: usize,
        in_cooldown: usize,
    },
    
    /// 扫描进度
    ScanProgress {
        mode: WallMode,
        scanned: usize,
        files_found: usize,
        current_dir: PathBuf,
    },
    
    /// 配置已重载
    ConfigReloaded,
    
    /// 引擎状态变更
    EngineStateChanged {
        swww_running: bool,
        mpvpaper_running: bool,
    },
    
    /// GPU 状态更新
    GpuStateUpdated {
        action: VramAction,
        vram_info: Option<VramInfo>,
    },
    
    /// 调度器 tick（内部事件，用于触发定时切换）
    SchedulerTick,
    
    /// 时间点到达（触发重建向量空间）
    TimePointReached {
        /// 当前时间点 "HH:MM"
        time: String,
        /// 下一个时间点 "HH:MM"（可能为 None）
        next_time: Option<String>,
    },
    
    /// 错误事件
    Error {
        message: String,
    },
    
    /// 即将关闭
    ShuttingDown,
}

/// 空间更新原因
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaceUpdateReason {
    /// 初始扫描
    InitialScan,
    /// 手动重新扫描
    Rescan,
    /// 文件变更（如果实现 watch）
    FileChange,
    /// 锁定/解锁
    LockChange,
}

/// 事件总线
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<Event>,
}

impl EventBus {
    /// 创建新的事件总线
    ///
    /// `capacity` 是缓冲区大小，超过后旧事件会被丢弃
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }
    
    /// 发布事件
    ///
    /// 如果没有订阅者，事件会被静默丢弃
    pub fn publish(&self, event: Event) {
        // 忽略发送失败（没有接收者）
        let _ = self.sender.send(event);
    }
    
    /// 订阅事件
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }
    
    /// 获取当前订阅者数量
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_publish_subscribe() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        
        bus.publish(Event::ConfigReloaded);
        
        match rx.recv().await {
            Ok(Event::ConfigReloaded) => {}
            other => panic!("Unexpected event: {:?}", other),
        }
    }
    
    #[tokio::test]
    async fn test_multiple_subscribers() {
        let bus = EventBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();
        
        assert_eq!(bus.subscriber_count(), 2);
        
        bus.publish(Event::ConfigReloaded);
        
        // 两个订阅者都应收到
        assert!(matches!(rx1.recv().await, Ok(Event::ConfigReloaded)));
        assert!(matches!(rx2.recv().await, Ok(Event::ConfigReloaded)));
    }
    
    #[tokio::test]
    async fn test_no_subscriber() {
        let bus = EventBus::new(16);
        
        // 没有订阅者时发布不应 panic
        bus.publish(Event::ConfigReloaded);
    }
}
