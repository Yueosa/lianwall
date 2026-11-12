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

use lianwall_core::socket::{Event, EventType, Response};

use crate::client::{Client, ClientError};
use crate::output::Formatter;

/// 解析事件类型字符串
pub fn parse_event_types(args: &[String]) -> Result<Vec<EventType>, ClientError> {
    let mut events = Vec::new();
    for argument in args {
        let event = match argument.to_ascii_lowercase().as_str() {
            "all" => EventType::All,
            "wallpaper" | "wp" | "wallpaper_changed" => EventType::WallpaperChanged,
            "status" | "st" | "status_changed" => EventType::StatusChanged,
            "config" | "cfg" | "config_changed" => EventType::ConfigChanged,
            "space" | "sp" | "space_updated" => EventType::SpaceUpdated,
            "vram" | "gpu" | "vram_changed" => EventType::VramChanged,
            "time" | "tp" | "time_point_reached" => EventType::TimePointReached,
            "scan" | "progress" | "scan_progress" => EventType::ScanProgress,
            "error" | "err" => EventType::Error,
            _ => {
                return Err(ClientError::Codec(format!(
                    "Unknown event type: {argument}"
                )));
            }
        };
        if !events.contains(&event) {
            events.push(event);
        }
    }
    if events.is_empty() || events.contains(&EventType::All) {
        return Ok(vec![EventType::All]);
    }
    Ok(events)
}

fn print_json(response: &Response) -> Result<(), ClientError> {
    use std::io::Write;
    let stdout = std::io::stdout();
    let mut writer = stdout.lock();
    serde_json::to_writer(&mut writer, response).map_err(|e| ClientError::Codec(e.to_string()))?;
    writeln!(writer)?;
    writer.flush()?;
    Ok(())
}

/// NDJSON contains the existing protocol's Subscribed, Status and Event frames.
/// Subscription setup remains bounded; once synchronized an idle stream may wait indefinitely.
pub fn run_subscribe(
    fmt: &Formatter,
    socket_path: &Path,
    event_args: Vec<String>,
) -> Result<(), ClientError> {
    use std::time::{Duration, Instant};
    let events = parse_event_types(&event_args)?;
    let mut client = Client::connect(socket_path)?;
    let session_id = client.subscribe(events.clone(), true)?;
    if fmt.is_json() {
        print_json(&Response::Subscribed {
            session_id,
            subscribed_events: EventType::expand(&events),
        })?;
    } else {
        fmt.print_success(&format!("Subscribed (session: {session_id})"));
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(ClientError::ConnectionTimeout);
        }
        client.set_read_timeout(Some(remaining))?;
        match client.receive_response()? {
            Response::Status(status) => {
                if fmt.is_json() {
                    print_json(&Response::Status(status))?;
                } else {
                    fmt.print_info(&format!(
                        "Current: {} [{:?}]",
                        status.current_filename.as_deref().unwrap_or("none"),
                        status.mode
                    ));
                }
                break;
            }
            Response::Event(event) => {
                if fmt.is_json() {
                    print_json(&Response::Event(event))?;
                } else {
                    print_event(fmt, &event);
                }
            }
            Response::Error { code, message } => {
                return Err(ClientError::DaemonError { code, message });
            }
            other => return Err(ClientError::UnexpectedResponse(format!("{other:?}"))),
        }
    }
    client.set_read_timeout(None)?;
    if !fmt.is_json() {
        fmt.print_info("Waiting for events... (Ctrl+C to exit)");
    }
    loop {
        match client.receive_response()? {
            Response::Event(event) => {
                if fmt.is_json() {
                    print_json(&Response::Event(event))?;
                } else {
                    print_event(fmt, &event);
                }
            }
            Response::Status(status) => {
                if fmt.is_json() {
                    print_json(&Response::Status(status))?;
                }
            }
            Response::Error { code, message } => {
                return Err(ClientError::DaemonError { code, message });
            }
            other => return Err(ClientError::UnexpectedResponse(format!("{other:?}"))),
        }
    }
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
            println!("{} 📊 StatusChanged", fmt.dim(&format!("[{}]", timestamp)));
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
            println!("{} ⚙️  ConfigChanged", fmt.dim(&format!("[{}]", timestamp)));
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
            println!("{} 🔄 SpaceUpdated", fmt.dim(&format!("[{}]", timestamp)));
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
            println!("{} 💾 VramChanged", fmt.dim(&format!("[{}]", timestamp)));
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
            println!("{} 📂 ScanProgress", fmt.dim(&format!("[{}]", timestamp)));
            println!("    Mode:      {:?}", mode);
            println!("    Dirs:      {}", dirs_scanned);
            println!("    Files:     {}", files_found);
            println!("    Completed: {}", completed);
            println!();
        }
    }
}
