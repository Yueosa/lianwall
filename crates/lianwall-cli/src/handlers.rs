//! 命令处理器

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use lianwall_core::config::WallMode;
use lianwall_core::socket::{self, quick, Client, StatusInfo};

use crate::commands::{ConfigAction, ModeArg};
use crate::output::{format_uptime, Formatter};

/// 命令执行结果
pub type Result<T> = std::result::Result<T, HandlerError>;

/// 处理器错误
#[derive(Debug)]
pub enum HandlerError {
    /// Daemon 未运行
    DaemonNotRunning,
    /// Socket 错误
    Socket(socket::SocketError),
    /// 配置错误
    Config(lianwall_core::config::ConfigError),
    /// 其他错误
    Other(String),
}

impl std::fmt::Display for HandlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DaemonNotRunning => write!(f, "Daemon is not running. Start it with: lianwall start"),
            Self::Socket(e) => write!(f, "Socket error: {}", e),
            Self::Config(e) => write!(f, "Config error: {}", e),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

impl From<socket::SocketError> for HandlerError {
    fn from(e: socket::SocketError) -> Self {
        match e {
            socket::SocketError::DaemonNotRunning => Self::DaemonNotRunning,
            _ => Self::Socket(e),
        }
    }
}

impl From<lianwall_core::config::ConfigError> for HandlerError {
    fn from(e: lianwall_core::config::ConfigError) -> Self {
        Self::Config(e)
    }
}

/// 获取 socket 路径
fn get_socket_path() -> PathBuf {
    // 从配置文件读取，如果读取失败则使用默认值
    match lianwall_core::config::read(lianwall_core::config::ConfigReadInput { path: None }) {
        Ok(output) => output.config.daemon.socket_path,
        Err(_) => PathBuf::from("/tmp/lianwall.sock"),
    }
}

/// 检查 daemon 是否在运行
fn is_daemon_running() -> bool {
    quick::is_running(get_socket_path())
}

/// 连接到 daemon
fn connect() -> Result<Client> {
    let path = get_socket_path();
    Ok(Client::connect(&path)?)
}

// ============================================================================
// 生命周期命令
// ============================================================================

/// 处理 start 命令
pub fn handle_start(fmt: &Formatter, foreground: bool) -> Result<()> {
    // 检查是否已在运行
    if is_daemon_running() {
        fmt.print_warning("Daemon is already running");
        return Ok(());
    }

    if foreground {
        // 前台运行：exec 替换当前进程，运行 daemon 模式
        fmt.print_info("Starting daemon in foreground mode...");
        
        let err = exec_daemon();
        return Err(HandlerError::Other(format!("Failed to exec daemon: {}", err)));
    } else {
        // 后台运行：spawn 自己 + --daemon
        let self_exe = std::env::current_exe()
            .map_err(|e| HandlerError::Other(format!("Failed to get current exe: {}", e)))?;
        
        let child = Command::new(&self_exe)
            .arg("--daemon")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| HandlerError::Other(format!("Failed to start daemon: {}", e)))?;

        // 等待 daemon 就绪
        thread::sleep(Duration::from_millis(500));

        if is_daemon_running() {
            fmt.print_success(&format!("Daemon started (PID: {})", child.id()));
        } else {
            // 再等一会儿
            thread::sleep(Duration::from_millis(500));
            if is_daemon_running() {
                fmt.print_success(&format!("Daemon started (PID: {})", child.id()));
            } else {
                return Err(HandlerError::Other(
                    "Daemon process started but not responding. Check logs.".to_string(),
                ));
            }
        }
    }

    Ok(())
}

/// 处理 stop 命令
pub fn handle_stop(fmt: &Formatter) -> Result<()> {
    if !is_daemon_running() {
        fmt.print_warning("Daemon is not running");
        return Ok(());
    }

    let mut client = connect()?;
    client.shutdown()?;
    fmt.print_success("Daemon stopped");
    Ok(())
}

/// 处理 restart 命令
pub fn handle_restart(fmt: &Formatter) -> Result<()> {
    // 停止（如果在运行）
    if is_daemon_running() {
        fmt.print_info("Stopping daemon...");
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
    fmt.print_info("Starting daemon...");
    handle_start(fmt, false)
}

/// 使用 exec 替换当前进程，运行 daemon 模式
#[cfg(unix)]
fn exec_daemon() -> std::io::Error {
    use std::os::unix::process::CommandExt;
    let self_exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return e,
    };
    Command::new(self_exe).arg("--daemon").exec()
}

#[cfg(not(unix))]
fn exec_daemon() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Unsupported, "exec not supported on this platform")
}

// ============================================================================
// 状态查询命令
// ============================================================================

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

fn print_status(fmt: &Formatter, status: &StatusInfo) {
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
        let filename = path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        fmt.print_kv("Current", &filename);
    } else {
        fmt.print_kv("Current", "(none)");
    }

    fmt.print_kv("Engine", &status.engine);

    // VRAM
    if status.vram_total_mb > 0 {
        let vram_percent = 100.0 - (status.vram_used_mb as f64 / status.vram_total_mb as f64 * 100.0);
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
    fmt.print_kv("Total", &status.total_wallpapers.to_string());
    fmt.print_kv("Locked", &status.locked_count.to_string());
    fmt.print_kv("Available", &status.available_count.to_string());
}

// ============================================================================
// 壁纸控制命令
// ============================================================================

/// 处理 next 命令
pub fn handle_next(fmt: &Formatter) -> Result<()> {
    let mut client = connect()?;
    client.next()?;
    fmt.print_success("Switched to next wallpaper");
    Ok(())
}

/// 处理 prev 命令
pub fn handle_prev(fmt: &Formatter) -> Result<()> {
    let mut client = connect()?;
    client.previous()?;
    fmt.print_success("Switched to previous wallpaper");
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

    let icon = match new_mode {
        WallMode::Video => fmt.icon_video(),
        WallMode::Image => fmt.icon_image(),
    };
    fmt.print_success(&format!("Switched to {} {:?} mode", icon, new_mode));
    Ok(())
}

/// 处理 set 命令
pub fn handle_set(fmt: &Formatter, path: PathBuf) -> Result<()> {
    // 规范化路径
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    };

    // 检查文件是否存在
    if !path.exists() {
        return Err(HandlerError::Other(format!(
            "File not found: {}",
            path.display()
        )));
    }

    let mut client = connect()?;
    client.set_wallpaper(&path)?;

    let filename = path.file_name()
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

    let icon = match wall_mode {
        WallMode::Video => fmt.icon_video(),
        WallMode::Image => fmt.icon_image(),
    };
    fmt.print_success(&format!("Set mode to {} {:?}", icon, wall_mode));
    Ok(())
}

/// 处理 lock 命令
pub fn handle_lock(fmt: &Formatter, path: PathBuf) -> Result<()> {
    let path = normalize_path(path);

    let mut client = connect()?;
    client.lock(&path)?;

    let filename = path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    fmt.print_success(&format!("{} Locked: {}", fmt.icon_lock(), filename));
    Ok(())
}

/// 处理 unlock 命令
pub fn handle_unlock(fmt: &Formatter, path: PathBuf) -> Result<()> {
    let path = normalize_path(path);

    let mut client = connect()?;
    client.unlock(&path)?;

    let filename = path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    fmt.print_success(&format!("{} Unlocked: {}", fmt.icon_unlock(), filename));
    Ok(())
}

/// 处理 reload 命令
pub fn handle_reload(fmt: &Formatter) -> Result<()> {
    let mut client = connect()?;
    client.reload()?;
    fmt.print_success("Reloaded config and rescanned directories");
    Ok(())
}

// ============================================================================
// 配置命令
// ============================================================================

/// 处理 config 子命令
pub fn handle_config(fmt: &Formatter, action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Show => handle_config_show(fmt),
        ConfigAction::Get { key } => handle_config_get(fmt, &key),
        ConfigAction::Set { key, value } => handle_config_set(fmt, &key, &value),
        ConfigAction::Reset => handle_config_reset(fmt),
    }
}

fn handle_config_show(fmt: &Formatter) -> Result<()> {
    use lianwall_core::config::{read, ConfigReadInput};

    let output = read(ConfigReadInput { path: None })?;

    if fmt.is_json() {
        println!("{}", serde_json::to_string_pretty(&output.config).unwrap());
    } else {
        // 打印为 TOML 格式
        let toml_str = toml::to_string_pretty(&output.config)
            .map_err(|e| HandlerError::Other(format!("Failed to serialize config: {}", e)))?;
        println!("{}", toml_str);
    }

    Ok(())
}

fn handle_config_get(fmt: &Formatter, key: &str) -> Result<()> {
    use lianwall_core::config::{read, ConfigReadInput};

    let output = read(ConfigReadInput { path: None })?;
    let config = output.config;

    // 简单的键值访问（支持嵌套，用 . 分隔）
    let value = get_config_value(&config, key)
        .ok_or_else(|| HandlerError::Other(format!("Unknown config key: {}", key)))?;

    if fmt.is_json() {
        println!("{}", serde_json::json!({ "key": key, "value": value }));
    } else {
        println!("{}", value);
    }

    Ok(())
}

fn handle_config_set(fmt: &Formatter, key: &str, value: &str) -> Result<()> {
    use lianwall_core::config::{read, update, ConfigReadInput, ConfigUpdateInput};

    // 读取当前配置
    let output = read(ConfigReadInput { path: None })?;
    let mut config = output.config;

    // 设置值
    set_config_value(&mut config, key, value)
        .map_err(|e| HandlerError::Other(e))?;

    // 保存
    update(ConfigUpdateInput {
        path: None,
        config: config.clone(),
    })?;

    fmt.print_success(&format!("Set {} = {}", key, value));

    // 如果 daemon 在运行，提示 reload
    if socket::quick::is_running(get_socket_path()) {
        fmt.print_info("Run 'lianwall reload' to apply changes");
    }

    Ok(())
}

fn handle_config_reset(fmt: &Formatter) -> Result<()> {
    use lianwall_core::config::{delete, create, ConfigDeleteInput, ConfigCreateInput};

    // 删除现有配置
    let _ = delete(ConfigDeleteInput { path: None });

    // 创建默认配置
    create(ConfigCreateInput { path: None })?;

    fmt.print_success("Config reset to default");
    Ok(())
}

// ============================================================================
// 辅助函数
// ============================================================================

fn normalize_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    }
}

/// 从 Config 结构获取指定键的值
fn get_config_value(config: &lianwall_core::config::Config, key: &str) -> Option<String> {
    let parts: Vec<&str> = key.split('.').collect();

    match parts.as_slice() {
        // paths
        ["paths", "mode"] => Some(format!("{:?}", config.paths.mode)),
        ["paths", "video_dir"] => Some(config.paths.video_dir.to_string_lossy().to_string()),
        ["paths", "image_dir"] => Some(config.paths.image_dir.to_string_lossy().to_string()),

        // video_engine
        ["video_engine", "interval"] => Some(config.video_engine.interval.to_string()),
        ["video_engine", "display"] => Some(config.video_engine.display.clone()),

        // image_engine
        ["image_engine", "interval"] => Some(config.image_engine.interval.to_string()),
        ["image_engine", "outputs"] => Some(config.image_engine.outputs.clone()),

        // vram
        ["vram", "enabled"] => Some(config.vram.enabled.to_string()),
        ["vram", "threshold_percent"] => Some(config.vram.threshold_percent.to_string()),
        ["vram", "recovery_percent"] => Some(config.vram.recovery_percent.to_string()),
        ["vram", "check_interval"] => Some(config.vram.check_interval.to_string()),
        ["vram", "cooldown_seconds"] => Some(config.vram.cooldown_seconds.to_string()),

        // daemon
        ["daemon", "socket_path"] => Some(config.daemon.socket_path.to_string_lossy().to_string()),
        ["daemon", "pid_path"] => Some(config.daemon.pid_path.to_string_lossy().to_string()),
        ["daemon", "log_level"] => Some(config.daemon.log_level.clone()),

        _ => None,
    }
}

/// 设置 Config 结构的指定键
fn set_config_value(
    config: &mut lianwall_core::config::Config,
    key: &str,
    value: &str,
) -> std::result::Result<(), String> {
    let parts: Vec<&str> = key.split('.').collect();

    match parts.as_slice() {
        // paths
        ["paths", "mode"] => {
            config.paths.mode = match value.to_lowercase().as_str() {
                "video" => WallMode::Video,
                "image" => WallMode::Image,
                _ => return Err(format!("Invalid mode: {}. Use 'Video' or 'Image'", value)),
            };
        }
        ["paths", "video_dir"] => {
            config.paths.video_dir = PathBuf::from(value);
        }
        ["paths", "image_dir"] => {
            config.paths.image_dir = PathBuf::from(value);
        }

        // video_engine
        ["video_engine", "interval"] => {
            config.video_engine.interval = value
                .parse()
                .map_err(|_| format!("Invalid interval: {}", value))?;
        }
        ["video_engine", "display"] => {
            config.video_engine.display = value.to_string();
        }

        // image_engine
        ["image_engine", "interval"] => {
            config.image_engine.interval = value
                .parse()
                .map_err(|_| format!("Invalid interval: {}", value))?;
        }
        ["image_engine", "outputs"] => {
            config.image_engine.outputs = value.to_string();
        }

        // vram
        ["vram", "enabled"] => {
            config.vram.enabled = value
                .parse()
                .map_err(|_| format!("Invalid boolean: {}", value))?;
        }
        ["vram", "threshold_percent"] => {
            config.vram.threshold_percent = value
                .parse()
                .map_err(|_| format!("Invalid percent: {}", value))?;
        }
        ["vram", "recovery_percent"] => {
            config.vram.recovery_percent = value
                .parse()
                .map_err(|_| format!("Invalid percent: {}", value))?;
        }
        ["vram", "check_interval"] => {
            config.vram.check_interval = value
                .parse()
                .map_err(|_| format!("Invalid interval: {}", value))?;
        }
        ["vram", "cooldown_seconds"] => {
            config.vram.cooldown_seconds = value
                .parse()
                .map_err(|_| format!("Invalid seconds: {}", value))?;
        }

        // daemon
        ["daemon", "socket_path"] => {
            config.daemon.socket_path = PathBuf::from(value);
        }
        ["daemon", "pid_path"] => {
            config.daemon.pid_path = PathBuf::from(value);
        }
        ["daemon", "log_level"] => {
            config.daemon.log_level = value.to_string();
        }

        _ => return Err(format!("Unknown config key: {}", key)),
    }

    Ok(())
}
