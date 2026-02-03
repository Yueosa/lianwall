//! Native API 层
//!
//! ## 职责
//! - 封装 Core 层接口，提供统一的对外 API
//! - Debug 追踪：记录完整的函数调用链路
//! - 错误追踪：函数级别的错误来源标记
//! - 结构化输出：统一的 ApiResponse 格式
//!
//! ## 核心功能
//! - **核心操作**: start/next/switch_mode/reload/stop/status
//! - **系统操作**: diagnose/uninstall
//! - **配置操作**: config_get/set/show/reset
//!
//! ## Debug 模式
//! 启用 debug 后，所有 API 调用都会记录：
//! - 模块路径
//! - 输入参数
//! - 输出结果/错误
//! - 执行时间
//! - 完整的调用栈（嵌套）

pub mod config_ops;
pub mod context;
pub mod core_ops;
pub mod debug;
pub mod error;
pub mod system_ops;
pub mod r#struct;

// 导出核心函数
pub use config_ops::{config_get, config_reset, config_set, config_show};
pub use context::init;
pub use core_ops::{next, reload, start, status, stop, switch_mode};
pub use system_ops::{diagnose, uninstall};

// 导出类型
pub use error::ApiError;
pub use r#struct::*;
