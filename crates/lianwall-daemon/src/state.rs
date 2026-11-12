//! SharedState - Daemon 共享状态管理
//!
//! 使用 Arc<RwLock<T>> 模式管理各组件状态：
//! - config: 配置文件
//! - video_space / image_space: 壁纸空间
//! - engine: 引擎状态（swww/mpvpaper 进程）
//! - gpu: GPU 监控状态

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock, broadcast};

use lianwall_core::config::{Config, WallMode};
use lianwall_core::gpu::{GpuBackend, VramInfo, VramState};
use lianwall_core::wallpaper::{TimePoint, WallpaperSpace};

use crate::hook::HookHandle;

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

/// 空间计数摘要（不 clone items，只取标量）
///
/// 用于 GetStatus 等只需要计数的高频查询，避免深拷贝整个壁纸空间
#[derive(Debug, Clone, Copy)]
pub struct SpaceSummary {
    /// 壁纸总数
    pub total: usize,
    /// 锁定的壁纸数
    pub locked_count: usize,
    /// 冷却队列长度
    pub cooldown_len: usize,
    pub available_count: usize,
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

    pub fn peek_forward(&self) -> Option<PathBuf> {
        self.cursor
            .and_then(|cursor| self.entries.get(cursor + 1))
            .cloned()
    }
    pub fn peek_backward(&self) -> Option<PathBuf> {
        self.cursor
            .and_then(|cursor| cursor.checked_sub(1))
            .and_then(|cursor| self.entries.get(cursor))
            .cloned()
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

/// A foreground engine and its private Unix process group.
/// Dropping an uncommitted replacement also stops descendants.
pub struct EngineProcess {
    pub child: tokio::process::Child,
    group: Option<i32>,
    pub ipc_directory: Option<tempfile::TempDir>,
}

impl EngineProcess {
    pub fn spawn(command: &mut tokio::process::Command) -> std::io::Result<Self> {
        command.process_group(0).kill_on_drop(true);
        let child = command.spawn()?;
        let group = child.id().map(|id| id as i32);
        Ok(Self {
            child,
            group,
            ipc_directory: None,
        })
    }

    fn signal_group(&self, signal: i32) {
        if let Some(group) = self.group {
            // SAFETY: the child was started in its own process group; never signal our group.
            unsafe {
                libc::kill(-group, signal);
            }
        }
    }

    pub async fn stop(mut self) {
        self.signal_group(libc::SIGTERM);
        let _ =
            tokio::time::timeout(std::time::Duration::from_millis(500), self.child.wait()).await;
        // The launcher may have exited while decoder/helper children remain.
        self.signal_group(libc::SIGKILL);
        let _ = self.child.wait().await;
        self.group = None;
    }
}

impl Drop for EngineProcess {
    fn drop(&mut self) {
        self.signal_group(libc::SIGKILL);
    }
}

/// Only engines created by this daemon are owned and stopped.
pub struct ManagedProcess {
    child: Mutex<Option<EngineProcess>>,
    name: &'static str,
}

impl ManagedProcess {
    pub fn new(name: &'static str) -> Self {
        Self {
            child: Mutex::new(None),
            name,
        }
    }
    pub async fn set(&self, child: EngineProcess) {
        let old = self.child.lock().await.replace(child);
        if let Some(old) = old {
            old.stop().await;
        }
        tracing::debug!("{} process started", self.name);
    }
    pub async fn kill(&self) {
        let old = self.child.lock().await.take();
        if let Some(old) = old {
            old.stop().await;
        }
    }
    pub async fn is_running(&self) -> bool {
        let mut guard = self.child.lock().await;
        match guard.as_mut().map(|process| process.child.try_wait()) {
            Some(Ok(None)) => true,
            _ => {
                *guard = None;
                false
            }
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
    pub image_bin: Option<String>,
}

/// 引擎状态（异步安全版本）
pub struct AsyncEngineState {
    /// 当前模式
    pub mode: RwLock<WallMode>,
    /// 当前壁纸路径
    pub current: RwLock<Option<PathBuf>>,
    /// swww-daemon 进程
    pub swww_daemon: ManagedProcess,
    pub image_bin: RwLock<Option<String>>,
    pub owned_ipc: Mutex<Option<(PathBuf, u64, u64)>>,
    /// mpvpaper 进程
    pub mpvpaper: ManagedProcess,
}

impl AsyncEngineState {
    /// 创建新的引擎状态
    pub fn new(mode: WallMode) -> Self {
        Self {
            mode: RwLock::new(mode),
            current: RwLock::new(None),
            swww_daemon: ManagedProcess::new("image-daemon"),
            image_bin: RwLock::new(None),
            owned_ipc: Mutex::new(None),
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
            image_bin: self.image_bin.read().await.clone(),
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

    /// VRAM 手动覆盖状态（Some(true)=强制降级到Image, Some(false)=强制升级到Video, None=自动）
    /// daemon 重启后自动清除，不持久化
    pub vram_override: RwLock<Option<bool>>,

    /// 时间关键点缓存（用于时间调度）
    pub time_points: RwLock<BTreeSet<TimePoint>>,

    /// 浏览器式壁纸播放历史（支持前进/后退导航）
    pub playback_history: RwLock<PlaybackHistory>,

    /// 下次壁纸切换的时间点（用于 status 查询倒计时）
    pub next_switch: RwLock<Option<Instant>>,

    /// 扫描的壁纸原始总数（过滤前），用于 status 显示
    /// (video_scanned, image_scanned)
    pub scanned_counts: RwLock<(usize, usize)>,

    /// 暂停前保存的 video interval（Some = 视频模式暂停中，daemon 重启丢失）
    pub saved_video_interval: RwLock<Option<u64>>,

    /// 暂停前保存的 image interval（Some = 图片模式暂停中，daemon 重启丢失）
    pub saved_image_interval: RwLock<Option<u64>>,

    /// Hook 管理器句柄（hook 系统初始化后设置）
    pub hook_handle: RwLock<Option<HookHandle>>,

    /// 启动时间
    start_time: Instant,
    shutting_down: std::sync::atomic::AtomicBool,

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
            selector_nonce: 0,
            cooldown_queue: std::collections::VecDeque::new(),
            current_index: None,
        };
        let image_space = WallpaperSpace {
            items: Vec::new(),
            pointer: 0.0,
            selector_nonce: 0,
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
            vram_override: RwLock::new(None),
            time_points: RwLock::new(BTreeSet::new()),
            playback_history: RwLock::new(PlaybackHistory::new()),
            next_switch: RwLock::new(
                (initial_interval > 0)
                    .then(|| Instant::now() + std::time::Duration::from_secs(initial_interval)),
            ),
            scanned_counts: RwLock::new((0, 0)),
            saved_video_interval: RwLock::new(None),
            saved_image_interval: RwLock::new(None),
            hook_handle: RwLock::new(None),
            start_time: Instant::now(),
            shutting_down: std::sync::atomic::AtomicBool::new(false),
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
    pub fn is_shutting_down(&self) -> bool {
        self.shutting_down
            .load(std::sync::atomic::Ordering::Acquire)
    }

    pub fn trigger_shutdown(&self) {
        self.shutting_down
            .store(true, std::sync::atomic::Ordering::Release);
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

    /// 获取指定模式空间的计数摘要（不 clone items）
    pub async fn get_space_summary(&self, mode: WallMode) -> SpaceSummary {
        let space = match mode {
            WallMode::Video => self.video_space.read().await,
            WallMode::Image => self.image_space.read().await,
        };
        SpaceSummary {
            total: space.items.len(),
            locked_count: space.items.iter().filter(|w| w.locked).count(),
            cooldown_len: space.cooldown_queue.len(),
            available_count: space.ready_count(),
        }
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
    pub async fn update_gpu_snapshot(
        &self,
        vram_info: VramInfo,
        degraded: bool,
        backend: GpuBackend,
    ) {
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
    pub async fn next_switch_remaining_secs(&self) -> Option<u64> {
        self.next_switch
            .read()
            .await
            .map(|next| next.saturating_duration_since(Instant::now()).as_secs())
    }

    /// None means automatic rotation is paused; it has no countdown deadline.
    pub async fn set_next_switch(&self, instant: Option<Instant>) {
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
