mod algorithm;
mod command;
mod config;
mod manager;
mod paperengine;
mod vram;

use std::thread;
use std::time::{Duration, Instant};

use command::{Cli, Commands};
use config::{Config, WallpaperMode};
use manager::WallManager;
use vram::{get_vram_info, is_vram_low, is_vram_recovered};

fn parse_mode(mode_str: &str) -> WallpaperMode {
    match mode_str.to_lowercase().as_str() {
        "picture" | "image" | "static" => WallpaperMode::Image,
        _ => WallpaperMode::Video,
    }
}

/// 显存监控状态
struct VramMonitorState {
    /// 是否因显存不足而降级到静态壁纸
    degraded: bool,
    /// 上次检查时间
    last_check: Instant,
}

impl Default for VramMonitorState {
    fn default() -> Self {
        Self {
            degraded: false,
            last_check: Instant::now(),
        }
    }
}

fn main() {
    let cli = Cli::parse_args();
    let config = Config::load();

    match cli.command {
        Commands::Daemon => {
            run_daemon(config);
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
            let _ = std::process::Command::new("swww").arg("kill").status();

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
                    let _ = std::process::Command::new("pkill").arg("mpvpaper").status();
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
            let _ = std::process::Command::new("pkill").arg("mpvpaper").status();

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

/// 运行守护进程（带显存监控）
fn run_daemon(config: Config) {
    let mut video_manager = WallManager::new(config.clone(), WallpaperMode::Video);
    let mut image_manager: Option<WallManager> = None;

    let video_interval = config.interval(WallpaperMode::Video);
    let vram_config = &config.vram;

    let mut vram_state = VramMonitorState::default();
    let mut last_switch = Instant::now();

    // 初始模式
    let mut current_mode = WallpaperMode::Video;
    Config::save_current_mode(current_mode);

    println!("🎬 LianWall 守护进程启动");
    println!("引擎: {}", video_manager.engine.name());
    println!("切换间隔: {}秒", video_interval);
    println!("壁纸数量: {}", video_manager.wallpapers.len());

    if vram_config.enabled {
        println!("显存监控: 已启用");
        println!("  - 降级阈值: 剩余 < {}%", vram_config.threshold_percent);
        println!("  - 恢复阈值: 剩余 > {}%", vram_config.recovery_percent);
        println!("  - 检测间隔: {}秒", vram_config.check_interval);

        // 打印当前显存状态
        if let Some(info) = get_vram_info() {
            println!(
                "  - 当前状态: {}/{} MB ({:.1}% 使用, {:.1}% 剩余)",
                info.used_mb, info.total_mb, info.usage_percent, info.free_percent
            );
        } else {
            println!("  ⚠️ 无法获取显存信息（可能不支持该显卡）");
        }
    } else {
        println!("显存监控: 已禁用");
    }
    println!("---");

    // 立即播放第一个壁纸
    match video_manager.next() {
        Ok(_) => {}
        Err(e) => eprintln!("初始壁纸切换失败: {}", e),
    }

    loop {
        thread::sleep(Duration::from_secs(1));

        // 显存监控检查
        if vram_config.enabled
            && vram_state.last_check.elapsed() >= Duration::from_secs(vram_config.check_interval)
        {
            vram_state.last_check = Instant::now();

            if !vram_state.degraded {
                // 当前是视频模式，检查是否需要降级
                if is_vram_low(vram_config.threshold_percent) {
                    println!("⚠️ 显存紧张！自动切换到静态壁纸模式");

                    // 初始化图片管理器（懒加载）
                    if image_manager.is_none() {
                        image_manager =
                            Some(WallManager::new(config.clone(), WallpaperMode::Image));
                    }

                    // 切换到图片模式
                    if let Some(ref mut img_mgr) = image_manager {
                        // 先设置静态壁纸
                        if let Err(e) = img_mgr.next() {
                            eprintln!("切换静态壁纸失败: {}", e);
                        } else {
                            // 等待 swww 渲染
                            thread::sleep(Duration::from_millis(500));
                            // 停止 mpvpaper
                            let _ = std::process::Command::new("pkill").arg("mpvpaper").status();

                            current_mode = WallpaperMode::Image;
                            Config::save_current_mode(current_mode);
                            vram_state.degraded = true;
                            last_switch = Instant::now();

                            if let Some(info) = get_vram_info() {
                                println!("  显存: {:.1}% 剩余 → 已降级", info.free_percent);
                            }
                        }
                    }
                }
            } else {
                // 当前是降级模式（图片），检查是否可以恢复
                if is_vram_recovered(vram_config.recovery_percent) {
                    println!("✅ 显存已恢复，切换回动态壁纸模式");

                    // 停止 swww
                    let _ = std::process::Command::new("swww")
                        .arg("kill")
                        .stderr(std::process::Stdio::null())
                        .status();

                    // 恢复视频模式
                    if let Err(e) = video_manager.next() {
                        eprintln!("恢复动态壁纸失败: {}", e);
                    } else {
                        current_mode = WallpaperMode::Video;
                        Config::save_current_mode(current_mode);
                        vram_state.degraded = false;
                        last_switch = Instant::now();

                        if let Some(info) = get_vram_info() {
                            println!("  显存: {:.1}% 剩余 → 已恢复", info.free_percent);
                        }
                    }
                }
            }
        }

        // 壁纸切换逻辑
        let interval = match current_mode {
            WallpaperMode::Video => config.interval(WallpaperMode::Video),
            WallpaperMode::Image => config.interval(WallpaperMode::Image),
        };

        if last_switch.elapsed() >= Duration::from_secs(interval) {
            last_switch = Instant::now();

            match current_mode {
                WallpaperMode::Video => {
                    if let Err(e) = video_manager.next() {
                        eprintln!("切换动态壁纸失败: {}", e);
                    }
                }
                WallpaperMode::Image => {
                    if let Some(ref mut img_mgr) = image_manager {
                        if let Err(e) = img_mgr.next() {
                            eprintln!("切换静态壁纸失败: {}", e);
                        }
                    }
                }
            }
        }
    }
}
