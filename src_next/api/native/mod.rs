//! Native API 层
//!
//! ## 职责
//! - 封装 Core 层接口，提供统一的对外 API
//! - Debug 追踪：记录完整的函数调用链路
//! - 错误追踪：函数级别的错误来源标记
//! - 结构化输出：统一的 ApiResponse 格式
//!
//! ## 公共接口（函数签名）
//!
//! ### 初始化
//! - `init() -> Result<(), ApiError>`
//!
//! ### 生命周期管理
//! - `start(debug: bool) -> Result<ApiResponse<ApiStartOutput>, ApiError>`
//! - `next(debug: bool) -> Result<ApiResponse<ApiNextOutput>, ApiError>`
//! - `switch_mode(debug: bool) -> Result<ApiResponse<ApiSwitchModeOutput>, ApiError>`
//! - `reload(mode: Option<RunMode>, debug: bool) -> Result<ApiResponse<ApiReloadOutput>, ApiError>`
//! - `stop(debug: bool) -> Result<ApiResponse<ApiStopOutput>, ApiError>`
//! - `status(debug: bool) -> Result<ApiResponse<ApiStatusOutput>, ApiError>`
//!
//! ### 壁纸管理
//! - `list(mode: Option<RunMode>, debug: bool) -> Result<ApiResponse<ApiListOutput>, ApiError>`
//! - `lock(mode: RunMode, path: PathBuf, debug: bool) -> Result<ApiResponse<ApiLockOutput>, ApiError>`
//! - `unlock(mode: RunMode, path: PathBuf, debug: bool) -> Result<ApiResponse<ApiLockOutput>, ApiError>`
//! - `stats(mode: Option<RunMode>, debug: bool) -> Result<ApiResponse<ApiStatsOutput>, ApiError>`
//!
//! ### 系统操作
//! - `diagnose(debug: bool) -> Result<ApiResponse<ApiDiagnoseOutput>, ApiError>`
//! - `uninstall(purge_data: bool, debug: bool) -> Result<ApiResponse<ApiUninstallOutput>, ApiError>`
//!
//! ### 配置操作
//! - `config_get(key: &str, debug: bool) -> Result<ApiResponse<ApiConfigGetOutput>, ApiError>`
//! - `config_set(key: &str, value: &str, debug: bool) -> Result<ApiResponse<ApiConfigSetOutput>, ApiError>`
//! - `config_show(debug: bool) -> Result<ApiResponse<ApiConfigShowOutput>, ApiError>`
//! - `config_reset(debug: bool) -> Result<ApiResponse<ApiConfigResetOutput>, ApiError>`
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
pub use core_ops::{list, list_time_ranges, lock, next, reload, start, stats, status, stop, switch_mode, unlock};
pub use system_ops::{diagnose, uninstall};

// 导出类型
pub use error::ApiError;
pub use r#struct::*;
