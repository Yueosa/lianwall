//! # LianWall Core
//!
//! 动态壁纸管理器的核心库，提供以下模块：
//!
//! - [`config`] - 配置文件 CRUD 操作
//! - [`algorithm`] - 向量空间选择算法（黄金角遍历）
//! - [`wallpaper`] - 壁纸扫描、空间管理与持久化
//! - [`engine`] - 壁纸引擎生命周期管理（mpvpaper/swww/awww）
//! - [`gpu`] - 显存监控与降级决策
//! - [`socket`] - Unix Socket 通信

pub mod algorithm;
pub mod config;
pub mod engine;
pub mod gpu;
pub mod hook;
pub mod socket;
pub mod wallpaper;
