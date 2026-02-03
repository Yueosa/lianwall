//! 运行时调度模块
//!
//! ## 职责
//! - 管理运行时状态（当前壁纸、模式、计数器）
//! - 定时器调度（壁纸切换 + VRAM 检测）
//! - VRAM 监控与模式自动切换
//!
//! ## 设计原则
//! - **通过回调解耦**：Scheduler 不直接操作 Manager，通过回调函数通知外部
//! - **快速响应**：VRAM 检测默认 2 秒间隔，适应游戏瞬时显存占用
//! - **状态透明**：Monitor 返回决策原因和显存信息
//!
//! ## 使用示例
//! ```rust
//! use crate::core::runtime::{scheduler, SchedulerConfig, SchedulerCallbacks, SchedulerRunInput, RuntimeState, RunMode};
//!
//! let config = SchedulerConfig {
//!     video_interval: 600,
//!     image_interval: 600,
//!     vram_enabled: true,
//!     vram_check_interval: 2,
//!     vram_threshold: 25,
//!     vram_recovery: 40,
//! };
//!
//! scheduler::run(SchedulerRunInput {
//!     config,
//!     state: RuntimeState::new(),
//!     callbacks: SchedulerCallbacks {
//!         on_switch: |mode| {
//!             println!("切换壁纸: {:?}", mode);
//!             Ok(())
//!         },
//!         on_degrade: || {
//!             println!("降级到图片模式");
//!             Ok(())
//!         },
//!         on_upgrade: || {
//!             println!("恢复到视频模式");
//!             Ok(())
//!         },
//!     },
//! }).unwrap();
//! ```

mod error;
mod monitor;
mod scheduler;
mod state;
mod r#struct;

// 导出核心函数
pub use monitor::{check as monitor_check};
pub use scheduler::{run as scheduler_run, SchedulerRunInput};

// 导出错误类型
pub use error::RuntimeError;

// 导出结构体
pub use state::{RunMode, RuntimeState};
pub use r#struct::{
    ModeAction, MonitorCheckInput, MonitorCheckOutput, SchedulerConfig, SchedulerEvent,
};
