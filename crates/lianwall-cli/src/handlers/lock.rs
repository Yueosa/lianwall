//! 锁定命令处理器
//!
//! - `lock` - 锁定壁纸（从轮换中排除）
//! - `unlock` - 解锁壁纸
//! - `toggle-lock` - 切换锁定状态

use std::path::PathBuf;

use lianwall_core::socket::ErrorCode;

use crate::client::ClientError;
use crate::output::Formatter;

use super::{connect, normalize_path, HandlerError, Result};

/// 将 ClientError 转换为更友好的 lock 相关错误
fn map_lock_error(e: ClientError, path: &PathBuf) -> HandlerError {
    match &e {
        ClientError::DaemonError { code: ErrorCode::NotFound, .. } => {
            HandlerError::Other(format!(
                "Wallpaper not found in any space: {}",
                path.display()
            ))
        }
        _ => HandlerError::Client(e),
    }
}

/// 处理 lock 命令
pub fn handle_lock(fmt: &Formatter, path: PathBuf) -> Result<()> {
    let path = normalize_path(path);

    let mut client = connect()?;
    client.lock(path.clone()).map_err(|e| map_lock_error(e, &path))?;

    let path_str = path.to_string_lossy().into_owned();
    let filename = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path_str.clone());
    if fmt.is_json() {
        println!("{}", serde_json::json!({
            "success": true,
            "path": path_str,
            "filename": filename,
            "locked": true
        }));
        return Ok(());
    }
    fmt.print_success(&format!("{} Locked: {}", fmt.icon_lock(), filename));
    Ok(())
}

/// 处理 unlock 命令
pub fn handle_unlock(fmt: &Formatter, path: PathBuf) -> Result<()> {
    let path = normalize_path(path);

    let mut client = connect()?;
    client.unlock(path.clone()).map_err(|e| map_lock_error(e, &path))?;

    let path_str = path.to_string_lossy().into_owned();
    let filename = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path_str.clone());
    if fmt.is_json() {
        println!("{}", serde_json::json!({
            "success": true,
            "path": path_str,
            "filename": filename,
            "locked": false
        }));
        return Ok(());
    }
    fmt.print_success(&format!("{} Unlocked: {}", fmt.icon_unlock(), filename));
    Ok(())
}

/// 处理 toggle-lock 命令
pub fn handle_toggle_lock(fmt: &Formatter, path: PathBuf) -> Result<()> {
    let path = normalize_path(path);

    let mut client = connect()?;
    client.toggle_lock(path.clone()).map_err(|e| map_lock_error(e, &path))?;

    let path_str = path.to_string_lossy().into_owned();
    let filename = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path_str.clone());
    if fmt.is_json() {
        println!("{}", serde_json::json!({
            "success": true,
            "path": path_str,
            "filename": filename
        }));
        return Ok(());
    }
    fmt.print_success(&format!("Toggled lock: {}", filename));
    Ok(())
}
