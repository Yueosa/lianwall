//! Config 模块：配置文件的统一读写入口。
//!
//! ## 公共接口（函数签名）
//! - create(input: ConfigCreateInput) -> Result<ConfigCreateOutput, ConfigError>
//! - read(input: ConfigReadInput) -> Result<ConfigReadOutput, ConfigError>
//! - update(input: ConfigUpdateInput) -> Result<ConfigUpdateOutput, ConfigError>
//! - delete(input: ConfigDeleteInput) -> Result<ConfigDeleteOutput, ConfigError>
//! - config_path(custom_path: Option<PathBuf>) -> PathBuf
//! - expand_path(path: &str) -> PathBuf
//!
//! ## 输入/输出结构体
//! - ConfigCreateInput / ConfigCreateOutput
//! - ConfigReadInput / ConfigReadOutput
//! - ConfigUpdateInput / ConfigUpdateOutput
//! - ConfigDeleteInput / ConfigDeleteOutput
//!
//! ## 路径处理
//! - 读取/创建/更新后会自动规范化路径（支持 `~/` 展开）。
//! - PathsConfig 中的目录字段为 PathBuf。
//!
//! ## 错误类型
//! - ConfigError::Io
//! - ConfigError::Parse
//! - ConfigError::Serialize

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
