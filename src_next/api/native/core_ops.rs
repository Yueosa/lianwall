use serde_json::json;

use crate::api::native::context::with_context;
use crate::api::native::debug::{self, DebugGuard};
use crate::api::native::error::ApiError;
use crate::api::native::r#struct::*;
use crate::core::manager::{ManagerStatusOutput, ModeStats};
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
        let mode = ctx.manager.get_status().current_mode;

        let manager_result = ctx
            .manager
            .next(mode.clone())
            .map_err(|e| ApiError::track(e, "next"))?;

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

/// 切换模式（Video ↔ Image）
pub fn switch_mode(debug: bool) -> Result<ApiResponse<ApiSwitchModeOutput>, ApiError> {
    if debug {
        debug::enable_debug();
    }

    let guard = DebugGuard::new("api::switch_mode", json!({}));

    let result = with_context(|ctx| {
        let old_mode = ctx.manager.get_status().current_mode;
        let new_mode = match old_mode {
            RunMode::Video => RunMode::Image,
            RunMode::Image => RunMode::Video,
        };

        let switch_result = match new_mode {
            RunMode::Video => ctx.manager.switch_to_video(),
            RunMode::Image => ctx.manager.switch_to_image(),
        };

        switch_result.map_err(|e| ApiError::track(e, "switch_mode"))?;

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
