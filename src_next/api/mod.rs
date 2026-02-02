//! API 层
//!
//! 提供对外接口，封装 Core 层的业务逻辑
//!
//! ## 模块结构
//! - `native`: 原生 Rust API（供 CLI 调用）
//! - `ffi`: FFI 接口（未来 GUI 支持）

pub mod ffi;
pub mod native;

// 重导出常用类型
pub use native::{
    config_get, config_reset, config_set, config_show, diagnose, init, next, reload, start,
    status, stop, switch_mode, uninstall, ApiError, ApiResponse,
};
