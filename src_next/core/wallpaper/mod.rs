//! Wallpaper 模块：壁纸扫描与时间段匹配的统一入口。
//!
//! ## 公共接口（函数签名）
//! - scan(input: WallpaperScanInput) -> Result<WallpaperScanOutput, WallpaperError>
//! - scan_all_time_ranges(input: WallpaperScanInput) -> Result<Vec<TimeRangeInfo>, WallpaperError>
//! - parse_time_range(input: &str) -> Option<TimeRange>
//! - is_in_range(range: &TimeRange, now: (u8, u8)) -> bool
//! - validate_time_range(input: &str) -> Result<TimeRange, WallpaperError>
//!
//! ## 输入/输出结构体
//! - WallpaperScanInput / WallpaperScanOutput
//! - ScannedWallpaper / TimeRange / TimeRangeInfo
//!
//! ## 职责
//! - 扫描壁纸目录并返回符合条件的文件列表
//! - 支持时间段子目录（HH-HH / HHMM-HHMM，支持跨天）
//! - 递归匹配当前时间到对应的子目录
//! - 过滤文件扩展名
//!
//! ## 设计原则
//! - **递归匹配**：支持无限嵌套时间段目录
//! - **合并结果**：匹配的所有层级都会收集
//! - **精确到分钟**：时间段判断精确到分钟级别
//! - **错误透传**：使用 Result + thiserror

mod error;
mod scanner;
mod r#struct;
mod time_range;

// 导出核心函数
pub use scanner::{scan, scan_all_time_ranges};

// 导出错误类型
pub use error::WallpaperError;

// 导出结构体
pub use r#struct::{ScannedWallpaper, TimeRange, TimeRangeInfo, WallpaperScanInput, WallpaperScanOutput};

// 导出时间段工具函数
pub use time_range::{format_time_range, is_in_range, parse_time_range, validate_time_range};
