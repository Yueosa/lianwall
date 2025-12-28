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
            let mut manager = WallManager::new(config, WallpaperMode::Video);
            match manager.next() {
                Ok(_) => println!("✅ 动态壁纸切换成功"),
                Err(e) => eprintln!("❌ 切换失败: {}", e),
            }
        }

        Commands::Video => {
            let mut manager = WallManager::new(config, WallpaperMode::Video);
            match manager.next() {
                Ok(_) => println!("🎬 切换到动态壁纸模式"),
                Err(e) => eprintln!("❌ 切换失败: {}", e),
            }
        }

        Commands::Picture => {
            let mut manager = WallManager::new(config, WallpaperMode::Image);
            // 先停止 mpvpaper
            let _ = std::process::Command::new("pkill")
                .arg("mpvpaper")
                .status();
            
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
    }
}

