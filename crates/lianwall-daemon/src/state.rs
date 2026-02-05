//! SharedState - Daemon 共享状态管理
//!
//! 使用 Arc<RwLock<T>> 模式管理各组件状态：
//! - config: 配置文件
//! - video_space / image_space: 壁纸空间
//! - engine: 引擎状态（swww/mpvpaper 进程）
//! - gpu: GPU 监控状态
//!
//! # 设计决策
//!
//! TODO: 锁粒度优化
//! 当前实现: video_space/image_space 分开锁，配置/引擎/GPU 各自独立
//! 未来期望: 根据实际性能测试结果，可能采用 dashmap 等并发容器
//! 原因: 需要实际数据支撑优化方向

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock, Mutex};

use lianwall_core::config::{Config, WallMode};
use lianwall_core::gpu::VramState;
use lianwall_core::wallpaper::WallpaperSpace;

/// 托管进程包装
///
/// TODO: 引擎进程管理优化
/// 当前方案: Arc<Mutex<Option<Child>>> 包装
/// 未来方案: 进程管理器 Task，通过 channel 发送 spawn/kill 命令
/// 切换原因: 如果进程管理逻辑变复杂（例如需要重启策略、健康检查等），
///         消息传递模式更易于维护和测试
///
/// 重要: 完全接管 swww 和 mpvpaper 的生命周期
/// 任何在程序结束后仍然运行的 swww/mpvpaper 都属于设计失误
pub struct ManagedProcess {
    child: Mutex<Option<tokio::process::Child>>,
    name: &'static str,
}

impl ManagedProcess {
    /// 创建空的托管进程
    pub fn new(name: &'static str) -> Self {
        Self {
            child: Mutex::new(None),
            name,
        }
    }

    /// 设置进程
    pub async fn set(&self, child: tokio::process::Child) {
        let mut guard = self.child.lock().await;
        // 如果已有进程，先 kill
        if let Some(mut old) = guard.take() {
            let _ = old.kill().await;
            let _ = old.wait().await;
        }
        *guard = Some(child);
        tracing::debug!("{} process started", self.name);
    }

    /// 终止进程
    pub async fn kill(&self) {
        let mut guard = self.child.lock().await;
        if let Some(mut child) = guard.take() {
            if let Err(e) = child.kill().await {
                tracing::warn!("Failed to kill {} process: {}", self.name, e);
            }
            let _ = child.wait().await;
            tracing::debug!("{} process stopped", self.name);
        }
    }

    /// 检查进程是否运行中
    pub async fn is_running(&self) -> bool {
        let mut guard = self.child.lock().await;
        if let Some(child) = guard.as_mut() {
            // try_wait 不会阻塞，返回 None 表示仍在运行
            match child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) => {
                    // 进程已退出，清理
                    *guard = None;
                    false
                }
                Err(_) => false,
            }
        } else {
            false
        }
    }
}

/// 引擎状态快照（用于查询响应）
#[derive(Debug, Clone)]
pub struct EngineSnapshot {
    pub mode: WallMode,
    pub current: Option<PathBuf>,
    pub swww_daemon_running: bool,
    pub mpvpaper_running: bool,
}

/// 引擎状态（异步安全版本）
pub struct AsyncEngineState {
    /// 当前模式
    pub mode: RwLock<WallMode>,
    /// 当前壁纸路径
    pub current: RwLock<Option<PathBuf>>,
    /// swww-daemon 进程
    pub swww_daemon: ManagedProcess,
    /// mpvpaper 进程
    pub mpvpaper: ManagedProcess,
}

impl AsyncEngineState {
    /// 从同步 EngineState 创建
    pub fn new() -> Self {
        Self {
            mode: RwLock::new(WallMode::Image),
            current: RwLock::new(None),
            swww_daemon: ManagedProcess::new("swww-daemon"),
            mpvpaper: ManagedProcess::new("mpvpaper"),
        }
    }

    /// 转换为快照（用于序列化）
    pub async fn snapshot(&self) -> EngineSnapshot {
        EngineSnapshot {
            mode: *self.mode.read().await,
            current: self.current.read().await.clone(),
            swww_daemon_running: self.swww_daemon.is_running().await,
            mpvpaper_running: self.mpvpaper.is_running().await,
        }
    }
}

impl Default for AsyncEngineState {
    fn default() -> Self {
        Self::new()
    }
}

/// Daemon 共享状态
pub struct SharedState {
    /// 配置
    pub config: RwLock<Config>,
    
    /// 视频壁纸空间
    pub video_space: RwLock<WallpaperSpace>,
    
    /// 图片壁纸空间
    pub image_space: RwLock<WallpaperSpace>,
    
    /// 引擎状态
    pub engine: AsyncEngineState,
    
    /// GPU 状态
    pub gpu: RwLock<Option<VramState>>,
    
    /// 启动时间
    start_time: Instant,
    
    /// 关闭信号发送端
    shutdown_tx: broadcast::Sender<()>,
}

impl SharedState {
    /// 初始化共享状态
    pub async fn init(config: Config) -> anyhow::Result<Arc<Self>> {
        let (shutdown_tx, _) = broadcast::channel(1);
        
        // 初始化壁纸空间（空的默认空间）
        let video_space = WallpaperSpace {
            items: Vec::new(),
            pointer: 0.0,
            cooldown_queue: std::collections::VecDeque::new(),
            history: Vec::new(),
            current_index: None,
        };
        let image_space = WallpaperSpace {
            items: Vec::new(),
            pointer: 0.0,
            cooldown_queue: std::collections::VecDeque::new(),
            history: Vec::new(),
            current_index: None,
        };
        
        let state = Arc::new(Self {
            config: RwLock::new(config),
            video_space: RwLock::new(video_space),
            image_space: RwLock::new(image_space),
            engine: AsyncEngineState::new(),
            gpu: RwLock::new(None),
            start_time: Instant::now(),
            shutdown_tx,
        });
        
        Ok(state)
    }
    
    /// 获取运行时间（秒）
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
    
    /// 获取关闭信号接收端
    pub fn shutdown_receiver(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }
    
    /// 发送关闭信号
    pub fn trigger_shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
    
    /// 获取配置快照
    pub async fn get_config(&self) -> Config {
        self.config.read().await.clone()
    }
    
    /// 更新配置
    pub async fn set_config(&self, config: Config) {
        *self.config.write().await = config;
    }
    
    /// 获取视频空间快照
    pub async fn get_video_space(&self) -> WallpaperSpace {
        self.video_space.read().await.clone()
    }
    
    /// 获取图片空间快照
    pub async fn get_image_space(&self) -> WallpaperSpace {
        self.image_space.read().await.clone()
    }
    
    /// 更新视频空间
    pub async fn set_video_space(&self, space: WallpaperSpace) {
        *self.video_space.write().await = space;
    }
    
    /// 更新图片空间
    pub async fn set_image_space(&self, space: WallpaperSpace) {
        *self.image_space.write().await = space;
    }
    
    /// 获取引擎状态快照
    pub async fn get_engine_state(&self) -> EngineSnapshot {
        self.engine.snapshot().await
    }
    
    /// 获取 GPU 状态（只读引用）
    pub async fn has_gpu_state(&self) -> bool {
        self.gpu.read().await.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn test_config() -> Config {
        toml::from_str(lianwall_core::config::DEFAULT_CONFIG_TOML)
            .expect("Failed to parse default config")
    }
    
    #[tokio::test]
    async fn test_shared_state_init() {
        let config = test_config();
        let state = SharedState::init(config).await.unwrap();
        
        // 验证初始状态
        let engine = state.get_engine_state().await;
        assert!(!engine.swww_daemon_running);
        assert!(!engine.mpvpaper_running);
    }
    
    #[tokio::test]
    async fn test_shutdown_signal() {
        let config = test_config();
        let state = SharedState::init(config).await.unwrap();
        
        let mut rx = state.shutdown_receiver();
        
        // 触发关闭
        state.trigger_shutdown();
        
        // 应该收到信号
        assert!(rx.recv().await.is_ok());
    }
    
    #[tokio::test]
    async fn test_uptime() {
        let config = test_config();
        let state = SharedState::init(config).await.unwrap();
        
        // 刚启动，uptime 应该很小
        let uptime = state.uptime_secs();
        assert!(uptime < 2);
    }
}
