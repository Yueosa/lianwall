use std::fs;
use std::path::PathBuf;

use crate::core::config::config_default::DEFAULT_CONFIG_TOML;
use crate::core::config::error::ConfigError;
use crate::core::config::r#struct::{
    Config, ConfigCreateInput, ConfigCreateOutput, ConfigDeleteInput, ConfigDeleteOutput,
    ConfigReadInput, ConfigReadOutput, ConfigUpdateInput, ConfigUpdateOutput,
};

/// 默认配置路径（允许外部传入自定义路径覆盖）
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

/// C: 初始化配置（不存在则创建）
pub fn create(input: ConfigCreateInput) -> Result<ConfigCreateOutput, ConfigError> {
    let path = config_path(input.path);

    if path.exists() {
        let config = read(ConfigReadInput { path: Some(path.clone()) })?.config;
        return Ok(ConfigCreateOutput { 
            path, 
            config,
            created: false,
        });
    }

    let config: Config = toml::from_str(DEFAULT_CONFIG_TOML)
        .map_err(|e| ConfigError::Parse { 
            path: path.clone(), 
            source: e 
        })?;
    let mut config = config;
    normalize_paths(&mut config);
    save_config(&path, &config)?;

    Ok(ConfigCreateOutput { 
        path, 
        config,
        created: true,
    })
}

/// R: 读取配置
pub fn read(input: ConfigReadInput) -> Result<ConfigReadOutput, ConfigError> {
    let path = config_path(input.path);
    let content = fs::read_to_string(&path)
        .map_err(|e| ConfigError::Io { 
            operation: "read".to_string(), 
            path: path.clone(), 
            source: e 
        })?;
    let config: Config = toml::from_str(&content)
        .map_err(|e| ConfigError::Parse { 
            path: path.clone(), 
            source: e 
        })?;
    let mut config = config;
    normalize_paths(&mut config);
    Ok(ConfigReadOutput { path, config })
}

/// U: 更新配置（覆盖写入）
pub fn update(input: ConfigUpdateInput) -> Result<ConfigUpdateOutput, ConfigError> {
    let path = config_path(input.path);
    let mut config = input.config;
    normalize_paths(&mut config);
    
    let modified = if path.exists() {
        let old_content = fs::read_to_string(&path)
            .map_err(|e| ConfigError::Io { 
                operation: "update_read".to_string(), 
                path: path.clone(), 
                source: e 
            })?;
        let new_content = toml::to_string_pretty(&config)
            .map_err(|e| ConfigError::Serialize { 
                path: path.clone(), 
                source: e 
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

fn normalize_paths(config: &mut Config) {
    config.paths.video_dir = expand_path_buf(&config.paths.video_dir);
    config.paths.image_dir = expand_path_buf(&config.paths.image_dir);
}

/// D: 删除配置文件
pub fn delete(input: ConfigDeleteInput) -> Result<ConfigDeleteOutput, ConfigError> {
    let path = config_path(input.path);
    let deleted = if path.exists() {
        fs::remove_file(&path)
            .map_err(|e| ConfigError::Io { 
                operation: "delete".to_string(), 
                path: path.clone(), 
                source: e 
            })?;
        true
    } else {
        false
    };
    Ok(ConfigDeleteOutput { path, deleted })
}

fn save_config(path: &PathBuf, config: &Config) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| ConfigError::Io { 
                operation: "create_dir".to_string(), 
                path: parent.to_path_buf(), 
                source: e 
            })?;
    }
    let content = toml::to_string_pretty(config)
        .map_err(|e| ConfigError::Serialize { 
            path: path.clone(), 
            source: e 
        })?;
    fs::write(path, content)
        .map_err(|e| ConfigError::Io { 
            operation: "write".to_string(), 
            path: path.clone(), 
            source: e 
        })?;
    Ok(())
}
