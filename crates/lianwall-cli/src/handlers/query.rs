//! 状态查询命令处理器
//!
//! - `status` - 显示 daemon 状态
//! - `space` - 显示向量空间信息

use lianwall_core::config::WallMode;
use lianwall_core::socket::SpaceSnapshot;

use crate::output::{format_uptime, Formatter};

use super::{connect, Result};

/// 处理 status 命令
pub fn handle_status(fmt: &Formatter) -> Result<()> {
    if fmt.is_json() {
        return handle_status_json();
    }

    let mut client = connect()?;
    let status = client.status()?;

    print_status(fmt, &status);
    Ok(())
}

fn handle_status_json() -> Result<()> {
    let mut client = connect()?;
    let status = client.status()?;
    println!("{}", serde_json::to_string_pretty(&status).unwrap());
    Ok(())
}

fn print_status(fmt: &Formatter, status: &lianwall_core::socket::StatusInfo) {
    // Header
    let mode_icon = match status.mode {
        WallMode::Video => fmt.icon_video(),
        WallMode::Image => fmt.icon_image(),
    };

    println!(
        "{} lianwall daemon {} (uptime: {})",
        fmt.success(fmt.icon_running()),
        fmt.success("running"),
        format_uptime(status.uptime_secs)
    );
    println!();

    // Mode & Current
    fmt.print_kv("Mode", &format!("{} {:?}", mode_icon, status.mode));

    if let Some(ref path) = status.current {
        let filename = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        fmt.print_kv("Current", &filename);
    } else {
        fmt.print_kv("Current", "(none)");
    }

    fmt.print_kv("Engine", &status.engine);

    // VRAM
    if status.vram_total_mb > 0 {
        let vram_percent =
            100.0 - (status.vram_used_mb as f64 / status.vram_total_mb as f64 * 100.0);
        fmt.print_kv(
            "VRAM",
            &format!(
                "{}/{} MB ({:.0}% free)",
                status.vram_used_mb, status.vram_total_mb, vram_percent
            ),
        );
    }

    // Wallpapers section
    fmt.print_separator("Wallpapers");
    fmt.print_kv("Scanned", &status.scanned_count.to_string());
    fmt.print_kv("Active", &status.total_wallpapers.to_string());
    fmt.print_kv("Locked", &status.locked_count.to_string());
    fmt.print_kv("Available", &status.available_count.to_string());

    // Time schedule section (if any time constraints exist)
    if status.time_points_count > 0 {
        fmt.print_separator("Time Schedule");
        fmt.print_kv("Time Points", &status.time_points_count.to_string());
        if let Some(ref next_tp) = status.next_time_point {
            fmt.print_kv("Next Refresh", next_tp);
        }
    }
}

/// 处理 space 命令
pub fn handle_space(fmt: &Formatter, video: bool, image: bool) -> Result<()> {
    let mut client = connect()?;

    // 确定要查询的模式
    let mode = if video {
        Some(WallMode::Video)
    } else if image {
        Some(WallMode::Image)
    } else {
        None // 当前模式
    };

    let snapshot = client.space(mode)?;

    if fmt.is_json() {
        println!("{}", serde_json::to_string_pretty(&snapshot).unwrap());
    } else {
        print_space(fmt, &snapshot);
    }

    Ok(())
}

fn print_space(fmt: &Formatter, space: &SpaceSnapshot) {
    // Header
    let mode_icon = match space.mode {
        WallMode::Video => fmt.icon_video(),
        WallMode::Image => fmt.icon_image(),
    };

    println!("Vector Space: {} {:?}", mode_icon, space.mode);
    println!();

    // Summary
    let total = space.items.len();
    let locked = space.items.iter().filter(|p| p.locked).count();
    let in_cooldown = space.items.iter().filter(|p| p.in_cooldown).count();
    let available = total - locked - in_cooldown;

    fmt.print_separator("Summary");
    fmt.print_kv("Total", &total.to_string());
    fmt.print_kv("Available", &available.to_string());
    fmt.print_kv("Locked", &locked.to_string());
    fmt.print_kv("In Cooldown", &in_cooldown.to_string());
    fmt.print_kv("Pointer", &format!("{:.1}°", space.pointer_angle.to_degrees()));

    // Wallpaper list
    if !space.items.is_empty() {
        fmt.print_separator("Wallpapers");
        for item in &space.items {
            let status = if item.is_current {
                fmt.highlight("current")
            } else if item.locked {
                fmt.warning("locked")
            } else if item.in_cooldown {
                fmt.dim("cooldown")
            } else {
                fmt.success("available")
            };

            println!(
                "  [{:3}] {:<30} ({:>6.1}°)  {}",
                item.index,
                truncate_filename(&item.filename, 30),
                item.angle.to_degrees(),
                status
            );
        }
    }
}

/// 截断文件名以适应显示宽度
fn truncate_filename(name: &str, max_len: usize) -> String {
    if name.len() <= max_len {
        name.to_string()
    } else {
        format!("{}...", &name[..max_len - 3])
    }
}
