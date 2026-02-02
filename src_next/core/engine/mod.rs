//! 壁纸引擎适配层
//!
//! ## 职责
//! - 统一封装 mpvpaper 和 swww 的调用接口
//! - 检测引擎可用性
//! - 显示器输出检测与回退
//! - 进程管理（先杀后启，避免内存泄漏）
//!
//! ## 支持的引擎
//! - **mpvpaper**：动态壁纸（视频），依赖 mpvpaper
//! - **swww**：静态壁纸（图片），依赖 swww
//!
//! ## 使用示例
//! ```rust
//! use crate::core::engine::{detect, set, EngineType};
//! use crate::core::engine::{EngineDetectInput, EngineSetInput};
//! use std::path::PathBuf;
//!
//! // 检测引擎可用性
//! let detect_result = detect(EngineDetectInput {
//!     engine_type: EngineType::MpvPaper,
//! });
//! if !detect_result.available {
//!     eprintln!("mpvpaper 不可用: {:?}", detect_result.reason);
//! }
//!
//! // 设置壁纸
//! let result = set(EngineSetInput {
//!     engine_type: EngineType::MpvPaper,
//!     wallpaper_path: PathBuf::from("/path/to/video.mp4"),
//!     args: vec!["--no-audio".to_string(), "--loop=inf".to_string()],
//!     outputs: vec![],  // 空 = 自动检测所有显示器
//! });
//! if !result.success {
//!     eprintln!("设置壁纸失败: {:?}", result.error);
//! }
//! ```
//!
//! ## 设计原则
//! - **先杀后启**：每次设置壁纸前先停止旧进程（避免 mpv 内存泄漏）
//! - **互斥启动**：切换引擎时自动停止另一个引擎
//! - **显示器回退**：hyprctl 不可用时自动回退到 "*" 通配符
//! - **用户参数透传**：所有配置参数完全交给用户控制
//!
//! ## 系统依赖
//! - mpvpaper（动态壁纸）
//! - swww（静态壁纸）
//! - hyprctl（可选，用于多显示器检测，不可用时自动回退）

mod error;
mod mpvpaper;
mod r#struct;
mod swww;
mod utils;

use crate::core::engine::error::EngineError;
use crate::core::engine::r#struct::{
    EngineDetectInput, EngineDetectOutput, EngineSetInput, EngineSetOutput, EngineStopInput,
    EngineStopOutput, EngineType,
};

/// 检测引擎可用性
pub fn detect(input: EngineDetectInput) -> Result<EngineDetectOutput, EngineError> {
    match input.engine_type {
        EngineType::MpvPaper => mpvpaper::detect(input),
        EngineType::Swww => swww::detect(input),
    }
}

/// 设置壁纸
pub fn set(input: EngineSetInput) -> Result<EngineSetOutput, EngineError> {
    match input.engine_type {
        EngineType::MpvPaper => mpvpaper::set(input),
        EngineType::Swww => swww::set(input),
    }
}

/// 停止引擎
pub fn stop(input: EngineStopInput) -> Result<EngineStopOutput, EngineError> {
    match input.engine_type {
        EngineType::MpvPaper => mpvpaper::stop(input),
        EngineType::Swww => swww::stop(input),
    }
}

// 导出错误类型
pub use error::EngineError;

// 导出结构体
pub use r#struct::{
    EngineDetectInput, EngineDetectOutput, EngineSetInput, EngineSetOutput, EngineStopInput,
    EngineStopOutput, EngineType,
};
