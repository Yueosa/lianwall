//! # Wallpaper 模块
//!
//! 壁纸扫描、向量空间管理与持久化。
//!
//! ## 核心概念
//! - **WallpaperRecord**: 单个壁纸记录（路径 + 角度 + 状态）
//! - **WallpaperSpace**: 向量空间（壁纸集合 + 指针 + 冷却队列）
//! - **WeightsFile**: 持久化文件结构
//!
//! ## 导出接口
//! - 扫描: `scan_directory`
//! - 空间: `build_space`, `rebuild_space`
//! - 持久化: `load_weights`, `save_weights`, `weights_path`

mod error;
mod persist;
mod scanner;
mod space;
mod r#struct;

pub use error::WallpaperError;
pub use persist::{load_weights, save_weights, weights_path};
pub use scanner::scan_directory;
pub use space::{build_space, export_to_persisted, rebuild_space};
pub use r#struct::*;
