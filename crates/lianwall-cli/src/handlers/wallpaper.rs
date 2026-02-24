//! 壁纸控制命令处理器
//!
//! - `next` - 切换下一张壁纸
//! - `prev` - 切换上一张壁纸
//! - `switch` - 切换模式 (Video ↔ Image)
//! - `set` - 设置指定壁纸
//! - `mode` - 设置指定模式

use std::path::PathBuf;

use lianwall_core::config::WallMode;

use crate::commands::ModeArg;
use crate::output::Formatter;

use super::{connect, normalize_path, HandlerError, Result};

/// 处理 next 命令
pub fn handle_next(fmt: &Formatter) -> Result<()> {
    let mut client = connect()?;
    client.next()?;

    // 查询新壁纸
    let status = client.status()?;
    if fmt.is_json() {
        let current_str = status.current.as_ref().map(|p| p.to_string_lossy().into_owned());
        let filename_str = status.current.as_ref().and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()));
        println!("{}", serde_json::json!({
            "success": true,
            "current": current_str,
            "current_filename": filename_str,
            "mode": format!("{:?}", status.mode)
        }));
        return Ok(());
    }
    if let Some(ref path) = status.current {
        let filename = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        fmt.print_success(&format!("Switched to: {}", filename));
    } else {
        fmt.print_success("Switched to next wallpaper");
    }
    Ok(())
}

/// 处理 prev 命令
pub fn handle_prev(fmt: &Formatter) -> Result<()> {
    let mut client = connect()?;
    client.prev()?;

    // 查询新壁纸
    let status = client.status()?;
    if fmt.is_json() {
        let current_str = status.current.as_ref().map(|p| p.to_string_lossy().into_owned());
        let filename_str = status.current.as_ref().and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()));
        println!("{}", serde_json::json!({
            "success": true,
            "current": current_str,
            "current_filename": filename_str,
            "mode": format!("{:?}", status.mode)
        }));
        return Ok(());
    }
    if let Some(ref path) = status.current {
        let filename = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        fmt.print_success(&format!("Switched to: {} (from history)", filename));
    } else {
        fmt.print_success("Switched to previous wallpaper");
    }
    Ok(())
}

/// 处理 switch 命令 (Video ↔ Image)
pub fn handle_switch(fmt: &Formatter) -> Result<()> {
    let mut client = connect()?;

    // 获取当前模式
    let status = client.status()?;
    let new_mode = match status.mode {
        WallMode::Video => WallMode::Image,
        WallMode::Image => WallMode::Video,
    };

    // 切换
    client.set_mode(new_mode)?;

    // 查询新壁纸
    let status = client.status()?;
    if fmt.is_json() {
        let current_str = status.current.as_ref().map(|p| p.to_string_lossy().into_owned());
        let filename_str = status.current.as_ref().and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()));
        println!("{}", serde_json::json!({
            "success": true,
            "mode": format!("{:?}", new_mode),
            "current": current_str,
            "current_filename": filename_str
        }));
        return Ok(());
    }
    let icon = match new_mode {
        WallMode::Video => fmt.icon_video(),
        WallMode::Image => fmt.icon_image(),
    };
    if let Some(ref path) = status.current {
        let filename = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        fmt.print_success(&format!("Switched to {} {:?} mode, now playing: {}", icon, new_mode, filename));
    } else {
        fmt.print_success(&format!("Switched to {} {:?} mode", icon, new_mode));
    }
    Ok(())
}

/// 处理 set 命令
///
/// # TODO
/// 当前 daemon 实现会根据文件扩展名自动切换 mode。
/// 后续可考虑添加 `--keep-mode` 参数，允许用户选择是否保持当前模式。
pub fn handle_set(fmt: &Formatter, path: PathBuf) -> Result<()> {
    // 规范化路径
    let path = normalize_path(path);

    // 检查文件是否存在
    if !path.exists() {
        return Err(HandlerError::Other(format!(
            "File not found: {}",
            path.display()
        )));
    }

    let mut client = connect()?;
    client.set_wallpaper(path.clone())?;

    if fmt.is_json() {
        let status = client.status()?;
        let path_str = path.to_string_lossy().into_owned();
        let filename_str = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| path_str.clone());
        println!("{}", serde_json::json!({
            "success": true,
            "path": path_str,
            "current_filename": filename_str,
            "mode": format!("{:?}", status.mode)
        }));
        return Ok(());
    }

    let filename = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    fmt.print_success(&format!("Set wallpaper: {}", filename));
    Ok(())
}

/// 处理 mode 命令
pub fn handle_mode(fmt: &Formatter, mode: ModeArg) -> Result<()> {
    let mut client = connect()?;
    let wall_mode: WallMode = mode.into();
    client.set_mode(wall_mode)?;

    // 查询新壁纸
    let status = client.status()?;
    if fmt.is_json() {
        let current_str = status.current.as_ref().map(|p| p.to_string_lossy().into_owned());
        let filename_str = status.current.as_ref().and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()));
        println!("{}", serde_json::json!({
            "success": true,
            "mode": format!("{:?}", wall_mode),
            "current": current_str,
            "current_filename": filename_str
        }));
        return Ok(());
    }
    let icon = match wall_mode {
        WallMode::Video => fmt.icon_video(),
        WallMode::Image => fmt.icon_image(),
    };
    if let Some(ref path) = status.current {
        let filename = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        fmt.print_success(&format!("Set mode to {} {:?}, now playing: {}", icon, wall_mode, filename));
    } else {
        fmt.print_success(&format!("Set mode to {} {:?}", icon, wall_mode));
    }
    Ok(())
}
