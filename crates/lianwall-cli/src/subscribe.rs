//! Subscribe 命令实现
//!
//! 订阅 daemon 事件流，用于调试和监控。
//!
//! # 使用示例
//!
//! ```bash
//! # 订阅所有事件
//! lianwall subscribe
//!
//! # 订阅特定事件
//! lianwall subscribe wallpaper status
//! ```

use std::path::Path;

use lianwall_core::socket::{Event, EventType};

use crate::client::{Client, ClientError};
use crate::output::Formatter;

/// 解析事件类型字符串
pub fn parse_event_types(args: &[String]) -> Vec<EventType> {
    if args.is_empty() || args.iter().any(|s| s.eq_ignore_ascii_case("all")) {
        return vec![EventType::All];
    }

    args.iter()
        .filter_map(|s| match s.to_lowercase().as_str() {
            "wallpaper" | "wp" => Some(EventType::WallpaperChanged),
            "status" | "st" => Some(EventType::StatusChanged),
            "config" | "cfg" => Some(EventType::ConfigChanged),
            "space" | "sp" => Some(EventType::SpaceUpdated),
            "vram" | "gpu" => Some(EventType::VramChanged),
            "time" | "tp" => Some(EventType::TimePointReached),
            "scan" | "progress" => Some(EventType::ScanProgress),
            "error" | "err" => Some(EventType::Error),
            _ => {
                eprintln!("Warning: unknown event type '{}', ignoring", s);
                None
            }
        })
        .collect()
}

/// 运行订阅命令
pub fn run_subscribe(
    fmt: &Formatter,
    socket_path: &Path,
    event_args: Vec<String>,
) -> Result<(), ClientError> {
    let events = parse_event_types(&event_args);

    if events.is_empty() {
        fmt.print_error("No valid event types specified");
        return Ok(());
    }

    fmt.print_info(&format!(
        "Subscribing to events: {:?}",
        events
            .iter()
            .map(|e| format!("{:?}", e))
            .collect::<Vec<_>>()
    ));

    let mut client = Client::connect(socket_path)?;

    // 发送订阅请求
    let session_id = client.subscribe(events, true)?;
    fmt.print_success(&format!("Subscribed (session: {})", session_id));
    fmt.print_info("Waiting for events... (Ctrl+C to exit)");
    println!();

    // 持续接收事件
    loop {
        match client.receive_event() {
            Ok(event) => print_event(fmt, &event),
            Err(ClientError::Io(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                fmt.print_warning("Connection closed by daemon");
                break;
            }
            Err(e) => {
                fmt.print_error(&format!("Error receiving event: {}", e));
                break;
            }
        }
    }

    Ok(())
}

/// 格式化打印事件
fn print_event(fmt: &Formatter, event: &Event) {
    let timestamp = chrono::Local::now().format("%H:%M:%S");

    match event {
        Event::WallpaperChanged {
            path,
            filename,
            mode,
            trigger,
        } => {
            println!(
                "{} {} WallpaperChanged",
                fmt.dim(&format!("[{}]", timestamp)),
                fmt.icon_image()
            );
            println!("    Mode:     {:?}", mode);
            println!("    File:     {}", filename);
            println!("    Path:     {}", path.display());
            println!("    Trigger:  {:?}", trigger);
            println!();
        }

        Event::StatusChanged { changes } => {
            println!(
                "{} 📊 StatusChanged",
                fmt.dim(&format!("[{}]", timestamp))
            );
            for change in changes {
                println!("    {:?}", change);
            }
            println!();
        }

        Event::ConfigChanged {
            key,
            old_value,
            new_value,
        } => {
            println!(
                "{} ⚙️  ConfigChanged",
                fmt.dim(&format!("[{}]", timestamp))
            );
            println!("    Key:      {}", key);
            println!("    Old:      {}", old_value);
            println!("    New:      {}", new_value);
            println!();
        }

        Event::SpaceUpdated {
            mode,
            reason,
            summary,
        } => {
            println!(
                "{} 🔄 SpaceUpdated",
                fmt.dim(&format!("[{}]", timestamp))
            );
            println!("    Mode:      {:?}", mode);
            println!("    Reason:    {:?}", reason);
            println!(
                "    Summary:   total={}, available={}, locked={}, cooldown={}",
                summary.total, summary.available, summary.locked, summary.in_cooldown
            );
            println!();
        }

        Event::VramChanged {
            action,
            used_mb,
            total_mb,
            free_percent,
        } => {
            println!(
                "{} 💾 VramChanged",
                fmt.dim(&format!("[{}]", timestamp))
            );
            println!("    Action:    {:?}", action);
            println!(
                "    Usage:     {}/{} MB ({:.1}% free)",
                used_mb, total_mb, free_percent
            );
            println!();
        }

        Event::TimePointReached { time, next_time } => {
            println!(
                "{} ⏰ TimePointReached",
                fmt.dim(&format!("[{}]", timestamp))
            );
            println!("    Time:      {}", time);
            if let Some(next) = next_time {
                println!("    Next:      {}", next);
            }
            println!();
        }

        Event::Error {
            code,
            message,
            recoverable,
        } => {
            println!("{} ❌ Error", fmt.dim(&format!("[{}]", timestamp)));
            println!("    Code:        {:?}", code);
            println!("    Message:     {}", message);
            println!("    Recoverable: {}", recoverable);
            println!();
        }

        Event::ScanProgress {
            mode,
            dirs_scanned,
            files_found,
            completed,
        } => {
            println!(
                "{} 📂 ScanProgress",
                fmt.dim(&format!("[{}]", timestamp))
            );
            println!("    Mode:      {:?}", mode);
            println!("    Dirs:      {}", dirs_scanned);
            println!("    Files:     {}", files_found);
            println!("    Completed: {}", completed);
            println!();
        }
    }
}
