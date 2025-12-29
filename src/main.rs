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
            // 正常逻辑：先杀 mpvpaper，再启动 swww
            // let _ = std::process::Command::new("pkill")
            //     .arg("mpvpaper")
            //     .status();
            // 
            // let mut manager = WallManager::new(config.clone(), WallpaperMode::Image);
            // Config::save_current_mode(WallpaperMode::Image);
            // match manager.next() {
            //     Ok(_) => println!("🖼️ 切换到静态壁纸模式"),
            //     Err(e) => eprintln!("❌ 切换失败: {}", e),
            // }
            
            // 备选逻辑：先启动 swww 并设置壁纸（在 mpvpaper 下面准备好）
            let mut manager = WallManager::new(config.clone(), WallpaperMode::Image);
            Config::save_current_mode(WallpaperMode::Image);
            match manager.next() {
                Ok(_) => {
                    // 等待 swww 完全渲染完成
                    thread::sleep(Duration::from_millis(1000));
                    // swww 准备好后再杀 mpvpaper，实现平滑切换
                    let _ = std::process::Command::new("pkill")
                        .arg("mpvpaper")
                        .status();
                    println!("🖼️ 切换到静态壁纸模式");
                }
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
            let mode = match mode {
                Some(m) => parse_mode(&m),
                None => Config::load_current_mode(),
            };
            let manager = WallManager::new(config, mode);
            println!("{}", manager.status());
            println!("{}", manager.list_wallpapers());
        }

        Commands::Kill => {
            // 停止 mpvpaper
            let _ = std::process::Command::new("pkill")
                .arg("mpvpaper")
                .status();
            
            // 停止 swww（忽略错误，可能未运行）
            let _ = std::process::Command::new("swww")
                .arg("kill")
                .stderr(std::process::Stdio::null())
                .status();
            
            println!("✅ 已停止所有壁纸引擎");
            
            // 杀掉所有 lianwall 进程（包括 daemon 和自己）
            let _ = std::process::Command::new("killall")
                .arg("lianwall")
                .status();
        }
    }
}

