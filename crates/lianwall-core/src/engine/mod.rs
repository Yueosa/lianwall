//! # Engine 模块
//!
//! 壁纸引擎生命周期管理，支持 mpvpaper（视频）和 swww（图片）。
//!
//! ## 生命周期策略（完全接管）
//! - **mpvpaper**: 每次切换壁纸时 kill 并重启（避免内存泄漏）
//! - **swww-daemon**: 完全接管生命周期，杀死任何现有进程再启动我们的
//!
//! ## 模式切换策略（无缝切换）
//! - Video → Image: 启动 swww-daemon 设置壁纸，**后台**关闭 mpvpaper
//! - Image → Video: 启动 mpvpaper，swww clear（不杀 daemon）
//! - 应用退出: 杀死所有 swww-daemon 和 mpvpaper 进程
//!
//! ## 导出接口
//! - 状态: `EngineState`
//! - 操作: `init`, `set_wallpaper`, `switch_mode_seamless`, `shutdown`
//! - 检测: `detect`
//!
//! ## 全异步设计
//! 所有操作都是异步的，使用 tokio::process 管理子进程

mod async_ops;
mod error;
mod r#struct;

pub use error::EngineError;
pub use r#struct::DetectOutput;

// 异步 API（主要接口）
pub use async_ops::{
    detect,
    detect_image_bin,
    init,
    set_wallpaper,
    shutdown,
    switch_mode_seamless,
    clear_wallpaper,
    EngineState,
};
