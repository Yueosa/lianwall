//! # Wallpaper 模块
//!
//! 壁纸扫描、向量空间管理与持久化。
//!
//! ## 核心概念
//! - **WallpaperRecord**: 单个壁纸记录（路径 + 角度 + 状态）
//! - **WallpaperSpace**: 向量空间（壁纸集合 + 指针 + 冷却队列）
//! - **WeightsFile**: 持久化文件结构
//! - **TimeRange**: 时间段目录支持
//!
//! ## 导出接口
//! - 扫描: `scan_directory`, `filter_active`
//! - 时间: `TimePoint`, `TimeRange`, `next_key_point`
//! - 空间: `build_space`, `rebuild_space`
//! - 持久化: `load_weights`, `save_weights`, `weights_path`

mod error;
mod persist;
mod scanner;
mod space;
mod r#struct;
mod time_range;

pub use error::WallpaperError;
pub use persist::{load_weights, save_weights, weights_path};
pub use scanner::{filter_active, scan_directory, ScanResult, ScannedWallpaper};
pub use space::{build_space, export_to_persisted, rebuild_space};
pub use r#struct::*;
pub use time_range::{next_key_point, TimePoint, TimeRange};
