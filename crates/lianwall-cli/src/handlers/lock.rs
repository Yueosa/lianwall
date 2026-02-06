//! 锁定命令处理器
//!
//! - `lock` - 锁定壁纸（从轮换中排除）
//! - `unlock` - 解锁壁纸

use std::path::PathBuf;

use crate::output::Formatter;

use super::{connect, normalize_path, Result};

/// 处理 lock 命令
pub fn handle_lock(fmt: &Formatter, path: PathBuf) -> Result<()> {
    let path = normalize_path(path);

    let mut client = connect()?;
    client.lock(path.clone())?;

    let filename = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    fmt.print_success(&format!("{} Locked: {}", fmt.icon_lock(), filename));
    Ok(())
}

/// 处理 unlock 命令
pub fn handle_unlock(fmt: &Formatter, path: PathBuf) -> Result<()> {
    let path = normalize_path(path);

    let mut client = connect()?;
    client.unlock(path.clone())?;

    let filename = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    fmt.print_success(&format!("{} Unlocked: {}", fmt.icon_unlock(), filename));
    Ok(())
}

/// 处理 toggle-lock 命令
pub fn handle_toggle_lock(fmt: &Formatter, path: PathBuf) -> Result<()> {
    let path = normalize_path(path);

    let mut client = connect()?;
    client.toggle_lock(path.clone())?;

    let filename = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    fmt.print_success(&format!("Toggled lock: {}", filename));
    Ok(())
}
