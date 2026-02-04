//! 定义配置文件的结构    操作配置文件用的结构体

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 主配置结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub paths: PathsConfig,
    pub video_engine: VideoEngineConfig,
    pub image_engine: ImageEngineConfig,
    pub vram: VramConfig,
    #[serde(default)]
    pub daemon: DaemonConfig,
}

/// 路径配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    /// 模式: "Video" 或 "Image"
    pub mode: WallMode,
    /// 动态壁纸目录
    pub video_dir: PathBuf,
    /// 静态壁纸目录
    pub image_dir: PathBuf,
}

/// 模式枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WallMode {
    Video,
    Image,
}


/// 动态壁纸引擎配置 (mpvpaper)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoEngineConfig {
    /// 切换间隔（秒）
    pub interval: u64,
    /// 目标显示器（"*" = 所有显示器，或指定如 "eDP-1"）
    pub display: String,
    /// 透传给 mpvpaper 的参数（如 ["-p"] 启动时暂停）
    #[serde(default)]
    pub mpvpaper_args: Vec<String>,
    /// 透传给 mpv 的参数（通过 mpvpaper -o 传递）
    pub mpv_args: Vec<String>,
}

/// 静态壁纸引擎配置 (swww)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEngineConfig {
    /// 切换间隔（秒）
    pub interval: u64,
    /// 目标显示器（空字符串 = 所有显示器，或逗号分隔如 "eDP-1,HDMI-A-1"）
    #[serde(default)]
    pub outputs: String,
    /// 透传给 swww img 的参数
    pub swww_args: Vec<String>,
}

/// 显存监控配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VramConfig {
    /// 是否启用显存监控
    pub enabled: bool,
    /// 降级阈值（%），范围 5.0-50.0
    pub threshold_percent: f32,
    /// 恢复阈值（%），范围 20.0-80.0，必须大于 threshold_percent
    pub recovery_percent: f32,
    /// 检测间隔（秒），范围 1-60
    pub check_interval: u64,
    /// 降级冷却时间（秒），范围 10-600，降级后在此时间内不会恢复
    #[serde(default = "default_cooldown")]
    pub cooldown_seconds: u64,
}

fn default_cooldown() -> u64 {
    30
}

/// 守护进程配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    /// Socket 路径
    pub socket_path: PathBuf,
    /// PID 文件路径
    pub pid_path: PathBuf,
    /// 日志级别: "error", "warn", "info", "debug", "trace"
    pub log_level: String,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/tmp/lianwall.sock"),
            pid_path: PathBuf::from("/tmp/lianwall.pid"),
            log_level: "info".to_string(),
        }
    }
}

// === CRUD IO 结构 ===

/// Create IO
#[derive(Debug, Clone)]
pub struct ConfigCreateInput {
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ConfigCreateOutput {
    pub path: PathBuf,
    pub config: Config,
    pub created: bool,
}

/// Read IO
#[derive(Debug, Clone)]
pub struct ConfigReadInput {
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ConfigReadOutput {
    pub path: PathBuf,
    pub config: Config,
}

/// Update IO
#[derive(Debug, Clone)]
pub struct ConfigUpdateInput {
    pub path: Option<PathBuf>,
    pub config: Config,
}

#[derive(Debug, Clone)]
pub struct ConfigUpdateOutput {
    pub path: PathBuf,
    pub modified: bool,
}

/// Delete IO
#[derive(Debug, Clone)]
pub struct ConfigDeleteInput {
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ConfigDeleteOutput {
    pub path: PathBuf,
    pub deleted: bool,
}
