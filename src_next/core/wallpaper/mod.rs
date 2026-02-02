//! 壁纸扫描模块
//!
//! ## 职责
//! - 扫描壁纸目录并返回符合条件的文件列表
//! - 支持时间段子目录（HH-HH / HHMM-HHMM，支持跨天）
//! - 递归匹配当前时间到对应的子目录
//! - 过滤文件扩展名
//!
//! ## 时间段目录规则
//! - 根目录的文件**始终包含**
//! - 时间段目录匹配当前时间则递归进入
//! - 不匹配的时间段目录直接跳过
//! - 普通目录（非时间段格式）正常递归
//! - 支持无限嵌套
//!
//! ## 使用示例
//! ```rust
//! use crate::core::wallpaper::{scan, WallpaperScanInput};
//! use std::path::PathBuf;
//!
//! let result = scan(WallpaperScanInput {
//!     base_dir: PathBuf::from("~/Videos/lianwall"),
//!     extensions: vec!["mp4".to_string(), "mkv".to_string()],
//!     use_time_ranges: true,
//! }).unwrap();
//!
//! println!("找到 {} 个壁纸", result.wallpapers.len());
//! println!("匹配的时间段: {:?}", result.matched_ranges);
//! ```
//!
//! ## 目录结构示例
//! ```
//! ~/Videos/lianwall/
//!   ├── A.mp4              ← 始终包含
//!   ├── 1800-0100/
//!   │   ├── B.mp4          ← 18:00-01:00 包含
//!   │   └── 1900-0200/
//!   │       └── C.mp4      ← 19:00-02:00 包含（嵌套）
//!   └── 0600-1200/
//!       └── D.mp4          ← 06:00-12:00 包含
//! ```
//!
//! 当前时间 20:30 的扫描结果: [A.mp4, B.mp4, C.mp4]
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
pub use scanner::scan;

// 导出错误类型
pub use error::WallpaperError;

// 导出结构体
pub use r#struct::{TimeRange, WallpaperScanInput, WallpaperScanOutput};

// 导出时间段工具函数
pub use time_range::{is_in_range, parse_time_range, validate_time_range};
