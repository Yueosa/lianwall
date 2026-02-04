//! # Config 模块
//!
//! 配置文件的 CRUD 操作与结构定义。
//!
//! ## 文件位置
//! - 默认路径: `~/.config/lianwall/config.toml`
//! - 支持自定义路径覆盖
//!
//! ## 导出接口
//! - CRUD: `create`, `read`, `update`, `delete`
//! - 路径: `config_path`, `expand_path`
//! - 类型: `Config`, `ConfigError` 及所有子结构

mod default;
mod error;
mod ops;
mod r#struct;

// 重导出
pub use default::DEFAULT_CONFIG_TOML;
pub use error::ConfigError;
pub use ops::{config_path, create, delete, expand_path, read, update};
pub use r#struct::*;
