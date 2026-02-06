//! 目录操作命令处理器
//!
//! - `reload` - 重新加载配置文件并重新扫描壁纸目录
//! - `rescan` - 重新扫描壁纸目录

use std::io::{self, Write};
use std::time::{Duration, Instant};

use lianwall_core::socket::{Event, EventType};

use crate::client::ClientError;
use crate::output::{messages, Formatter};

use super::{connect, Result};

/// 等待事件的超时时间（秒）
const WAIT_TIMEOUT_SECS: u64 = 30;
/// 显示提示信息的延迟（秒）
const HINT_DELAY_SECS: u64 = 10;

/// 处理 reload 命令
///
/// 重新加载配置文件并重新扫描壁纸目录。
///
/// # 与 rescan 的区别
/// - `reload`: 重新读取 config.toml 文件，更新 daemon 的所有配置状态，
///   如果配置中的壁纸目录路径发生变化，也会自动触发重新扫描。
/// - `rescan`: 只重新扫描壁纸目录发现新增/删除的文件，不读取配置文件。
pub fn handle_reload(fmt: &Formatter) -> Result<()> {
    let mut client = connect()?;

    // 先订阅事件
    client.subscribe(vec![EventType::ConfigChanged, EventType::SpaceUpdated], false)?;

    // 发送 reload 命令
    client.reload_config()?;

    if !fmt.is_json() {
        eprint!("{}...", messages::RELOADING);
        io::stderr().flush().unwrap();
    }

    // 等待 ConfigChanged 和 SpaceUpdated 两个事件
    let mut config_changed = false;
    let mut space_updated_info: Option<(usize, usize, usize)> = None; // (total, available, locked)

    let result = wait_for_events(&mut client, fmt, |event| {
        match event {
            Event::ConfigChanged { .. } => {
                config_changed = true;
            }
            Event::SpaceUpdated { summary, .. } => {
                space_updated_info = Some((summary.total, summary.available, summary.locked));
            }
            _ => {}
        }
        // 两个事件都收到才算完成
        config_changed && space_updated_info.is_some()
    });

    // 清除 "Reloading..."
    if !fmt.is_json() {
        eprint!("\r\x1b[K");
    }

    match result {
        Ok(()) => {
            if let Some((total, available, locked)) = space_updated_info {
                if fmt.is_json() {
                    println!(
                        "{}",
                        serde_json::json!({
                            "status": "ok",
                            "total": total,
                            "available": available,
                            "locked": locked
                        })
                    );
                } else {
                    fmt.print_success(&format!(
                        "Reloaded: {} wallpapers ({} available, {} locked)",
                        total, available, locked
                    ));
                }
            } else {
                fmt.print_success("Reloaded config");
            }
        }
        Err(_) => {
            print_timeout_message(fmt);
        }
    }

    Ok(())
}

/// 处理 rescan 命令
///
/// 重新扫描壁纸目录，发现新增/删除的壁纸文件。
///
/// # 使用场景
/// - 在壁纸目录中添加或删除了壁纸文件
/// - 修改了壁纸文件的时间约束目录结构（如 `00-06/`）
///
/// # 与 reload 的区别
/// - `rescan`: 只重新扫描目录，不重新读取配置文件，适合壁纸文件变动的情况
/// - `reload`: 重新读取 config.toml，适合配置文件变动的情况
pub fn handle_rescan(fmt: &Formatter) -> Result<()> {
    let mut client = connect()?;

    // 先订阅事件
    client.subscribe(vec![EventType::SpaceUpdated], false)?;

    // 发送 rescan 命令
    client.rescan()?;

    if !fmt.is_json() {
        eprint!("{}...", messages::RESCANNING);
        io::stderr().flush().unwrap();
    }

    // 等待 SpaceUpdated 事件
    let mut space_info: Option<(usize, usize, usize)> = None;

    let result = wait_for_events(&mut client, fmt, |event| {
        if let Event::SpaceUpdated { summary, .. } = event {
            space_info = Some((summary.total, summary.available, summary.locked));
            return true;
        }
        false
    });

    // 清除 "Rescanning..."
    if !fmt.is_json() {
        eprint!("\r\x1b[K");
    }

    match result {
        Ok(()) => {
            if let Some((total, available, locked)) = space_info {
                if fmt.is_json() {
                    println!(
                        "{}",
                        serde_json::json!({
                            "status": "ok",
                            "total": total,
                            "available": available,
                            "locked": locked
                        })
                    );
                } else {
                    fmt.print_success(&format!(
                        "Rescanned: {} wallpapers ({} available, {} locked)",
                        total, available, locked
                    ));
                }
            }
        }
        Err(_) => {
            print_timeout_message(fmt);
        }
    }

    Ok(())
}

/// 等待事件，带超时和提示
fn wait_for_events<F>(
    client: &mut crate::client::Client,
    fmt: &Formatter,
    mut is_done: F,
) -> std::result::Result<(), ()>
where
    F: FnMut(&Event) -> bool,
{
    let start = Instant::now();
    let mut hint_shown = false;

    // 设置短超时用于轮询（1秒）
    let _ = client.set_read_timeout(Some(Duration::from_secs(1)));

    loop {
        let elapsed = start.elapsed().as_secs();

        // Timeout check
        if elapsed >= WAIT_TIMEOUT_SECS {
            return Err(());
        }

        // Show hint after 10 seconds
        if !hint_shown && elapsed >= HINT_DELAY_SECS && !fmt.is_json() {
            hint_shown = true;
            eprintln!();
            eprintln!("  {}", crate::output::messages::SCAN_HINT_LINE1);
            eprintln!("  {}", crate::output::messages::SCAN_HINT_LINE2);
            eprint!("  {}...", crate::output::messages::WAITING);
            io::stderr().flush().unwrap();
        }

        // Try to receive event
        match client.receive_event() {
            Ok(event) => {
                if is_done(&event) {
                    return Ok(());
                }
            }
            Err(ClientError::Io(ref e)) if e.kind() == io::ErrorKind::WouldBlock => {
                // Timeout, continue waiting
                continue;
            }
            Err(ClientError::Io(ref e)) if e.kind() == io::ErrorKind::TimedOut => {
                // Timeout, continue waiting
                continue;
            }
            Err(_) => {
                // Other error, exit
                return Err(());
            }
        }
    }
}

/// Print timeout message
fn print_timeout_message(fmt: &Formatter) {
    use crate::output::messages;
    
    if fmt.is_json() {
        println!(
            "{}",
            serde_json::json!({
                "status": "timeout",
                "message": messages::SCAN_BACKGROUND_HINT
            })
        );
    } else {
        fmt.print_warning(messages::TIMEOUT_WARNING);
        fmt.print_info(messages::SCAN_BACKGROUND_HINT);
    }
}
