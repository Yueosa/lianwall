//! 配置命令处理器
//!
//! - `config show` - 显示完整配置
//! - `config get` - 获取指定配置项
//! - `config set` - 设置配置项（显示 old → new）
//! - `config reset` - 重置配置为默认值（需确认）

use std::io::{self, Write};

use lianwall_core::config::{get_value, parse_value, set_value};

use crate::commands::ConfigAction;
use crate::output::Formatter;

use super::{HandlerError, Result, connect, is_daemon_running};

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
    if is_daemon_running()? {
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
                    let toml_str = toml::to_string_pretty(&config).map_err(|e| {
                        HandlerError::Other(format!("Failed to serialize config: {}", e))
                    })?;
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
    use lianwall_core::config::{ConfigReadInput, read};

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
    if is_daemon_running()? {
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
    use lianwall_core::config::{ConfigReadInput, read};

    let output = read(ConfigReadInput { path: None })?;
    let config = output.config;

    let value = get_value(&config, key)
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
    if is_daemon_running()? {
        let mut client = connect()?;

        // 先获取旧值
        let old_snapshot = client.config(Some(key.to_string()))?;
        let old_value = old_snapshot.value;

        // 尝试解析为 JSON 值
        let config = client.config(None)?.value;
        let mut config = serde_json::from_value(config)
            .map_err(|e| HandlerError::Other(format!("Invalid daemon config: {e}")))?;
        let parsed = parse_value(&config, key, value).map_err(HandlerError::Other)?;
        let json_value = set_value(&mut config, key, parsed).map_err(HandlerError::Other)?;

        client.set_config(key.to_string(), json_value.clone())?;

        if fmt.is_json() {
            println!(
                "{}",
                serde_json::json!({
                    "success": true,
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
    use lianwall_core::config::{ConfigReadInput, ConfigUpdateInput, read, update};

    let output = read(ConfigReadInput { path: None })?;
    let mut config = output.config;

    // 获取旧值
    let old_value = get_value(&config, key)
        .ok_or_else(|| HandlerError::Other(format!("Unknown config key: {}", key)))?;

    let parsed = parse_value(&config, key, value).map_err(HandlerError::Other)?;
    let new_value = set_value(&mut config, key, parsed).map_err(HandlerError::Other)?;

    update(ConfigUpdateInput {
        path: None,
        config: config.clone(),
    })?;

    if fmt.is_json() {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "key": key,
                "old_value": old_value,
                "new_value": new_value
            })
        );
    } else {
        fmt.print_success(&format!("{}: {} → {}", key, old_value, new_value));
        fmt.print_info("Note: Daemon is not running, changes saved to file only");
    }

    Ok(())
}

fn handle_config_reset(fmt: &Formatter) -> Result<()> {
    use lianwall_core::config::{
        ConfigCreateInput, ConfigDeleteInput, ConfigReadInput, create, delete, read,
    };

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

    let output = read(ConfigReadInput { path: None })?;
    if fmt.is_json() {
        println!(
            "{}",
            serde_json::json!({
                "success": true,
                "config": output.config
            })
        );
    } else {
        fmt.print_success("Config reset to default");
        if is_daemon_running()? {
            fmt.print_info("Run 'lianwall reload' to apply changes to running daemon");
        }
        println!();
        let toml_str = toml::to_string_pretty(&output.config)
            .map_err(|e| HandlerError::Other(format!("Failed to serialize config: {}", e)))?;
        println!("{}", toml_str);
    }

    Ok(())
}
