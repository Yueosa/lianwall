//! 配置文件的 CRUD 操作
//!
//! 负责 config.toml 的创建、读取、更新和删除

use std::fs;
use std::path::PathBuf;

use super::default::DEFAULT_CONFIG_TOML;
use super::error::ConfigError;
use super::r#struct::{
    Config, ConfigCreateInput, ConfigCreateOutput, ConfigDeleteInput, ConfigDeleteOutput,
    ConfigReadInput, ConfigReadOutput, ConfigUpdateInput, ConfigUpdateOutput,
};

/// 获取配置文件路径
///
/// 优先使用自定义路径，否则使用默认路径 `~/.config/lianwall/config.toml`
pub fn config_path(custom_path: Option<PathBuf>) -> PathBuf {
    if let Some(path) = custom_path {
        return expand_path_buf(&path);
    }
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("~/.config"));
    base.join("lianwall/config.toml")
}

/// 扩展 `~` 路径写法
pub fn expand_path(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(&path[2..])
    } else {
        PathBuf::from(path)
    }
}

fn expand_path_buf(path: &PathBuf) -> PathBuf {
    let path_str = path.to_string_lossy();
    if path_str.starts_with("~/") {
        expand_path(&path_str)
    } else {
        path.clone()
    }
}

/// 归一化配置中的路径（扩展 `~`）
fn normalize_paths(config: &mut Config) {
    config.paths.video_dir = expand_path_buf(&config.paths.video_dir);
    config.paths.image_dir = expand_path_buf(&config.paths.image_dir);
    config.daemon.socket_path = expand_path_buf(&config.daemon.socket_path);
    config.daemon.pid_path = expand_path_buf(&config.daemon.pid_path);
}

/// 保存配置到文件
fn save_config(path: &PathBuf, config: &Config) -> Result<(), ConfigError> {
    // 确保父目录存在
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| ConfigError::Io {
            operation: "create_dir".to_string(),
            path: parent.to_path_buf(),
            source: e,
        })?;
    }

    let content = toml::to_string_pretty(config).map_err(|e| ConfigError::Serialize {
        path: path.clone(),
        source: e,
    })?;

    fs::write(path, content).map_err(|e| ConfigError::Io {
        operation: "write".to_string(),
        path: path.clone(),
        source: e,
    })?;

    Ok(())
}

// === CRUD 操作 ===

/// Create: 初始化配置（不存在则创建）
pub fn create(input: ConfigCreateInput) -> Result<ConfigCreateOutput, ConfigError> {
    let path = config_path(input.path);

    // 已存在则直接读取返回
    if path.exists() {
        let config = read(ConfigReadInput {
            path: Some(path.clone()),
        })?
        .config;
        return Ok(ConfigCreateOutput {
            path,
            config,
            created: false,
        });
    }

    // 解析默认配置
    let mut config: Config =
        toml::from_str(DEFAULT_CONFIG_TOML).map_err(|e| ConfigError::Parse {
            path: path.clone(),
            source: e,
        })?;
    normalize_paths(&mut config);
    save_config(&path, &config)?;

    Ok(ConfigCreateOutput {
        path,
        config,
        created: true,
    })
}

/// Read: 读取配置
pub fn read(input: ConfigReadInput) -> Result<ConfigReadOutput, ConfigError> {
    let path = config_path(input.path);

    let content = fs::read_to_string(&path).map_err(|e| ConfigError::Io {
        operation: "read".to_string(),
        path: path.clone(),
        source: e,
    })?;

    let mut config: Config = toml::from_str(&content).map_err(|e| ConfigError::Parse {
        path: path.clone(),
        source: e,
    })?;
    normalize_paths(&mut config);

    Ok(ConfigReadOutput { path, config })
}

/// Update: 更新配置（覆盖写入）
pub fn update(input: ConfigUpdateInput) -> Result<ConfigUpdateOutput, ConfigError> {
    let path = config_path(input.path);
    let mut config = input.config;
    normalize_paths(&mut config);

    // 检查是否真正有修改
    let modified = if path.exists() {
        let old_content = fs::read_to_string(&path).map_err(|e| ConfigError::Io {
            operation: "update_read".to_string(),
            path: path.clone(),
            source: e,
        })?;
        let new_content = toml::to_string_pretty(&config).map_err(|e| ConfigError::Serialize {
            path: path.clone(),
            source: e,
        })?;
        old_content != new_content
    } else {
        true
    };

    if modified {
        save_config(&path, &config)?;
    }

    Ok(ConfigUpdateOutput { path, modified })
}

/// Delete: 删除配置文件
pub fn delete(input: ConfigDeleteInput) -> Result<ConfigDeleteOutput, ConfigError> {
    let path = config_path(input.path);

    let deleted = if path.exists() {
        fs::remove_file(&path).map_err(|e| ConfigError::Io {
            operation: "delete".to_string(),
            path: path.clone(),
            source: e,
        })?;
        true
    } else {
        false
    };

    Ok(ConfigDeleteOutput { path, deleted })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_expand_path() {
        let home = env::var("HOME").unwrap_or_else(|_| "/home/test".to_string());
        let expanded = expand_path("~/test/file.txt");
        assert_eq!(expanded, PathBuf::from(format!("{}/test/file.txt", home)));

        let absolute = expand_path("/absolute/path");
        assert_eq!(absolute, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_default_config_parse() {
        let config: Result<Config, _> = toml::from_str(DEFAULT_CONFIG_TOML);
        assert!(config.is_ok(), "Default config should parse successfully");
    }
}
