//! API 层
//!
//! 提供对外接口，封装 Core 层的业务逻辑
//!
//! ## 模块结构
//! - `native`: 原生 Rust API（供 CLI 调用）
//! - `ffi`: FFI 接口（未来 GUI 支持）
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

pub mod ffi;
pub mod native;

// 重导出常用类型
pub use native::{
    config_get, config_reset, config_set, config_show, diagnose, init, list, lock, next, reload,
    start, stats, status, stop, switch_mode, unlock, uninstall, ApiError, ApiResponse,
};
