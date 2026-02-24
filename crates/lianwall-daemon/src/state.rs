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

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock, Mutex};

use lianwall_core::config::{Config, WallMode};
use lianwall_core::gpu::{VramState, VramInfo, GpuBackend};
use lianwall_core::wallpaper::{WallpaperSpace, TimePoint};

/// GPU 状态快照（包含 VramInfo 用于查询）
#[derive(Debug, Clone)]
pub struct GpuSnapshot {
    /// 是否处于降级状态
    pub degraded: bool,
    /// 最新的 VRAM 信息
    pub vram_info: Option<VramInfo>,
    /// GPU 后端类型
    pub backend: GpuBackend,
}

impl GpuSnapshot {
    /// 创建空的快照
    pub fn empty() -> Self {
        Self {
            degraded: false,
            vram_info: None,
            backend: GpuBackend::None,
        }
    }
}

/// 浏览器式播放历史
///
/// 类似浏览器的前进/后退模型：
/// - `entries`: 历史记录列表（有序）
/// - `cursor`: 当前位置索引，指向 entries 中的某一项
///
/// ## 行为规则
/// - **Next（光标在末尾）**: 通过算法选出新壁纸，追加到 entries，cursor 指向末尾
/// - **Next（光标不在末尾）**: cursor 前进一步，播放 entries[cursor]
/// - **Prev**: cursor 后退一步，播放 entries[cursor]
/// - **非导航触发（定时/模式切换等）**: 截断 cursor 之后的所有记录，追加新壁纸，cursor 指向末尾
/// - **最大容量 100 条**: 超出时从前端移除最旧的记录
pub struct PlaybackHistory {
    /// 历史记录列表
    entries: Vec<PathBuf>,
    /// 当前光标位置（指向 entries 的索引）
    /// None 表示历史为空
    cursor: Option<usize>,
}

/// 历史最大容量
const MAX_HISTORY_SIZE: usize = 100;

impl PlaybackHistory {
    /// 创建空的播放历史
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            cursor: None,
        }
    }

    /// 光标是否在末尾（或历史为空）
    pub fn is_at_end(&self) -> bool {
        match self.cursor {
            None => true,
            Some(c) => c + 1 >= self.entries.len(),
        }
    }

    /// 追加新壁纸到历史末尾
    ///
    /// 如果光标不在末尾，先截断光标之后的记录（浏览器模型：从中间导航时新操作会清除前进历史）
    pub fn push(&mut self, path: PathBuf) {
        // 截断光标之后的记录
        if let Some(c) = self.cursor {
            self.entries.truncate(c + 1);
        }

        self.entries.push(path);
        self.cursor = Some(self.entries.len() - 1);

        // 限制容量
        self.trim();
    }

    /// 前进（Next 时光标不在末尾）
    ///
    /// 返回前进后光标指向的壁纸路径
    pub fn forward(&mut self) -> Option<PathBuf> {
        let c = self.cursor?;
        if c + 1 >= self.entries.len() {
            return None; // 已在末尾
        }
        let new_cursor = c + 1;
        self.cursor = Some(new_cursor);
        Some(self.entries[new_cursor].clone())
    }

    /// 后退（Prev）
    ///
    /// 返回后退后光标指向的壁纸路径
    pub fn backward(&mut self) -> Option<PathBuf> {
        let c = self.cursor?;
        if c == 0 {
            return None; // 已在最前
        }
        let new_cursor = c - 1;
        self.cursor = Some(new_cursor);
        Some(self.entries[new_cursor].clone())
    }

    /// 限制容量，超出时从前端移除
    fn trim(&mut self) {
        while self.entries.len() > MAX_HISTORY_SIZE {
            self.entries.remove(0);
            // 调整光标
            if let Some(ref mut c) = self.cursor {
                if *c > 0 {
                    *c -= 1;
                }
            }
        }
    }
}

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

    /// 设置进程（如果已有旧进程，先 kill 旧进程）
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

    /// 设置进程（不杀旧进程，由调用方自行管理旧进程生命周期）
    ///
    /// 用于需要延迟杀死旧进程的场景（如 Video→Video 切换时
    /// 先启动新 mpvpaper，等渲染首帧后再杀旧的）。
    pub async fn set_without_kill(&self, child: tokio::process::Child) {
        let mut guard = self.child.lock().await;
        *guard = Some(child);
        tracing::debug!("{} process started (no-kill mode)", self.name);
    }

    /// 取出进程（不杀死，转移所有权给调用方）
    ///
    /// 取出后内部状态变为 None（无进程）。
    pub async fn take(&self) -> Option<tokio::process::Child> {
        let mut guard = self.child.lock().await;
        guard.take()
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
    /// 创建新的引擎状态
    pub fn new(mode: WallMode) -> Self {
        Self {
            mode: RwLock::new(mode),
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
    
    /// GPU/VRAM 状态（用于监控）
    pub gpu_state: RwLock<Option<VramState>>,
    
    /// GPU 快照（包含最新 VRAM 信息，用于查询）
    pub gpu_snapshot: RwLock<GpuSnapshot>,
    
    /// 时间关键点缓存（用于时间调度）
    pub time_points: RwLock<BTreeSet<TimePoint>>,
    
    /// 浏览器式壁纸播放历史（支持前进/后退导航）
    pub playback_history: RwLock<PlaybackHistory>,
    
    /// 下次壁纸切换的时间点（用于 status 查询倒计时）
    pub next_switch: RwLock<Instant>,
    
    /// 扫描的壁纸原始总数（过滤前），用于 status 显示
    /// (video_scanned, image_scanned)
    pub scanned_counts: RwLock<(usize, usize)>,
    
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
            current_index: None,
        };
        let image_space = WallpaperSpace {
            items: Vec::new(),
            pointer: 0.0,
            cooldown_queue: std::collections::VecDeque::new(),
            current_index: None,
        };
        
        let initial_mode = config.paths.mode;
        
        // 根据初始模式计算首次切换间隔
        let initial_interval = match initial_mode {
            WallMode::Video => config.video_engine.interval,
            WallMode::Image => config.image_engine.interval,
        };
        
        let state = Arc::new(Self {
            config: RwLock::new(config),
            video_space: RwLock::new(video_space),
            image_space: RwLock::new(image_space),
            engine: AsyncEngineState::new(initial_mode),
            gpu_state: RwLock::new(None),
            gpu_snapshot: RwLock::new(GpuSnapshot::empty()),
            time_points: RwLock::new(BTreeSet::new()),
            playback_history: RwLock::new(PlaybackHistory::new()),
            next_switch: RwLock::new(Instant::now() + std::time::Duration::from_secs(initial_interval)),
            scanned_counts: RwLock::new((0, 0)),
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
    
    /// 获取 GPU 快照（用于状态查询）
    pub async fn get_gpu_snapshot(&self) -> GpuSnapshot {
        self.gpu_snapshot.read().await.clone()
    }
    
    /// 更新 GPU 快照
    pub async fn update_gpu_snapshot(&self, vram_info: VramInfo, degraded: bool, backend: GpuBackend) {
        let mut snapshot = self.gpu_snapshot.write().await;
        snapshot.vram_info = Some(vram_info);
        snapshot.degraded = degraded;
        snapshot.backend = backend;
    }
    
    /// 检查是否有 GPU 状态
    pub async fn has_gpu_state(&self) -> bool {
        self.gpu_state.read().await.is_some()
    }
    
    /// 获取时间点集合
    pub async fn get_time_points(&self) -> BTreeSet<TimePoint> {
        self.time_points.read().await.clone()
    }
    
    /// 更新时间点集合
    pub async fn set_time_points(&self, points: BTreeSet<TimePoint>) {
        *self.time_points.write().await = points;
    }
    
    /// 获取下次切换的剩余秒数
    pub async fn next_switch_remaining_secs(&self) -> u64 {
        let next = *self.next_switch.read().await;
        let now = Instant::now();
        if next > now {
            (next - now).as_secs()
        } else {
            0
        }
    }
    
    /// 更新下次切换时间
    pub async fn set_next_switch(&self, instant: Instant) {
        *self.next_switch.write().await = instant;
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
