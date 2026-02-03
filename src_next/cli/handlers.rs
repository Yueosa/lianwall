use std::path::PathBuf;

use colored::Colorize;

use crate::api::{self, ApiResponse};
use crate::cli::commands::{Cli, Commands, ConfigAction};
use crate::cli::error::CliError;
use crate::cli::output;
use crate::core::runtime::RunMode;

/// 文件扩展名检测模式
fn detect_mode_from_path(path: &PathBuf) -> Option<RunMode> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    match ext.as_str() {
        "mp4" | "webm" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "m4v" => Some(RunMode::Video),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tiff" | "svg" => Some(RunMode::Image),
        _ => None,
    }
}

pub fn handle_command(cli: Cli) -> Result<(), CliError> {
    let debug = cli.debug;
    let json = cli.json;

    match cli.command {
        Commands::Start => handle_start(debug, json),
        Commands::Stop => handle_stop(debug, json),
        Commands::Next => handle_next(debug, json),
        Commands::Switch => handle_switch(debug, json),
        Commands::Reload => handle_reload(debug, json),
        Commands::Status => handle_status(debug, json),
        Commands::List { filter } => handle_list(filter, debug, json),
        Commands::Lock { path } => handle_lock(path, debug, json),
        Commands::Unlock { path } => handle_unlock(path, debug, json),
        Commands::Stats => handle_stats(debug, json),
        Commands::Diagnose => handle_diagnose(debug, json),
        Commands::Config { action } => handle_config(action, debug, json),
    }
}

fn handle_start(debug: bool, json: bool) -> Result<(), CliError> {
    api::init()?;
    let response = api::start(debug)?;

    if json {
        output::print_json(&response);
    } else {
        output::success(&response.result.message);
        if let Some(debug_info) = &response.debug {
            output::print_debug_trace(debug_info);
        }
    }

    Ok(())
}

fn handle_stop(debug: bool, json: bool) -> Result<(), CliError> {
    api::init()?;
    let response = api::stop(debug)?;

    if json {
        output::print_json(&response);
    } else {
        output::success(&response.result.message);
        if let Some(debug_info) = &response.debug {
            output::print_debug_trace(debug_info);
        }
    }

    Ok(())
}

fn handle_next(debug: bool, json: bool) -> Result<(), CliError> {
    api::init()?;
    let response = api::next(debug)?;

    if json {
        output::print_json(&response);
    } else {
        output::success(&format!(
            "已切换壁纸: {}",
            response.result.selected_path.display()
        ));
        output::kv("模式", &format!("{:?}", response.result.mode));

        if response.result.normalized {
            output::info("📊 触发了权重归一化");
        }
        if response.result.shuffled {
            output::info("🎲 触发了权重洗牌");
        }

        if let Some(debug_info) = &response.debug {
            output::print_debug_trace(debug_info);
        }
    }

    Ok(())
}

fn handle_switch(debug: bool, json: bool) -> Result<(), CliError> {
    api::init()?;
    let response = api::switch_mode(debug)?;

    if json {
        output::print_json(&response);
    } else {
        output::success(&format!(
            "已切换模式: {:?} → {:?}",
            response.result.old_mode, response.result.new_mode
        ));
        output::kv("当前壁纸", &response.result.wallpaper.display().to_string());

        if let Some(debug_info) = &response.debug {
            output::print_debug_trace(debug_info);
        }
    }

    Ok(())
}

fn handle_reload(debug: bool, json: bool) -> Result<(), CliError> {
    api::init()?;
    let response = api::reload(None, debug)?;

    if json {
        output::print_json(&response);
    } else {
        output::success("热重载完成");
        output::kv("总数", &response.result.total_count.to_string());
        output::kv("活跃", &response.result.active_count.to_string());
        output::kv(
            "锁定",
            &(response.result.total_count - response.result.active_count).to_string(),
        );

        if response.result.new_count > 0 {
            output::info(&format!("➕ 新增 {} 个文件", response.result.new_count));
        }
        if response.result.removed_count > 0 {
            output::warning(&format!("➖ 移除 {} 个文件", response.result.removed_count));
        }

        if let Some(debug_info) = &response.debug {
            output::print_debug_trace(debug_info);
        }
    }

    Ok(())
}

fn handle_status(debug: bool, json: bool) -> Result<(), CliError> {
    api::init()?;
    let response = api::status(debug)?;

    if json {
        output::print_json(&response);
        return Ok(());
    }

    output::title("📊 LianWall 状态");

    output::kv("当前模式", &format!("{:?}", response.result.current_mode));
    output::kv(
        "当前壁纸",
        response
            .result
            .current_wallpaper
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "无".to_string())
            .as_str(),
    );
    output::kv(
        "运行状态",
        if response.result.is_running {
            "运行中"
        } else {
            "已停止"
        },
    );
    output::kv("切换次数", &response.result.selection_count.to_string());

    if let Some(video_stats) = &response.result.video_stats {
        println!();
        output::title("🎬 Video 模式统计");
        print_mode_stats(video_stats);
    }

    if let Some(image_stats) = &response.result.image_stats {
        println!();
        output::title("🖼️  Image 模式统计");
        print_mode_stats(image_stats);
    }

    if let Some(debug_info) = &response.debug {
        output::print_debug_trace(debug_info);
    }

    Ok(())
}

fn print_mode_stats(stats: &crate::api::native::r#struct::ModeStatsOutput) {
    output::kv(
        "总数",
        &format!(
            "{} (活跃: {}, 锁定: {})",
            stats.total_count, stats.active_count, stats.locked_count
        ),
    );
    output::kv(
        "权重范围",
        &format!("{:.1} ~ {:.1}", stats.min_value, stats.max_value),
    );
    output::kv("平均权重", &format!("{:.2}", stats.avg_value));
}

fn handle_list(filter: String, debug: bool, json: bool) -> Result<(), CliError> {
    api::init()?;
    let response = api::list(None, debug)?;

    if json {
        output::print_json(&response);
        return Ok(());
    }

    let result = &response.result;

    match filter.to_lowercase().as_str() {
        "all" => {
            output::title(&format!("📋 壁纸列表 ({:?})", result.mode));

            if !result.active.is_empty() {
                println!("\n{}", "活跃:".green().bold());
                for wp in &result.active {
                    println!("  {} (权重: {:.2})", wp.path.display(), wp.weight);
                }
            }

            if !result.locked.is_empty() {
                println!("\n{}", "锁定:".yellow().bold());
                for wp in &result.locked {
                    println!("  🔒 {}", wp.path.display());
                }
            }
        }
        "active" => {
            output::title(&format!("📋 活跃壁纸 ({:?})", result.mode));
            for wp in &result.active {
                println!("  {} (权重: {:.2})", wp.path.display(), wp.weight);
            }
        }
        "locked" => {
            output::title(&format!("🔒 锁定壁纸 ({:?})", result.mode));
            for wp in &result.locked {
                println!("  {}", wp.path.display());
            }
        }
        _ => return Err(CliError::InvalidFilter(filter)),
    }

    if let Some(debug_info) = &response.debug {
        output::print_debug_trace(debug_info);
    }

    Ok(())
}

fn handle_lock(path: PathBuf, debug: bool, json: bool) -> Result<(), CliError> {
    api::init()?;

    // 检查路径存在
    if !path.exists() {
        return Err(CliError::InvalidPath(path.display().to_string()));
    }

    // 自动检测模式
    let mode = detect_mode_from_path(&path).ok_or_else(|| {
        CliError::InvalidPath(format!("无法识别文件类型: {}", path.display()))
    })?;

    let response = api::lock(mode, path, debug)?;

    if json {
        output::print_json(&response);
    } else {
        output::success(&response.result.message);
        output::kv("路径", &response.result.path.display().to_string());

        if let Some(debug_info) = &response.debug {
            output::print_debug_trace(debug_info);
        }
    }

    Ok(())
}

fn handle_unlock(path: PathBuf, debug: bool, json: bool) -> Result<(), CliError> {
    api::init()?;

    // 检查路径存在
    if !path.exists() {
        return Err(CliError::InvalidPath(path.display().to_string()));
    }

    // 自动检测模式
    let mode = detect_mode_from_path(&path).ok_or_else(|| {
        CliError::InvalidPath(format!("无法识别文件类型: {}", path.display()))
    })?;

    let response = api::unlock(mode, path, debug)?;

    if json {
        output::print_json(&response);
    } else {
        output::success(&response.result.message);
        output::kv("路径", &response.result.path.display().to_string());

        if let Some(debug_info) = &response.debug {
            output::print_debug_trace(debug_info);
        }
    }

    Ok(())
}

fn handle_stats(debug: bool, json: bool) -> Result<(), CliError> {
    api::init()?;
    let response = api::stats(None, debug)?;

    if json {
        output::print_json(&response);
        return Ok(());
    }

    output::title(&format!("📈 统计信息 ({:?})", response.result.mode));

    output::kv("总数", &response.result.total_count.to_string());
    output::kv("活跃", &response.result.active_count.to_string());
    output::kv("锁定", &response.result.locked_count.to_string());
    output::kv(
        "权重范围",
        &format!("{:.1} ~ {:.1}", response.result.min_value, response.result.max_value),
    );
    output::kv("平均权重", &format!("{:.2}", response.result.avg_value));
    output::kv("总跳过次数", &response.result.total_skips.to_string());

    if let Some(debug_info) = &response.debug {
        output::print_debug_trace(debug_info);
    }

    Ok(())
}

fn handle_diagnose(debug: bool, json: bool) -> Result<(), CliError> {
    api::init()?;
    let response = api::diagnose(debug)?;

    if json {
        output::print_json(&response);
        return Ok(());
    }

    output::title("🔍 系统诊断");

    // GPU
    let gpu_status = if response.result.gpu_available {
        format!("{} ({})", "✓".green(), response.result.gpu_type)
    } else {
        let reason = response.result.gpu_reason.as_deref().unwrap_or("未知原因");
        format!("{} {}", "✗".red(), reason)
    };
    output::kv("GPU", &gpu_status);

    // 引擎
    let mpv_status = if response.result.mpvpaper_installed {
        "✓ 已安装".green().to_string()
    } else {
        "✗ 未安装".red().to_string()
    };
    output::kv("mpvpaper", &mpv_status);

    let swww_status = if response.result.swww_installed {
        "✓ 已安装".green().to_string()
    } else {
        "✗ 未安装".red().to_string()
    };
    output::kv("swww", &swww_status);

    // 配置
    let config_status = if response.result.config_exists {
        "✓ 存在".green().to_string()
    } else {
        "✗ 不存在".red().to_string()
    };
    output::kv("配置文件", &format!("{} ({})", response.result.config_path.display(), config_status));

    // 目录
    println!();
    output::title("📂 壁纸目录");

    let video_status = if response.result.video_dir_exists {
        format!("{} ({} 个文件)", "✓".green(), response.result.video_count)
    } else {
        "✗ 不存在".red().to_string()
    };
    output::kv("Video", &video_status);

    let image_status = if response.result.image_dir_exists {
        format!("{} ({} 个文件)", "✓".green(), response.result.image_count)
    } else {
        "✗ 不存在".red().to_string()
    };
    output::kv("Image", &image_status);

    // VRAM
    if let Some(vram_info) = &response.result.vram_info {
        println!();
        output::title("💾 显存信息");
        output::kv("总容量", &format!("{} MB", vram_info.total_mb));
        output::kv(
            "已使用",
            &format!("{} MB ({:.1}%)", vram_info.used_mb, vram_info.usage_percent),
        );
        output::kv(
            "剩余",
            &format!("{} MB ({:.1}%)", vram_info.free_mb, vram_info.free_percent),
        );
    }

    // 总体状态
    println!();
    if response.result.all_passed {
        output::success("所有检查通过");
    } else {
        output::error("存在问题:");
        for err in &response.result.errors {
            println!("  • {}", err.red());
        }
    }

    if let Some(debug_info) = &response.debug {
        output::print_debug_trace(debug_info);
    }

    Ok(())
}

fn handle_config(action: ConfigAction, debug: bool, json: bool) -> Result<(), CliError> {
    match action {
        ConfigAction::Get { key } => {
            let response = api::config_get(&key, debug)?;

            if json {
                output::print_json(&response);
            } else {
                output::success(&format!("{} = {}", response.result.key, response.result.value));

                if let Some(debug_info) = &response.debug {
                    output::print_debug_trace(debug_info);
                }
            }
        }
        ConfigAction::Set { key, value } => {
            let response = api::config_set(&key, &value, debug)?;

            if json {
                output::print_json(&response);
            } else {
                output::success(&format!(
                    "已更新配置: {} = {} → {}",
                    response.result.key, response.result.old_value, response.result.new_value
                ));

                if let Some(debug_info) = &response.debug {
                    output::print_debug_trace(debug_info);
                }
            }
        }
        ConfigAction::Show => {
            let response = api::config_show(debug)?;

            if json {
                output::print_json(&response);
            } else {
                output::title("⚙️  配置文件");
                println!("{}", response.result.config_toml);

                if let Some(debug_info) = &response.debug {
                    output::print_debug_trace(debug_info);
                }
            }
        }
        ConfigAction::Reset { yes } => {
            if !yes {
                if !output::confirm("确定要重置配置吗？当前配置将被备份。") {
                    output::warning("已取消操作");
                    return Err(CliError::UserCancelled);
                }
            }

            let response = api::config_reset(debug)?;

            if json {
                output::print_json(&response);
            } else {
                output::success(&response.result.message);

                if let Some(backup_path) = &response.result.backup_path {
                    output::info(&format!("备份已保存: {}", backup_path.display()));
                }

                if let Some(debug_info) = &response.debug {
                    output::print_debug_trace(debug_info);
                }
            }
        }
    }

    Ok(())
}
