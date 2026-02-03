use colored::Colorize;
use std::io::{self, Write};

use crate::api::ApiResponse;
use crate::cli::error::CliError;

/// 打印成功消息
pub fn success(message: &str) {
    println!("{} {}", "✓".green().bold(), message);
}

/// 打印错误消息
pub fn error(message: &str) {
    eprintln!("{} {}", "✗".red().bold(), message);
}

/// 打印警告消息
pub fn warning(message: &str) {
    println!("{} {}", "⚠".yellow().bold(), message);
}

/// 打印信息消息
pub fn info(message: &str) {
    println!("{} {}", "ℹ".blue().bold(), message);
}

/// 打印分隔线
pub fn separator() {
    println!("{}", "━".repeat(60).bright_black());
}

/// 打印标题
pub fn title(text: &str) {
    separator();
    println!("  {}", text.bold().cyan());
    separator();
}

/// 打印键值对
pub fn kv(key: &str, value: &str) {
    println!("  {}: {}", key.bright_black(), value);
}

/// 打印错误链
pub fn print_error_chain(err: &CliError) {
    error(&format!("{}", err));

    // 打印建议
    if let Some(hint) = get_error_hint(err) {
        println!();
        info(&format!("💡 {}", hint));
    }
}

/// 获取错误提示
fn get_error_hint(err: &CliError) -> Option<String> {
    match err {
        CliError::Api(api_err) => {
            let err_str = api_err.to_string();
            if err_str.contains("配置") || err_str.contains("config") {
                Some("运行 `lianwall config show` 检查配置是否正确".to_string())
            } else if err_str.contains("显存") || err_str.contains("VRAM") || err_str.contains("GPU") {
                Some("运行 `lianwall diagnose` 检查 GPU 状态".to_string())
            } else if err_str.contains("壁纸") || err_str.contains("wallpaper") || err_str.contains("目录") {
                Some("检查壁纸目录是否存在，或运行 `lianwall diagnose`".to_string())
            } else if err_str.contains("引擎") || err_str.contains("mpvpaper") || err_str.contains("swww") {
                Some("确保已安装 mpvpaper 或 swww，运行 `lianwall diagnose` 检查".to_string())
            } else {
                None
            }
        }
        CliError::UserCancelled => Some("操作已取消，没有进行任何更改".to_string()),
        CliError::InvalidFilter(_) => Some("有效值: all, active, locked".to_string()),
        CliError::InvalidPath(_) => Some("请检查文件路径是否正确".to_string()),
        _ => None,
    }
}

/// 打印 JSON 响应
pub fn print_json<T: serde::Serialize>(response: &ApiResponse<T>) {
    let json = serde_json::to_string_pretty(response).unwrap();
    println!("{}", json);
}

/// 确认提示（返回 true 表示用户确认）
pub fn confirm(message: &str) -> bool {
    print!("{} {} [y/N]: ", "?".yellow().bold(), message);
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    let input = input.trim().to_lowercase();
    input == "y" || input == "yes"
}

/// 打印文件列表
pub fn print_file_list(title_text: &str, files: &[String]) {
    if files.is_empty() {
        return;
    }

    println!();
    println!("{}", title_text.yellow().bold());
    for file in files {
        println!("  {} {}", "•".bright_black(), file.bright_black());
    }
}
