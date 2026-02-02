use std::fs;

use serde_json::json;

use crate::api::native::debug::{self, DebugGuard};
use crate::api::native::error::ApiError;
use crate::api::native::r#struct::*;
use crate::core::engine::{detect, EngineDetectInput, EngineType};
use crate::core::gpu::{detect as gpu_detect, get_info, VramDetectInput, VramGetInfoInput};

/// 诊断系统
pub fn diagnose(debug: bool) -> Result<ApiResponse<ApiDiagnoseOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::diagnose", json!({}));

    let gpu_detect_result = gpu_detect(VramDetectInput {});
    let mpvpaper_detect = detect(EngineDetectInput {
        engine_type: EngineType::Mpvpaper,
    });
    let swww_detect = detect(EngineDetectInput {
        engine_type: EngineType::Swww,
    });

    let vram_info = if gpu_detect_result.available {
        let info_result = get_info(VramGetInfoInput {});
        if info_result.success {
            info_result.info.map(|info| VramInfoOutput {
                total_mb: info.total_mb,
                used_mb: info.used_mb,
                free_mb: info.free_mb,
                usage_percent: info.usage_percent,
                free_percent: info.free_percent,
            })
        } else {
            None
        }
    } else {
        None
    };

    let config_path = dirs::config_dir()
        .unwrap_or_default()
        .join("lianwall/config.toml");

    let output = ApiDiagnoseOutput {
        gpu_available: gpu_detect_result.available,
        gpu_type: format!("{:?}", gpu_detect_result.gpu_type),
        mpvpaper_available: mpvpaper_detect.available,
        swww_available: swww_detect.available,
        config_path,
        vram_info,
    };

    guard.success(json!({"gpu": gpu_detect_result.available}));

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
