use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 时间段范围（统一转换为分钟表示）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    /// 开始时间（分钟，0-1439）
    pub start_minutes: u16,
    /// 结束时间（分钟，0-1439）
    pub end_minutes: u16,
    /// 是否跨天
    pub is_overnight: bool,
}

// --- IO 结构体 ---

/// 扫描壁纸目录
#[derive(Debug, Clone)]
pub struct WallpaperScanInput {
    /// 基础目录
    pub base_dir: PathBuf,
    /// 允许的文件扩展名（小写，如 ["mp4", "jpg"]）
    pub extensions: Vec<String>,
    /// 是否启用时间段目录匹配（false 则将所有目录视为普通目录）
    pub use_time_ranges: bool,
}

/// 单个壁纸的扫描结果
#[derive(Debug, Clone)]
pub struct ScannedWallpaper {
    /// 壁纸文件路径
    pub path: PathBuf,
    /// 所属时间段目录名（如 "18-23"），根目录为 None
    pub time_range: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WallpaperScanOutput {
    /// 所有符合条件的壁纸（含时间段归属信息）
    pub wallpapers: Vec<ScannedWallpaper>,
    /// 匹配到的时间段目录名列表
    pub matched_ranges: Vec<String>,
    /// 扫描到的文件总数（包括不符合扩展名的）
    pub total_scanned: usize,
    /// 来自根目录的壁纸数量
    pub from_root: usize,
}

/// 时间段目录信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRangeInfo {
    /// 目录名（如 "18-23"）
    pub name: String,
    /// 格式化的开始时间（如 "18:00"）
    pub start_time: String,
    /// 格式化的结束时间（如 "23:00"）
    pub end_time: String,
    /// 该时间段内的壁纸数量
    pub wallpaper_count: usize,
    /// 当前时间是否在此范围内
    pub is_active: bool,
}
