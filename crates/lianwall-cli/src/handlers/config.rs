//! 配置命令处理器
//!
//! - `config show` - 显示完整配置
//! - `config get` - 获取指定配置项
//! - `config set` - 设置配置项（显示 old → new）
//! - `config reset` - 重置配置为默认值（需确认）

use std::io::{self, Write};

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

        if !fmt.is_json() {
            fmt.print_info("(from daemon)");
        }

        if fmt.is_json() {
            println!("{}", serde_json::to_string_pretty(&snapshot.value).unwrap());
        } else {
            // 将 JSON 值转换为 Config 后以 TOML 格式输出，与离线行为一致
            match serde_json::from_value::<lianwall_core::config::Config>(snapshot.value.clone()) {
                Ok(config) => {
                    let toml_str = toml::to_string_pretty(&config)
                        .map_err(|e| HandlerError::Other(format!("Failed to serialize config: {}", e)))?;
                    println!("{}", toml_str);
                }
                Err(_) => {
                    // 回退：如果反序列化失败，直接输出 JSON
                    println!("{}", serde_json::to_string_pretty(&snapshot.value).unwrap());
                }
            }
        }
        return Ok(());
    }

    // Daemon 未运行，从文件读取
    use lianwall_core::config::{read, ConfigReadInput};

    let output = read(ConfigReadInput { path: None })?;

    if !fmt.is_json() {
        fmt.print_info("(from file)");
    }

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
            println!("{} = {}", key, snapshot.value);
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
        println!("{} = {}", key, value);
    }

    Ok(())
}

fn handle_config_set(fmt: &Formatter, key: &str, value: &str) -> Result<()> {
    // 如果 daemon 运行中，通过 socket 设置（会自动持久化）
    if is_daemon_running() {
        let mut client = connect()?;

        // 先获取旧值
        let old_snapshot = client.config(Some(key.to_string()))?;
        let old_value = old_snapshot.value;

        // 尝试解析为 JSON 值
        let json_value: serde_json::Value = parse_config_value(value);

        client.set_config(key.to_string(), json_value.clone())?;

        if fmt.is_json() {
            println!(
                "{}",
                serde_json::json!({
                    "key": key,
                    "old_value": old_value,
                    "new_value": json_value
                })
            );
        } else {
            fmt.print_success(&format!("{}: {} → {}", key, old_value, json_value));
        }
        return Ok(());
    }

    // Daemon 未运行，直接修改配置文件
    use lianwall_core::config::{read, update, ConfigReadInput, ConfigUpdateInput};

    let output = read(ConfigReadInput { path: None })?;
    let mut config = output.config;

    // 获取旧值
    let old_value = get_config_value(&config, key)
        .ok_or_else(|| HandlerError::Other(format!("Unknown config key: {}", key)))?;

    set_config_value(&mut config, key, value).map_err(HandlerError::Other)?;

    update(ConfigUpdateInput {
        path: None,
        config: config.clone(),
    })?;

    if fmt.is_json() {
        println!(
            "{}",
            serde_json::json!({
                "key": key,
                "old_value": old_value,
                "new_value": value
            })
        );
    } else {
        fmt.print_success(&format!("{}: {} → {}", key, old_value, value));
        fmt.print_info("Note: Daemon is not running, changes saved to file only");
    }

    Ok(())
}

fn handle_config_reset(fmt: &Formatter) -> Result<()> {
    use lianwall_core::config::{create, delete, read, ConfigCreateInput, ConfigDeleteInput, ConfigReadInput};

    // 交互确认
    if !fmt.is_json() {
        print!("Reset config to default? [y/N] ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        if !input.trim().eq_ignore_ascii_case("y") {
            fmt.print_info("Cancelled");
            return Ok(());
        }
    }

    // 删除现有配置
    let _ = delete(ConfigDeleteInput { path: None });

    // 创建默认配置
    create(ConfigCreateInput { path: None })?;

    fmt.print_success("Config reset to default");

    if is_daemon_running() {
        fmt.print_info("Run 'lianwall reload' to apply changes to running daemon");
    }

    // 显示新配置
    if !fmt.is_json() {
        println!();
    }

    let output = read(ConfigReadInput { path: None })?;
    if fmt.is_json() {
        println!("{}", serde_json::to_string_pretty(&output.config).unwrap());
    } else {
        let toml_str = toml::to_string_pretty(&output.config)
            .map_err(|e| HandlerError::Other(format!("Failed to serialize config: {}", e)))?;
        println!("{}", toml_str);
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
        ["video_engine", "mpvpaper_args"] => Some(format!("{:?}", config.video_engine.mpvpaper_args)),
        ["video_engine", "mpv_args"] => Some(format!("{:?}", config.video_engine.mpv_args)),

        // image_engine
        ["image_engine", "interval"] => Some(config.image_engine.interval.to_string()),
        ["image_engine", "outputs"] => Some(config.image_engine.outputs.clone()),
        ["image_engine", "swww_args"] => Some(format!("{:?}", config.image_engine.swww_args)),

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

/// 解析逗号分隔的字符串或 JSON 数组为 Vec<String>
fn parse_string_array(value: &str) -> Vec<String> {
    // 先尝试 JSON 数组
    if let Ok(arr) = serde_json::from_str::<Vec<String>>(value) {
        return arr;
    }
    // 空值
    if value.is_empty() || value == "[]" {
        return Vec::new();
    }
    // 逗号分隔
    value.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
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
            config.paths.video_dir = lianwall_core::config::expand_path(value);
        }
        ["paths", "image_dir"] => {
            config.paths.image_dir = lianwall_core::config::expand_path(value);
        }

        // video_engine
        ["video_engine", "interval"] => {
            let v: u64 = value.parse().map_err(|_| format!("Invalid interval: {}", value))?;
            if v == 0 {
                return Err("Interval must be greater than 0".to_string());
            }
            config.video_engine.interval = v;
        }
        ["video_engine", "display"] => {
            config.video_engine.display = value.to_string();
        }
        ["video_engine", "mpvpaper_args"] => {
            config.video_engine.mpvpaper_args = parse_string_array(value);
        }
        ["video_engine", "mpv_args"] => {
            config.video_engine.mpv_args = parse_string_array(value);
        }

        // image_engine
        ["image_engine", "interval"] => {
            let v: u64 = value.parse().map_err(|_| format!("Invalid interval: {}", value))?;
            if v == 0 {
                return Err("Interval must be greater than 0".to_string());
            }
            config.image_engine.interval = v;
        }
        ["image_engine", "outputs"] => {
            config.image_engine.outputs = value.to_string();
        }
        ["image_engine", "swww_args"] => {
            config.image_engine.swww_args = parse_string_array(value);
        }

        // vram
        ["vram", "enabled"] => {
            config.vram.enabled = value
                .parse()
                .map_err(|_| format!("Invalid boolean: {}", value))?;
        }
        ["vram", "threshold_percent"] => {
            let v: f32 = value.parse().map_err(|_| format!("Invalid percent: {}", value))?;
            if !(0.0..=100.0).contains(&v) {
                return Err(format!("Percent must be between 0 and 100, got {}", v));
            }
            config.vram.threshold_percent = v;
        }
        ["vram", "recovery_percent"] => {
            let v: f32 = value.parse().map_err(|_| format!("Invalid percent: {}", value))?;
            if !(0.0..=100.0).contains(&v) {
                return Err(format!("Percent must be between 0 and 100, got {}", v));
            }
            config.vram.recovery_percent = v;
        }
        ["vram", "check_interval"] => {
            let v: u64 = value.parse().map_err(|_| format!("Invalid interval: {}", value))?;
            if v == 0 {
                return Err("Check interval must be greater than 0".to_string());
            }
            config.vram.check_interval = v;
        }
        ["vram", "cooldown_seconds"] => {
            config.vram.cooldown_seconds = value
                .parse()
                .map_err(|_| format!("Invalid seconds: {}", value))?;
        }

        // daemon
        ["daemon", "socket_path"] => {
            config.daemon.socket_path = lianwall_core::config::expand_path(value);
        }
        ["daemon", "pid_path"] => {
            config.daemon.pid_path = lianwall_core::config::expand_path(value);
        }
        ["daemon", "log_level"] => {
            config.daemon.log_level = value.to_string();
        }

        _ => return Err(format!("Unknown config key: {}", key)),
    }

    Ok(())
}
