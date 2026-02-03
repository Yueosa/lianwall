use std::fs;

use serde_json::json;

use crate::api::native::context::with_context;
use crate::api::native::debug::{self, DebugGuard};
use crate::api::native::error::ApiError;
use crate::api::native::r#struct::*;
use crate::core::gpu::{get_info, VramGetInfoInput};

/// 诊断系统（使用 manager.check_all）
pub fn diagnose(debug: bool) -> Result<ApiResponse<ApiDiagnoseOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::diagnose", json!({}));

    let result = with_context(|ctx| {
        let diagnose_result = ctx.manager.check_all();

        // 获取 VRAM 信息（如果可用）
        let vram_info = if diagnose_result.gpu.vram_available {
            let info_result = get_info(VramGetInfoInput {});
            if info_result.success {
                info_result.info.map(|info| VramInfoOutput {
                    total_mb: info.total_mb,
                    used_mb: info.used_mb,
                    free_mb: info.free_mb,
                    usage_percent: info.usage_percent as f64,
                    free_percent: info.free_percent as f64,
                })
            } else {
                None
            }
        } else {
            None
        };

        Ok(ApiDiagnoseOutput {
            config_path: diagnose_result.config_path,
            config_exists: diagnose_result.config_exists,
            gpu_type: format!("{:?}", diagnose_result.gpu.gpu_type),
            gpu_available: diagnose_result.gpu.vram_available,
            gpu_reason: diagnose_result.gpu.reason,
            mpvpaper_installed: diagnose_result.engines.mpvpaper_installed,
            swww_installed: diagnose_result.engines.swww_installed,
            video_dir_exists: diagnose_result.dirs.video_dir_exists,
            video_count: diagnose_result.dirs.video_count,
            image_dir_exists: diagnose_result.dirs.image_dir_exists,
            image_count: diagnose_result.dirs.image_count,
            all_passed: diagnose_result.all_passed,
            errors: diagnose_result.errors,
            vram_info,
        })
    });

    match result {
        Ok(output) => {
            guard.success(json!({"all_passed": output.all_passed}));
            let debug_info = if debug {
                Some(ApiDebugInfo {
                    total_duration_ms: debug::get_trace()
                        .first()
                        .map(|t| t.duration_ms)
                        .unwrap_or(0),
                    trace: debug::get_trace(),
                })
            } else {
                None
            };
            debug::disable_debug();
            Ok(ApiResponse::success(output, debug_info))
        }
        Err(e) => {
            guard.error(&e.to_string());
            debug::disable_debug();
            Err(e)
        }
    }
}

/// 卸载程序
pub fn uninstall(purge_data: bool, debug: bool) -> Result<ApiResponse<ApiUninstallOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::uninstall", json!({"purge_data": purge_data}));

    let mut removed_items = Vec::new();

    // 1. 停止守护进程（忽略错误）
    let _ = super::core_ops::stop(false);

    // 2. 删除用户数据（如果指定 --purge）
    if purge_data {
        // 删除缓存
        if let Some(home) = dirs::home_dir() {
            let cache_dir = home.join(".cache/lianwall");
            if cache_dir.exists() {
                fs::remove_dir_all(&cache_dir)?;
                removed_items.push(cache_dir.display().to_string());
            }
        }

        // 删除配置
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("lianwall");
            if config_path.exists() {
                fs::remove_dir_all(&config_path)?;
                removed_items.push(config_path.display().to_string());
            }
        }
    }

    let output = ApiUninstallOutput {
        removed_items,
        note: if purge_data {
            "已删除用户数据。请使用包管理器删除二进制文件（如 yay -R lianwall）".to_string()
        } else {
            "仅停止了守护进程。使用 --purge 删除用户数据，使用包管理器删除二进制文件。".to_string()
        },
    };

    guard.success(json!({"purge": purge_data}));

    let debug_info = if debug {
        Some(ApiDebugInfo {
            total_duration_ms: debug::get_trace()
                .first()
                .map(|t| t.duration_ms)
                .unwrap_or(0),
            trace: debug::get_trace(),
        })
    } else {
        None
    };

    debug::disable_debug();

    Ok(ApiResponse::success(output, debug_info))
}
