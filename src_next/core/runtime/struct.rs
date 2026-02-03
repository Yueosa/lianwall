use crate::core::runtime::state::RunMode;

// --- Monitor IO ---

#[derive(Debug, Clone)]
pub struct MonitorCheckInput {
    /// 当前运行模式
    pub current_mode: RunMode,
    /// 是否因 VRAM 降级而切换到 Image 模式（区分主动配置和被动降级）
    pub was_degraded: bool,
    /// 降级阈值（显存剩余百分比）
    pub threshold_percent: f32,
    /// 恢复阈值（显存剩余百分比）
    pub recovery_percent: f32,
}

#[derive(Debug, Clone)]
pub struct MonitorCheckOutput {
    /// 建议的模式动作
    pub action: ModeAction,
    /// 决策原因
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModeAction {
    /// 保持当前模式
    Keep,
    /// 降级到静态壁纸
    DowngradeToImage,
    /// 恢复到动态壁纸
    UpgradeToVideo,
}

// --- Scheduler IO ---

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// 视频切换间隔（秒）
    pub video_interval: u64,
    /// 图片切换间隔（秒）
    pub image_interval: u64,
    /// 是否启用 VRAM 监控
    pub vram_enabled: bool,
    /// VRAM 检测间隔（秒）
    pub vram_check_interval: u64,
    /// VRAM 降级阈值（%）
    pub vram_threshold: f32,
    /// VRAM 恢复阈值（%）
    pub vram_recovery: f32,
}
// --- Scheduler Event ---

/// 调度器事件（用于消息传递）
#[derive(Debug, Clone)]
pub enum SchedulerEvent {
    /// 切换壁纸
    SwitchWallpaper(RunMode),
    /// 降级到图片模式
    DegradeToImage,
    /// 恢复到视频模式
    UpgradeToVideo,
    /// 停止调度器
    Shutdown,
}