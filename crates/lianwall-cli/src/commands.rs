//! CLI 命令定义
//!
//! 使用 clap 解析命令行参数

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// LianWall - 动态壁纸管理器
#[derive(Parser, Debug)]
#[command(name = "lianwall")]
#[command(version)]
#[command(about = "Selects wallpapers using the golden angle algorithm, sprinkled with Lian's magic")]
#[command(author = "Lian <https://github.com/Yueosa/lianwall>")]
#[command(propagate_version = true)]
pub struct Cli {
    /// Output in JSON format (for scripting)
    #[arg(long, global = true)]
    pub json: bool,

    /// Disable colored output
    #[arg(long, global = true)]
    pub no_color: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    // === 生命周期 ===
    /// Start the daemon
    Start {
        /// Run in foreground (don't daemonize)
        #[arg(short = 'F', long)]
        foreground: bool,
    },

    /// Stop the daemon
    Stop,

    /// Restart the daemon
    Restart,

    // === 状态查询 ===
    /// Show daemon status
    Status,

    /// Show vector space information
    Space {
        /// Show Video mode space
        #[arg(long, conflicts_with = "image")]
        video: bool,

        /// Show Image mode space
        #[arg(long, conflicts_with = "video")]
        image: bool,
    },

    /// Show time schedule information
    Time,

    // === 壁纸控制 ===
    /// Switch to next wallpaper
    Next,

    /// Switch to previous wallpaper
    Prev,

    /// Toggle between Video and Image mode
    Switch,

    /// Set specific wallpaper
    Set {
        /// Path to wallpaper file
        path: PathBuf,
    },

    /// Set wallpaper mode
    Mode {
        /// Mode: video or image
        #[arg(value_parser = parse_mode)]
        mode: ModeArg,
    },

    /// Lock a wallpaper (exclude from rotation)
    Lock {
        /// Path to wallpaper file
        path: PathBuf,
    },

    /// Unlock a wallpaper
    Unlock {
        /// Path to wallpaper file
        path: PathBuf,
    },

    /// Toggle wallpaper lock state
    ToggleLock {
        /// Path to wallpaper file
        path: PathBuf,
    },

    /// Reload config file and rescan wallpaper directories
    ///
    /// Use this after editing config.toml manually.
    /// If you only added/removed wallpaper files, use 'rescan' instead.
    Reload,

    /// Rescan wallpaper directories (without reloading config)
    ///
    /// Use this after adding/removing wallpaper files.
    /// This does NOT re-read config.toml - use 'reload' for that.
    Rescan,

    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Subscribe to daemon events (for debugging)
    ///
    /// Available event types: wallpaper, status, config, space, vram, time, error, all
    Subscribe {
        /// Event types to subscribe (default: all)
        #[arg(default_value = "all")]
        events: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Show full configuration
    Show,

    /// Get a specific config value
    Get {
        /// Config key (e.g., "paths.mode", "vram.enabled")
        key: String,
    },

    /// Set a config value
    Set {
        /// Config key
        key: String,
        /// New value
        value: String,
    },

    /// Reset config to default
    Reset,
}

/// 模式参数（用于 `lianwall mode` 命令）
#[derive(Debug, Clone, Copy)]
pub enum ModeArg {
    Video,
    Image,
}

fn parse_mode(s: &str) -> Result<ModeArg, String> {
    match s.to_lowercase().as_str() {
        "video" | "v" => Ok(ModeArg::Video),
        "image" | "img" | "i" => Ok(ModeArg::Image),
        _ => Err(format!("Invalid mode '{}'. Use 'video' or 'image'", s)),
    }
}

impl From<ModeArg> for lianwall_core::config::WallMode {
    fn from(arg: ModeArg) -> Self {
        match arg {
            ModeArg::Video => Self::Video,
            ModeArg::Image => Self::Image,
        }
    }
}
