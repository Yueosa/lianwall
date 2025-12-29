mod algorithm;
mod command;
mod config;
mod manager;
mod paperengine;

use std::thread;
use std::time::Duration;

use command::{Cli, Commands};
use config::{Config, WallpaperMode};
use manager::WallManager;

fn parse_mode(mode_str: &str) -> WallpaperMode {
    match mode_str.to_lowercase().as_str() {
        "picture" | "image" | "static" => WallpaperMode::Image,
        _ => WallpaperMode::Video,
    }
}

fn main() {
    let cli = Cli::parse_args();
    let config = Config::load();

    match cli.command {
        Commands::Daemon => {
            let mut manager = WallManager::new(config.clone(), WallpaperMode::Video);
            let interval = config.interval(WallpaperMode::Video);
            Config::save_current_mode(WallpaperMode::Video);
            
            println!("🎬 LianWall 守护进程启动 (动态壁纸模式)");
            println!("引擎: {}", manager.engine.name());
            println!("间隔: {}秒", interval);
            println!("壁纸数量: {}", manager.wallpapers.len());
            println!("---");

            loop {
                match manager.next() {
                    Ok(_) => {}
                    Err(e) => eprintln!("切换壁纸失败: {}", e),
                }
                thread::sleep(Duration::from_secs(interval));
            }
        }

        Commands::Next => {
            let current_mode = Config::load_current_mode();
            let mut manager = WallManager::new(config, current_mode);
            let mode_desc = match current_mode {
                WallpaperMode::Video => "动态壁纸",
                WallpaperMode::Image => "静态壁纸",
            };
            match manager.next() {
                Ok(_) => println!("✅ {}切换成功", mode_desc),
                Err(e) => eprintln!("❌ 切换失败: {}", e),
            }
        }

        Commands::Video => {
            let _ = std::process::Command::new("swww")
                .arg("kill")
                .status();
            
            let mut manager = WallManager::new(config.clone(), WallpaperMode::Video);
            Config::save_current_mode(WallpaperMode::Video);
            match manager.next() {
                Ok(_) => println!("🎬 切换到动态壁纸模式"),
                Err(e) => eprintln!("❌ 切换失败: {}", e),
            }
        }

        Commands::Picture => {
            let _ = std::process::Command::new("pkill")
                .arg("mpvpaper")
                .status();
            
            let mut manager = WallManager::new(config.clone(), WallpaperMode::Image);
            Config::save_current_mode(WallpaperMode::Image);
            match manager.next() {
                Ok(_) => println!("🖼️ 切换到静态壁纸模式"),
                Err(e) => eprintln!("❌ 切换失败: {}", e),
            }
        }

        Commands::Reset { mode } => {
            let mode = parse_mode(&mode);
            let mut manager = WallManager::new(config, mode);
            manager.reset();
            println!("✅ 热重载完成");
        }

        Commands::Status { mode } => {
            let mode = parse_mode(&mode);
            let manager = WallManager::new(config, mode);
            println!("{}", manager.status());
            println!("{}", manager.list_wallpapers());
        }

        Commands::Kill => {
            // 停止 mpvpaper
            let mpv_result = std::process::Command::new("pkill")
                .arg("mpvpaper")
                .status();
            
            // 停止 swww
            let swww_result = std::process::Command::new("swww")
                .arg("kill")
                .status();
            
            match (mpv_result, swww_result) {
                (Ok(_), Ok(_)) => println!("✅ 已停止所有壁纸引擎"),
                _ => println!("⚠️  尝试停止壁纸引擎（部分可能未运行）"),
            }
        }
    }
}

