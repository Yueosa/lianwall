//! 配置命令处理器
//!
//! - `config show` - 显示完整配置
//! - `config get` - 获取指定配置项
//! - `config set` - 设置配置项
//! - `config reset` - 重置配置为默认值

use std::path::PathBuf;

use lianwall_core::config::WallMode;

use crate::commands::ConfigAction;
use crate::output::Formatter;

use super::{connect, is_daemon_running, HandlerError, Result};

/// 处理 config 子命令
pub fn handle_config(fmt: &Formatter, action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Show => handle_config_show(fmt),
        ConfigAction::Get { key } => handle_config_get(fmt, &key),
        ConfigAction::Set { key, value } => handle_config_set(fmt, &key, &value),
        ConfigAction::Reset => handle_config_reset(fmt),
    }
}

fn handle_config_show(fmt: &Formatter) -> Result<()> {
    // 优先从 daemon 获取配置（如果运行中）
    if is_daemon_running() {
        let mut client = connect()?;
        let snapshot = client.config(None)?;

        if fmt.is_json() {
            println!("{}", serde_json::to_string_pretty(&snapshot.value).unwrap());
        } else {
            // 直接打印 JSON，因为是完整配置
            println!("{}", serde_json::to_string_pretty(&snapshot.value).unwrap());
        }
        return Ok(());
    }

    // Daemon 未运行，从文件读取
    use lianwall_core::config::{read, ConfigReadInput};

    let output = read(ConfigReadInput { path: None })?;

    if fmt.is_json() {
        println!("{}", serde_json::to_string_pretty(&output.config).unwrap());
    } else {
        // 打印为 TOML 格式
        let toml_str = toml::to_string_pretty(&output.config)
            .map_err(|e| HandlerError::Other(format!("Failed to serialize config: {}", e)))?;
        println!("{}", toml_str);
    }

    Ok(())
}

fn handle_config_get(fmt: &Formatter, key: &str) -> Result<()> {
    // 优先从 daemon 获取
    if is_daemon_running() {
        let mut client = connect()?;
        let snapshot = client.config(Some(key.to_string()))?;

        if fmt.is_json() {
            println!(
                "{}",
                serde_json::json!({ "key": key, "value": snapshot.value })
            );
        } else {
            println!("{}", snapshot.value);
        }
        return Ok(());
    }

    // Daemon 未运行，从文件读取
    use lianwall_core::config::{read, ConfigReadInput};

    let output = read(ConfigReadInput { path: None })?;
    let config = output.config;

    let value = get_config_value(&config, key)
        .ok_or_else(|| HandlerError::Other(format!("Unknown config key: {}", key)))?;

    if fmt.is_json() {
        println!("{}", serde_json::json!({ "key": key, "value": value }));
    } else {
        println!("{}", value);
    }

    Ok(())
}

fn handle_config_set(fmt: &Formatter, key: &str, value: &str) -> Result<()> {
    // 如果 daemon 运行中，通过 socket 设置（会自动持久化）
    if is_daemon_running() {
        let mut client = connect()?;

        // 尝试解析为 JSON 值
        let json_value: serde_json::Value = parse_config_value(value);

        client.set_config(key.to_string(), json_value)?;
        fmt.print_success(&format!("Set {} = {}", key, value));
        return Ok(());
    }

    // Daemon 未运行，直接修改配置文件
    use lianwall_core::config::{read, update, ConfigReadInput, ConfigUpdateInput};

    let output = read(ConfigReadInput { path: None })?;
    let mut config = output.config;

    set_config_value(&mut config, key, value).map_err(HandlerError::Other)?;

    update(ConfigUpdateInput {
        path: None,
        config: config.clone(),
    })?;

    fmt.print_success(&format!("Set {} = {}", key, value));
    fmt.print_info("Note: Daemon is not running, changes saved to file only");

    Ok(())
}

fn handle_config_reset(fmt: &Formatter) -> Result<()> {
    use lianwall_core::config::{create, delete, ConfigCreateInput, ConfigDeleteInput};

    // 删除现有配置
    let _ = delete(ConfigDeleteInput { path: None });

    // 创建默认配置
    create(ConfigCreateInput { path: None })?;

    fmt.print_success("Config reset to default");

    if is_daemon_running() {
        fmt.print_info("Run 'lianwall reload' to apply changes to running daemon");
    }

    Ok(())
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 解析配置值为 JSON
fn parse_config_value(value: &str) -> serde_json::Value {
    // 尝试解析为 JSON
    if let Ok(v) = serde_json::from_str(value) {
        return v;
    }

    // 尝试布尔值
    if value.eq_ignore_ascii_case("true") {
        return serde_json::Value::Bool(true);
    }
    if value.eq_ignore_ascii_case("false") {
        return serde_json::Value::Bool(false);
    }

    // 尝试数字
    if let Ok(n) = value.parse::<i64>() {
        return serde_json::Value::Number(n.into());
    }
    if let Ok(n) = value.parse::<f64>() {
        if let Some(num) = serde_json::Number::from_f64(n) {
            return serde_json::Value::Number(num);
        }
    }

    // 默认作为字符串
    serde_json::Value::String(value.to_string())
}

/// 从 Config 结构获取指定键的值
fn get_config_value(config: &lianwall_core::config::Config, key: &str) -> Option<String> {
    let parts: Vec<&str> = key.split('.').collect();

    match parts.as_slice() {
        // paths
        ["paths", "mode"] => Some(format!("{:?}", config.paths.mode)),
        ["paths", "video_dir"] => Some(config.paths.video_dir.to_string_lossy().to_string()),
        ["paths", "image_dir"] => Some(config.paths.image_dir.to_string_lossy().to_string()),

        // video_engine
        ["video_engine", "interval"] => Some(config.video_engine.interval.to_string()),
        ["video_engine", "display"] => Some(config.video_engine.display.clone()),

        // image_engine
        ["image_engine", "interval"] => Some(config.image_engine.interval.to_string()),
        ["image_engine", "outputs"] => Some(config.image_engine.outputs.clone()),

        // vram
        ["vram", "enabled"] => Some(config.vram.enabled.to_string()),
        ["vram", "threshold_percent"] => Some(config.vram.threshold_percent.to_string()),
        ["vram", "recovery_percent"] => Some(config.vram.recovery_percent.to_string()),
        ["vram", "check_interval"] => Some(config.vram.check_interval.to_string()),
        ["vram", "cooldown_seconds"] => Some(config.vram.cooldown_seconds.to_string()),

        // daemon
        ["daemon", "socket_path"] => Some(config.daemon.socket_path.to_string_lossy().to_string()),
        ["daemon", "pid_path"] => Some(config.daemon.pid_path.to_string_lossy().to_string()),
        ["daemon", "log_level"] => Some(config.daemon.log_level.clone()),

        _ => None,
    }
}

/// 设置 Config 结构的指定键
fn set_config_value(
    config: &mut lianwall_core::config::Config,
    key: &str,
    value: &str,
) -> std::result::Result<(), String> {
    let parts: Vec<&str> = key.split('.').collect();

    match parts.as_slice() {
        // paths
        ["paths", "mode"] => {
            config.paths.mode = match value.to_lowercase().as_str() {
                "video" => WallMode::Video,
                "image" => WallMode::Image,
                _ => return Err(format!("Invalid mode: {}. Use 'video' or 'image'", value)),
            };
        }
        ["paths", "video_dir"] => {
            config.paths.video_dir = PathBuf::from(value);
        }
        ["paths", "image_dir"] => {
            config.paths.image_dir = PathBuf::from(value);
        }

        // video_engine
        ["video_engine", "interval"] => {
            config.video_engine.interval = value
                .parse()
                .map_err(|_| format!("Invalid interval: {}", value))?;
        }
        ["video_engine", "display"] => {
            config.video_engine.display = value.to_string();
        }

        // image_engine
        ["image_engine", "interval"] => {
            config.image_engine.interval = value
                .parse()
                .map_err(|_| format!("Invalid interval: {}", value))?;
        }
        ["image_engine", "outputs"] => {
            config.image_engine.outputs = value.to_string();
        }

        // vram
        ["vram", "enabled"] => {
            config.vram.enabled = value
                .parse()
                .map_err(|_| format!("Invalid boolean: {}", value))?;
        }
        ["vram", "threshold_percent"] => {
            config.vram.threshold_percent = value
                .parse()
                .map_err(|_| format!("Invalid percent: {}", value))?;
        }
        ["vram", "recovery_percent"] => {
            config.vram.recovery_percent = value
                .parse()
                .map_err(|_| format!("Invalid percent: {}", value))?;
        }
        ["vram", "check_interval"] => {
            config.vram.check_interval = value
                .parse()
                .map_err(|_| format!("Invalid interval: {}", value))?;
        }
        ["vram", "cooldown_seconds"] => {
            config.vram.cooldown_seconds = value
                .parse()
                .map_err(|_| format!("Invalid seconds: {}", value))?;
        }

        // daemon
        ["daemon", "socket_path"] => {
            config.daemon.socket_path = PathBuf::from(value);
        }
        ["daemon", "pid_path"] => {
            config.daemon.pid_path = PathBuf::from(value);
        }
        ["daemon", "log_level"] => {
            config.daemon.log_level = value.to_string();
        }

        _ => return Err(format!("Unknown config key: {}", key)),
    }

    Ok(())
}
