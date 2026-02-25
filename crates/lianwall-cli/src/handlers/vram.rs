//! VRAM 手动覆盖命令处理器
//!
//! - `vram downgrade` - 强制降级到 Image 模式
//! - `vram upgrade`   - 强制恢复到 Video 模式
//! - `vram reset`     - 清除覆盖，恢复自动检测
//! - `vram status`    - 显示当前 VRAM 状态

use lianwall_core::socket::VramOverrideAction;

use crate::commands::VramAction;
use crate::output::Formatter;

use super::{connect, Result};

/// 处理 vram 子命令
pub fn handle_vram(fmt: &Formatter, action: VramAction) -> Result<()> {
    match action {
        VramAction::Downgrade => {
            let mut client = connect()?;
            client.vram_override(VramOverrideAction::Downgrade)?;
            if fmt.is_json() {
                println!("{{\"success\":true,\"action\":\"downgrade\"}}");
            } else {
                fmt.print_success("已强制降级到 Image 模式（VRAM 手动覆盖）");
            }
        }
        VramAction::Upgrade => {
            let mut client = connect()?;
            client.vram_override(VramOverrideAction::Upgrade)?;
            if fmt.is_json() {
                println!("{{\"success\":true,\"action\":\"upgrade\"}}");
            } else {
                fmt.print_success("已强制恢复到 Video 模式（VRAM 手动覆盖）");
            }
        }
        VramAction::Reset => {
            let mut client = connect()?;
            client.vram_override(VramOverrideAction::Reset)?;
            if fmt.is_json() {
                println!("{{\"success\":true,\"action\":\"reset\"}}");
            } else {
                fmt.print_success("已清除 VRAM 手动覆盖，恢复自动检测");
            }
        }
        VramAction::Status => {
            let mut client = connect()?;
            let status = client.status()?;
            if fmt.is_json() {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "vram_used_mb": status.vram_used_mb,
                        "vram_total_mb": status.vram_total_mb,
                        "vram_degraded": status.vram_degraded,
                        "vram_override": status.vram_override,
                    }))
                    .unwrap()
                );
            } else {
                fmt.print_separator("VRAM Status");
                if status.vram_total_mb > 0 {
                    let free_pct = 100.0
                        - (status.vram_used_mb as f64 / status.vram_total_mb as f64 * 100.0);
                    fmt.print_kv(
                        "Usage",
                        &format!(
                            "{}/{} MB ({:.0}% free)",
                            status.vram_used_mb, status.vram_total_mb, free_pct
                        ),
                    );
                } else {
                    fmt.print_kv("Usage", "N/A");
                }
                let degraded_str = if status.vram_degraded {
                    "⚠️  Degraded (Image mode)"
                } else {
                    "Normal (Video mode allowed)"
                };
                fmt.print_kv("Auto Status", degraded_str);
                match status.vram_override {
                    Some(true) => fmt.print_kv("Override", "⬇ Forced Downgrade (Image)"),
                    Some(false) => fmt.print_kv("Override", "⬆ Forced Upgrade (Video)"),
                    None => fmt.print_kv("Override", "None (auto)"),
                }
            }
        }
    }
    Ok(())
}
