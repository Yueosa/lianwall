//! 生命周期命令处理器
//!
//! - `start` - 启动守护进程
//! - `stop` - 停止守护进程
//! - `restart` - 重启守护进程

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use crate::output::Formatter;

use super::{connect, is_daemon_running, HandlerError, Result};

/// 处理 start 命令
pub fn handle_start(fmt: &Formatter, foreground: bool) -> Result<()> {
    // 检查是否已在运行
    if is_daemon_running() {
        if fmt.is_json() {
            println!("{}", serde_json::json!({"success": true, "already_running": true}));
        } else {
            fmt.print_warning("Daemon is already running");
        }
        return Ok(());
    }

    // 查找 lianwalld 可执行文件
    let daemon_exe = find_daemon_exe()?;

    if foreground {
        // 前台运行：exec 替换当前进程
        fmt.print_info("Starting daemon in foreground mode...");

        let err = exec_daemon(&daemon_exe);
        return Err(HandlerError::Other(format!(
            "Failed to exec daemon: {}",
            err
        )));
    } else {
        // 后台运行：spawn lianwalld
        let child = Command::new(&daemon_exe)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| HandlerError::Other(format!("Failed to start daemon: {}", e)))?;

        // 等待 daemon 就绪
        for _ in 0..10 {
            thread::sleep(Duration::from_millis(200));
            if is_daemon_running() {
                let pid = child.id();
                if fmt.is_json() {
                    println!("{}", serde_json::json!({"success": true, "pid": pid}));
                } else {
                    fmt.print_success(&format!("Daemon started (PID: {})", pid));
                }
                return Ok(());
            }
        }

        return Err(HandlerError::Other(
            "Daemon process started but not responding. Check logs.".to_string(),
        ));
    }
}

/// 查找 lianwalld 可执行文件
fn find_daemon_exe() -> Result<PathBuf> {
    // 1. 同目录下的 lianwalld
    if let Ok(self_exe) = std::env::current_exe() {
        if let Some(parent) = self_exe.parent() {
            let sibling = parent.join("lianwalld");
            if sibling.exists() {
                return Ok(sibling);
            }
        }
    }

    // 2. PATH 中的 lianwalld
    if let Ok(output) = Command::new("which").arg("lianwalld").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }

    Err(HandlerError::Other(
        "lianwalld not found. Please install lianwall-daemon package.".to_string(),
    ))
}

/// 使用 exec 替换当前进程，运行 daemon
#[cfg(unix)]
fn exec_daemon(daemon_exe: &PathBuf) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    Command::new(daemon_exe).exec()
}

#[cfg(not(unix))]
fn exec_daemon(_daemon_exe: &PathBuf) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "exec not supported on this platform",
    )
}

/// 处理 stop 命令
pub fn handle_stop(fmt: &Formatter) -> Result<()> {
    if !is_daemon_running() {
        if fmt.is_json() {
            println!("{}", serde_json::json!({"success": true, "already_stopped": true}));
        } else {
            fmt.print_warning("Daemon is not running");
        }
        return Ok(());
    }

    let mut client = connect()?;
    client.shutdown()?;
    if fmt.is_json() {
        println!("{}", serde_json::json!({"success": true}));
    } else {
        fmt.print_success("Daemon stopped");
    }
    Ok(())
}

/// 处理 restart 命令
pub fn handle_restart(fmt: &Formatter) -> Result<()> {
    // 停止（如果在运行）
    if is_daemon_running() {
        if !fmt.is_json() {
            fmt.print_info("Stopping daemon...");
        }
        let mut client = connect()?;
        client.shutdown()?;

        // 等待完全停止
        for _ in 0..10 {
            thread::sleep(Duration::from_millis(200));
            if !is_daemon_running() {
                break;
            }
        }
    }

    // 启动
    if !fmt.is_json() {
        fmt.print_info("Starting daemon...");
    }
    handle_start(fmt, false)
}
