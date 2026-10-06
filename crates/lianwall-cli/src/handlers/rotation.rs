//! 轮换暂停/恢复命令处理器
//!
//! - `rotation pause`  - 暂停当前模式的自动轮换（仅内存，daemon 重启恢复）
//! - `rotation resume` - 恢复当前模式的自动轮换
//! - `rotation status` - 显示轮换暂停状态

use crate::commands::RotationAction;
use crate::output::Formatter;

use super::{connect, Result};

/// 处理 rotation 子命令
pub fn handle_rotation(fmt: &Formatter, action: RotationAction) -> Result<()> {
    match action {
        RotationAction::Pause => {
            let mut client = connect()?;
            client.pause_rotation()?;
            if fmt.is_json() {
                println!("{{\"success\":true,\"action\":\"pause\"}}");
            } else {
                fmt.print_success("已暂停当前模式的自动轮换（daemon 重启后恢复）");
            }
        }
        RotationAction::Resume => {
            let mut client = connect()?;
            client.resume_rotation()?;
            if fmt.is_json() {
                println!("{{\"success\":true,\"action\":\"resume\"}}");
            } else {
                fmt.print_success("已恢复当前模式的自动轮换");
            }
        }
        RotationAction::Status => {
            let mut client = connect()?;
            let status = client.rotation_status()?;
            if fmt.is_json() {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "paused": status.paused,
                        "mode": status.mode,
                        "saved_interval": status.saved_interval,
                    }))
                    .unwrap()
                );
            } else {
                fmt.print_separator("Rotation Status");
                if status.paused {
                    fmt.print_kv("State", "⏸️  Paused");
                    if let Some(saved) = status.saved_interval {
                        fmt.print_kv("Saved Interval", &format!("{}s", saved));
                    }
                } else {
                    fmt.print_kv("State", "▶️  Running");
                }
                fmt.print_kv("Mode", &format!("{:?}", status.mode));
            }
        }
    }
    Ok(())
}
