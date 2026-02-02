//! 配置管理模块
//!
//! ## 职责
//! - 配置文件的 CRUD 操作（创建、读取、更新、删除）
//! - 配置结构的序列化与反序列化
//! - 路径扩展（支持 `~/` 写法）
//! - 默认配置模板提供
//!
//! ## 使用示例
//! ```rust
//! use crate::core::config::{create, read, ConfigCreateInput, ConfigReadInput};
//!
//! // 创建或读取配置（不存在则自动生成默认配置）
//! let output = create(ConfigCreateInput { path: None }).unwrap();
//! println!("配置路径: {:?}", output.path);
//! println!("是否新建: {}", output.created);
//!
//! // 读取现有配置
//! let config = read(ConfigReadInput { path: None }).unwrap().config;
//! println!("当前模式: {}", config.paths.mode);
//! ```
//!
//! ## 错误处理
//! 所有操作返回 `Result<Output, ConfigError>`，错误类型包含：
//! - 操作类型（create/read/update/delete）
//! - 文件路径
//! - 底层错误原因
//!
//! 方便 API 层进行精确的错误定位和审计。

mod config;
mod config_default;
mod error;
mod r#struct;

// 导出 CRUD 函数
pub use config::{create, delete, read, update};

// 导出辅助函数
pub use config::{config_path, expand_path};

// 导出错误类型
pub use error::ConfigError;

// 导出所有结构体
pub use r#struct::{
    Config, ConfigCreateInput, ConfigCreateOutput, ConfigDeleteInput, ConfigDeleteOutput,
    ConfigReadInput, ConfigReadOutput, ConfigUpdateInput, ConfigUpdateOutput, ImageEngineConfig,
    PathsConfig, VideoEngineConfig, VramConfig, WeightConfig,
};

// 导出默认配置常量（供外部诊断或调试使用）
pub use config_default::DEFAULT_CONFIG_TOML;
