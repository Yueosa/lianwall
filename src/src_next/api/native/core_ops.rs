use std::path::PathBuf;

use serde_json::json;

use crate::api::native::context::with_context;
use crate::api::native::debug::{self, DebugGuard};
use crate::api::native::error::ApiError;
use crate::api::native::r#struct::*;
use crate::core::config::{read, update, ConfigReadInput, ConfigUpdateInput};
use crate::core::manager::{ManagerError, ManagerStatusOutput, ModeStats};
use crate::core::runtime::RunMode;

/// 启动守护进程
pub fn start(debug: bool) -> Result<ApiResponse<ApiStartOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::start", json!({}));

    let result = with_context(|ctx| {
        ctx.manager
            .start()
            .map_err(|e| ApiError::track(e, "start"))?;

        Ok(ApiStartOutput {
            message: "守护进程已启动".to_string(),
        })
    });

    match result {
        Ok(output) => {
            guard.success(json!({"message": &output.message}));
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

/// 切换下一张壁纸（当前模式）
pub fn next(debug: bool) -> Result<ApiResponse<ApiNextOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::next", json!({}));

    let result = with_context(|ctx| {
        let _step1 = DebugGuard::new("core::get_current_mode", json!({}));
        let mode = ctx.manager.get_status().current_mode;
        _step1.success(json!({"mode": format!("{:?}", mode)}));

        let _step2 = DebugGuard::new("core::select_and_set_wallpaper", json!({"mode": format!("{:?}", mode)}));
        let manager_result = ctx
            .manager
            .next(mode.clone())
            .map_err(|e| ApiError::track(e, "next"))?;
        _step2.success(json!({
            "selected": manager_result.selected_path.display().to_string(),
            "normalized": manager_result.normalized,
            "shuffled": manager_result.shuffled
        }));

        Ok(ApiNextOutput {
            selected_path: manager_result.selected_path,
            mode: manager_result.mode,
            normalized: manager_result.normalized,
            shuffled: manager_result.shuffled,
        })
    });

    match result {
        Ok(output) => {
            guard.success(json!({"selected_path": &output.selected_path}));
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

/// 切换模式（Video ↔ Image）并更新配置文件
pub fn switch_mode(debug: bool) -> Result<ApiResponse<ApiSwitchModeOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::switch_mode", json!({}));

    let result = with_context(|ctx| {
        let _step1 = DebugGuard::new("core::get_current_mode", json!({}));
        let old_mode = ctx.manager.get_status().current_mode;
        let new_mode = match old_mode {
            RunMode::Video => RunMode::Image,
            RunMode::Image => RunMode::Video,
        };
        _step1.success(json!({"old_mode": format!("{:?}", old_mode), "new_mode": format!("{:?}", new_mode)}));

        let _step2 = DebugGuard::new("core::switch_engine", json!({"target": format!("{:?}", new_mode)}));
        let switch_result = match new_mode {
            RunMode::Video => ctx.manager.switch_to_video(),
            RunMode::Image => ctx.manager.switch_to_image(),
        };
        switch_result.map_err(|e| ApiError::track(e, "switch_mode"))?;
        _step2.success(json!({"switched": true}));

        // 更新配置文件中的 paths.mode
        let _step3 = DebugGuard::new("core::update_config", json!({"key": "paths.mode", "value": format!("{:?}", new_mode)}));
        let config_output = read(ConfigReadInput { path: None }).map_err(|e| ApiError::track(ManagerError::Config(e), "switch_mode::read_config"))?;
        let mut config = config_output.config;
        config.paths.mode = format!("{:?}", new_mode);
        update(ConfigUpdateInput { path: None, config }).map_err(|e| ApiError::track(ManagerError::Config(e), "switch_mode::update_config"))?;
        _step3.success(json!({"config_updated": true}));

        let wallpaper = ctx
            .manager
            .get_status()
            .current_wallpaper
            .unwrap_or_default();

        Ok(ApiSwitchModeOutput {
            old_mode,
            new_mode,
            wallpaper,
        })
    });

    match result {
        Ok(output) => {
            guard.success(json!({"new_mode": format!("{:?}", output.new_mode)}));
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

/// 热重载壁纸目录
pub fn reload(mode: Option<RunMode>, debug: bool) -> Result<ApiResponse<ApiReloadOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::reload", json!({"mode": mode}));

    let result = with_context(|ctx| {
        let target_mode = mode.unwrap_or_else(|| ctx.manager.get_status().current_mode);

        let reload_result = ctx
            .manager
            .reload(target_mode)
            .map_err(|e| ApiError::track(e, "reload"))?;

        Ok(ApiReloadOutput {
            total_count: reload_result.total_count,
            active_count: reload_result.active_count,
            new_count: reload_result.new_count,
            removed_count: reload_result.removed_count,
        })
    });

    match result {
        Ok(output) => {
            guard.success(json!({"new": output.new_count, "removed": output.removed_count}));
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

/// 停止守护进程
pub fn stop(debug: bool) -> Result<ApiResponse<ApiStopOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::stop", json!({}));

    let result = with_context(|ctx| {
        ctx.manager
            .stop()
            .map_err(|e| ApiError::track(e, "stop"))?;

        Ok(ApiStopOutput {
            message: "所有引擎已停止".to_string(),
        })
    });

    match result {
        Ok(output) => {
            guard.success(json!({"message": &output.message}));
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

/// 查询状态
pub fn status(debug: bool) -> Result<ApiResponse<ApiStatusOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::status", json!({}));

    let result = with_context(|ctx| {
        let status: ManagerStatusOutput = ctx.manager.get_status();

        let convert_stats = |stats: Option<ModeStats>| {
            stats.map(|s| ModeStatsOutput {
                total_count: s.total_count,
                active_count: s.active_count,
                locked_count: s.locked_count,
                min_value: s.algorithm_stats.min_value,
                max_value: s.algorithm_stats.max_value,
                avg_value: s.algorithm_stats.avg_value,
            })
        };

        Ok(ApiStatusOutput {
            current_mode: status.current_mode,
            current_wallpaper: status.current_wallpaper,
            is_running: status.is_running,
            selection_count: status.selection_count,
            video_stats: convert_stats(status.video_stats),
            image_stats: convert_stats(status.image_stats),
        })
    });

    match result {
        Ok(output) => {
            guard.success(json!({"mode": format!("{:?}", output.current_mode)}));
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

// --- 壁纸管理接口 ---

/// 列出壁纸
pub fn list(mode: Option<RunMode>, debug: bool) -> Result<ApiResponse<ApiListOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::list", json!({"mode": format!("{:?}", mode)}));

    let result = with_context(|ctx| {
        let target_mode = mode.unwrap_or_else(|| ctx.manager.get_status().current_mode);

        let list_result = ctx
            .manager
            .list(target_mode.clone())
            .map_err(|e| ApiError::track(e, "list"))?;

        // 转换为 API 输出结构
        let convert_info = |info: crate::core::manager::WallpaperInfo| ApiWallpaperInfo {
            path: info.path,
            weight: info.weight,
            locked: info.locked,
            skip_streak: info.skip_streak,
            last_played: info.last_played,
        };

        Ok(ApiListOutput {
            mode: list_result.mode,
            active: list_result.active.into_iter().map(convert_info).collect(),
            locked: list_result.locked.into_iter().map(convert_info).collect(),
            inactive: list_result.inactive.into_iter().map(convert_info).collect(),
        })
    });

    match result {
        Ok(output) => {
            guard.success(json!({
                "active": output.active.len(),
                "locked": output.locked.len(),
                "inactive": output.inactive.len()
            }));
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

/// 锁定壁纸
pub fn lock(
    mode: RunMode,
    path: PathBuf,
    debug: bool,
) -> Result<ApiResponse<ApiLockOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new(
        "api::lock",
        json!({"mode": format!("{:?}", mode), "path": path.display().to_string()}),
    );

    let result = with_context(|ctx| {
        let lock_result = ctx
            .manager
            .lock(mode, path.clone())
            .map_err(|e| ApiError::track(e, "lock"))?;

        Ok(ApiLockOutput {
            path: lock_result.path,
            locked: lock_result.locked,
            message: "壁纸已锁定，不再参与轮换".to_string(),
        })
    });

    match result {
        Ok(output) => {
            guard.success(json!({"locked": output.locked}));
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

/// 解锁壁纸
pub fn unlock(
    mode: RunMode,
    path: PathBuf,
    debug: bool,
) -> Result<ApiResponse<ApiLockOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new(
        "api::unlock",
        json!({"mode": format!("{:?}", mode), "path": path.display().to_string()}),
    );

    let result = with_context(|ctx| {
        let unlock_result = ctx
            .manager
            .unlock(mode, path.clone())
            .map_err(|e| ApiError::track(e, "unlock"))?;

        Ok(ApiLockOutput {
            path: unlock_result.path,
            locked: unlock_result.locked,
            message: "壁纸已解锁，重新参与轮换".to_string(),
        })
    });

    match result {
        Ok(output) => {
            guard.success(json!({"locked": output.locked}));
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

/// 获取统计信息
pub fn stats(mode: Option<RunMode>, debug: bool) -> Result<ApiResponse<ApiStatsOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::stats", json!({"mode": format!("{:?}", mode)}));

    let result = with_context(|ctx| {
        let target_mode = mode.unwrap_or_else(|| ctx.manager.get_status().current_mode);

        let stats_result = ctx
            .manager
            .stats(target_mode.clone())
            .map_err(|e| ApiError::track(e, "stats"))?;

        Ok(ApiStatsOutput {
            mode: target_mode,
            total_count: stats_result.total_count,
            active_count: stats_result.active_count,
            locked_count: stats_result.locked_count,
            min_value: stats_result.algorithm_stats.min_value,
            max_value: stats_result.algorithm_stats.max_value,
            avg_value: stats_result.algorithm_stats.avg_value,
            total_skips: stats_result.algorithm_stats.total_skips,
        })
    });

    match result {
        Ok(output) => {
            guard.success(json!({
                "total": output.total_count,
                "active": output.active_count,
                "locked": output.locked_count
            }));
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
