//! Hook 系统 —— 事件驱动的用户脚本执行
//!
//! 配置文件: `~/.config/lianwall/hooks.toml`
//!
//! - [`HookEntry`] - 单条 hook 规则
//! - [`HookConfig`] - hooks.toml 顶层结构
//! - [`HookEvent`] - 可触发的事件类型
//! - [`run_hook`] - hook 执行器

mod config;
mod runner;

pub use config::{HookConfig, HookEntry, HookEvent, DEFAULT_HOOKS_TOML};
pub use runner::run_hook;

use std::path::PathBuf;

/// 获取 hooks.toml 路径
pub fn hooks_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"));
    base.join("lianwall/hooks.toml")
}

/// 加载 hooks.toml，不存在则创建默认文件
pub fn load_or_create_hooks() -> HookConfig {
    let path = hooks_path();

    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str::<HookConfig>(&content) {
                Ok(config) => return config,
                Err(e) => {
                    tracing::error!("Failed to parse hooks.toml: {}", e);
                    return HookConfig::default();
                }
            },
            Err(e) => {
                tracing::error!("Failed to read hooks.toml: {}", e);
                return HookConfig::default();
            }
        }
    }

    // 文件不存在，创建默认配置
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Err(e) = std::fs::write(&path, DEFAULT_HOOKS_TOML) {
        tracing::warn!("Failed to create default hooks.toml: {}", e);
    } else {
        tracing::info!("Created default hooks.toml at {:?}", path);
    }

    HookConfig::default()
}

/// 仅重新加载 hooks.toml（热更新用）
pub fn reload_hooks() -> Result<HookConfig, String> {
    let path = hooks_path();

    if !path.exists() {
        return Err(format!("hooks.toml not found: {:?}", path));
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read hooks.toml: {}", e))?;

    toml::from_str::<HookConfig>(&content)
        .map_err(|e| format!("Failed to parse hooks.toml: {}", e))
}
