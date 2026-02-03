//! Runtime 模块：运行时调度与状态管理的统一入口。
//!
//! ## 公共接口（函数签名）
//! - scheduler_run(input: SchedulerRunInput) -> Result<(), RuntimeError>
//! - monitor_check(input: MonitorCheckInput) -> MonitorCheckOutput
//!
//! ## 输入/输出结构体
//! - SchedulerConfig / SchedulerEvent / SchedulerRunInput
//! - MonitorCheckInput / MonitorCheckOutput / ModeAction
//! - RuntimeState / RunMode
//!
//! ## 职责
//! - 管理运行时状态（当前壁纸、模式、计数器）
//! - 定时器调度（壁纸切换 + VRAM 检测）
//! - VRAM 监控与模式自动切换决策
//!
//! ## 设计原则
//! - **事件驱动**：Scheduler 通过事件通道通知上层
//! - **快速响应**：VRAM 检测短间隔，适应瞬时显存波动
//! - **保守策略**：显存信息不可用时不触发切换

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
