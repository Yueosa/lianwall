use colored::Colorize;

use crate::api::{self, ApiResponse};
use crate::cli::commands::{Commands, ConfigAction};
use crate::cli::error::CliError;
use crate::cli::output;
use crate::core::runtime::RunMode;

pub fn handle_command(cmd: Commands) -> Result<(), CliError> {
    match cmd {
        Commands::Start { debug } => handle_start(debug),
        Commands::Next { mode, debug } => handle_next(mode, debug),
        Commands::Stop { debug } => handle_stop(debug),
        Commands::Reload { mode, debug } => handle_reload(mode, debug),
        Commands::Status { json, debug } => handle_status(json, debug),
        Commands::Diagnose { json, debug } => handle_diagnose(json, debug),
        Commands::Config { action } => handle_config(action),
        Commands::Uninstall { purge, yes, debug } => handle_uninstall(purge, yes, debug),
    }
}

fn handle_start(debug: bool) -> Result<(), CliError> {
    api::init()?;

    let response = api::start(debug)?;
    output::success(&response.result.message);

    if let Some(debug_info) = response.debug {
        output::print_debug_trace(&debug_info);
    }

    Ok(())
}

fn handle_next(switch_mode: bool, debug: bool) -> Result<(), CliError> {
    api::init()?;

    if switch_mode {
        let response = api::switch_mode(debug)?;

        output::success(&format!(
            "已切换模式: {:?} → {:?}",
            response.result.old_mode, response.result.new_mode
        ));
        output::kv("当前壁纸", &response.result.wallpaper.display().to_string());

        if let Some(debug_info) = response.debug {
            output::print_debug_trace(&debug_info);
        }
    } else {
        let response = api::next(debug)?;

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

        if let Some(debug_info) = response.debug {
            output::print_debug_trace(&debug_info);
        }
    }

    Ok(())
}

fn handle_stop(debug: bool) -> Result<(), CliError> {
    api::init()?;

    let response = api::stop(debug)?;
    output::success(&response.result.message);

    if let Some(debug_info) = response.debug {
        output::print_debug_trace(&debug_info);
    }

    Ok(())
}

fn handle_reload(mode: Option<String>, debug: bool) -> Result<(), CliError> {
    api::init()?;

    let target_mode = if let Some(mode_str) = mode {
        Some(match mode_str.to_lowercase().as_str() {
            "video" => RunMode::Video,
            "image" => RunMode::Image,
            _ => return Err(CliError::InvalidMode(mode_str)),
        })
    } else {
        None
    };

    let response = api::reload(target_mode, debug)?;

    output::success("热重载完成");
    output::kv("总数", &response.result.total_count.to_string());
    output::kv("活跃", &response.result.active_count.to_string());
    output::kv(
        "封锁",
        &(response.result.total_count - response.result.active_count).to_string(),
    );

    if response.result.new_count > 0 {
        output::info(&format!("➕ 新增 {} 个文件", response.result.new_count));
    }
    if response.result.removed_count > 0 {
        output::warning(&format!("➖ 移除 {} 个文件", response.result.removed_count));
    }

    if let Some(debug_info) = response.debug {
        output::print_debug_trace(&debug_info);
    }

    Ok(())
}

fn handle_status(json: bool, debug: bool) -> Result<(), CliError> {
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
        response.result
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

    if let Some(video_stats) = response.result.video_stats {
        println!();
        output::title("🎬 Video 模式统计");
        print_mode_stats(&video_stats);
    }

    if let Some(image_stats) = response.result.image_stats {
        println!();
        output::title("🖼️  Image 模式统计");
        print_mode_stats(&image_stats);
    }

    if let Some(debug_info) = response.debug {
        output::print_debug_trace(&debug_info);
    }

    Ok(())
}

fn print_mode_stats(stats: &crate::api::native::r#struct::ModeStatsOutput) {
    output::kv(
        "总数",
        &format!(
            "{} (活跃: {}, 封锁: {})",
            stats.total_count, stats.active_count, stats.locked_count
        ),
    );
    output::kv(
        "权重范围",
        &format!("{:.1} ~ {:.1}", stats.min_value, stats.max_value),
    );
    output::kv("平均权重", &format!("{:.2}", stats.avg_value));
}

fn handle_diagnose(json: bool, debug: bool) -> Result<(), CliError> {
    let response = api::diagnose(debug)?;

    if json {
        output::print_json(&response);
        return Ok(());
    }

    output::title("🔍 系统诊断");

    let gpu_status = if response.result.gpu_available {
        format!("{} ({})", "✓".green(), response.result.gpu_type)
    } else {
        "✗ 不可用".red().to_string()
    };
    output::kv("GPU", &gpu_status);

    let mpv_status = if response.result.mpvpaper_available {
        "✓ 已安装".green().to_string()
    } else {
        "✗ 未安装".red().to_string()
    };
    output::kv("mpvpaper", &mpv_status);

    let swww_status = if response.result.swww_available {
        "✓ 已安装".green().to_string()
    } else {
        "✗ 未安装".red().to_string()
    };
    output::kv("swww", &swww_status);

    output::kv("配置文件", &response.result.config_path.display().to_string());

    if let Some(vram_info) = response.result.vram_info {
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

    if let Some(debug_info) = response.debug {
        output::print_debug_trace(&debug_info);
    }

    Ok(())
}

fn handle_config(action: ConfigAction) -> Result<(), CliError> {
    match action {
        ConfigAction::Get { key, debug } => {
            let response = api::config_get(&key, debug)?;

            output::success(&format!("{} = {}", response.result.key, response.result.value));

            if let Some(debug_info) = response.debug {
                output::print_debug_trace(&debug_info);
            }
        }
        ConfigAction::Set { key, value, debug } => {
            let response = api::config_set(&key, &value, debug)?;

            output::success(&format!(
                "已更新配置: {} = {} → {}",
                response.result.key, response.result.old_value, response.result.new_value
            ));

            if let Some(debug_info) = response.debug {
                output::print_debug_trace(&debug_info);
            }
        }
        ConfigAction::Show { debug } => {
            let response = api::config_show(debug)?;

            output::title("⚙️  配置文件");
            println!("{}", response.result.config_toml);

            if let Some(debug_info) = response.debug {
                output::print_debug_trace(&debug_info);
            }
        }
        ConfigAction::Reset { debug } => {
            let response = api::config_reset(debug)?;

            output::success(&response.result.message);

            if let Some(backup_path) = response.result.backup_path {
                output::info(&format!(
                    "备份已保存: {}",
                    backup_path.display()
                ));
            }

            if let Some(debug_info) = response.debug {
                output::print_debug_trace(&debug_info);
            }
        }
    }

    Ok(())
}

fn handle_uninstall(purge: bool, yes: bool, debug: bool) -> Result<(), CliError> {
    // 预先列出要删除的文件
    let mut files_to_remove = Vec::new();

    if purge {
        if let Some(home) = dirs::home_dir() {
            let cache_dir = home.join(".cache/lianwall");
            if cache_dir.exists() {
                files_to_remove.push(cache_dir.display().to_string());
            }
        }

        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("lianwall");
            if config_path.exists() {
                files_to_remove.push(config_path.display().to_string());
            }
        }
    }

    // 显示将要删除的文件
    if purge && !files_to_remove.is_empty() {
        output::print_file_list("📂 以下文件将被删除:", &files_to_remove);
        println!();
    }

    // 确认提示
    if !yes {
        let message = if purge {
            "确定要卸载并删除所有用户数据吗？"
        } else {
            "确定要停止守护进程吗？"
        };

        if !output::confirm(message) {
            output::warning("已取消操作");
            return Err(CliError::UserCancelled);
        }
    }

    // 执行卸载
    let response = api::uninstall(purge, debug)?;

    output::success("卸载完成");

    if !response.result.removed_items.is_empty() {
        output::print_file_list("🗑️  已删除:", &response.result.removed_items);
    }

    println!();
    output::info(&response.result.note);

    if let Some(debug_info) = response.debug {
        output::print_debug_trace(&debug_info);
    }

    Ok(())
}
