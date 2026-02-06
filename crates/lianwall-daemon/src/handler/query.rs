//! Query Handler - 只读请求处理
//!
//! 这些请求只读取状态，不修改，可以并发执行

use std::sync::Arc;

use lianwall_core::socket::{
    Request, Response, StatusInfo, ConfigSnapshot, ConfigKeyInfo, ConfigConstraints,
    SpaceSnapshot, WallpaperPoint, TimeScheduleInfo, ModeSchedule, WallpaperTimeSegment,
    TimeRangeInfo, ErrorCode, PROTOCOL_VERSION,
};
use lianwall_core::config::WallMode;
use lianwall_core::wallpaper::{TimePoint, WallpaperSpace};

use crate::state::SharedState;

/// 处理查询请求
pub async fn handle_query(state: &Arc<SharedState>, request: Request) -> Response {
    match request {
        Request::Ping => Response::Pong {
            uptime_secs: state.uptime_secs(),
            protocol_version: PROTOCOL_VERSION,
        },
        
        Request::GetStatus => get_status(state).await,
        
        Request::GetConfig { key } => get_config(state, key).await,
        
        Request::GetSpace { mode } => get_space(state, mode).await,
        
        Request::GetTimeInfo => get_time_info(state).await,
        
        // 其他请求不应该到这里
        _ => Response::error(ErrorCode::InvalidRequest, "Not a query request"),
    }
}

/// 获取状态
async fn get_status(state: &Arc<SharedState>) -> Response {
    let config = state.get_config().await;
    let engine = state.get_engine_state().await;
    let video_space = state.get_video_space().await;
    let image_space = state.get_image_space().await;
    let gpu_snapshot = state.get_gpu_snapshot().await;
    let time_points = state.get_time_points().await;
    
    let (space, mode) = if engine.mode == WallMode::Video {
        (&video_space, WallMode::Video)
    } else {
        (&image_space, WallMode::Image)
    };
    
    let current_filename = engine.current.as_ref().and_then(|p| {
        p.file_name().map(|s| s.to_string_lossy().to_string())
    });
    
    let engine_name = if engine.mpvpaper_running {
        "mpvpaper"
    } else if engine.swww_daemon_running {
        "swww"
    } else {
        "none"
    };
    
    let locked_count = space.items.iter().filter(|w| w.locked).count();
    let available_count = space.items.iter().filter(|w| !w.locked).count();
    
    // 获取 VRAM 信息
    let (vram_used_mb, vram_total_mb, vram_degraded) = match &gpu_snapshot.vram_info {
        Some(info) => (info.used_mb, info.total_mb, gpu_snapshot.degraded),
        None => (0, 0, false),
    };
    
    // 计算下一个时间点
    let now = lianwall_core::wallpaper::TimePoint::now();
    let next_tp = lianwall_core::wallpaper::next_key_point(&now, &time_points);
    let next_time_point = next_tp.map(|tp| format!("{:02}:{:02}", tp.hour, tp.minute));
    let time_points_count = time_points.len();
    
    // 根据当前模式选择正确的切换间隔
    let next_switch_secs = match mode {
        WallMode::Video => config.video_engine.interval,
        WallMode::Image => config.image_engine.interval,
    };
    
    Response::Status(StatusInfo {
        mode,
        current: engine.current.clone(),
        current_filename,
        engine: engine_name.to_string(),
        total_wallpapers: space.items.len(),
        locked_count,
        available_count,
        scanned_count: video_space.items.len() + image_space.items.len(),
        vram_used_mb,
        vram_total_mb,
        vram_degraded,
        uptime_secs: state.uptime_secs(),
        protocol_version: PROTOCOL_VERSION,
        next_time_point,
        time_points_count,
        next_switch_secs: Some(next_switch_secs),
    })
}

/// 获取配置
async fn get_config(state: &Arc<SharedState>, key: Option<String>) -> Response {
    let config = state.get_config().await;
    
    if let Some(key) = key {
        // 获取特定配置项
        let value = match key.as_str() {
            // paths
            "paths.mode" => serde_json::to_value(&config.paths.mode).ok(),
            "paths.video_dir" => serde_json::to_value(&config.paths.video_dir).ok(),
            "paths.image_dir" => serde_json::to_value(&config.paths.image_dir).ok(),
            // video_engine
            "video_engine.interval" => serde_json::to_value(config.video_engine.interval).ok(),
            "video_engine.display" => serde_json::to_value(&config.video_engine.display).ok(),
            "video_engine.mpvpaper_args" => serde_json::to_value(&config.video_engine.mpvpaper_args).ok(),
            "video_engine.mpv_args" => serde_json::to_value(&config.video_engine.mpv_args).ok(),
            // image_engine
            "image_engine.interval" => serde_json::to_value(config.image_engine.interval).ok(),
            "image_engine.outputs" => serde_json::to_value(&config.image_engine.outputs).ok(),
            "image_engine.swww_args" => serde_json::to_value(&config.image_engine.swww_args).ok(),
            // vram
            "vram.enabled" => serde_json::to_value(config.vram.enabled).ok(),
            "vram.threshold_percent" => serde_json::to_value(config.vram.threshold_percent).ok(),
            "vram.recovery_percent" => serde_json::to_value(config.vram.recovery_percent).ok(),
            "vram.check_interval" => serde_json::to_value(config.vram.check_interval).ok(),
            "vram.cooldown_seconds" => serde_json::to_value(config.vram.cooldown_seconds).ok(),
            // daemon
            "daemon.socket_path" => serde_json::to_value(&config.daemon.socket_path).ok(),
            "daemon.pid_path" => serde_json::to_value(&config.daemon.pid_path).ok(),
            "daemon.log_level" => serde_json::to_value(&config.daemon.log_level).ok(),
            // 兼容旧的 key
            "mode" => serde_json::to_value(&config.paths.mode).ok(),
            "vram.threshold" => serde_json::to_value(config.vram.threshold_percent).ok(),
            _ => None,
        };
        
        match value {
            Some(v) => Response::Config(ConfigSnapshot {
                key: Some(key),
                value: v,
                modifiable_keys: None,
            }),
            None => Response::error(ErrorCode::InvalidRequest, format!("Unknown config key: {}", key)),
        }
    } else {
        // 获取所有配置
        match serde_json::to_value(&config) {
            Ok(v) => Response::Config(ConfigSnapshot {
                key: None,
                value: v,
                modifiable_keys: Some(get_modifiable_keys()),
            }),
            Err(e) => Response::error(ErrorCode::InternalError, format!("Serialize error: {}", e)),
        }
    }
}

/// 获取可修改的配置键列表
fn get_modifiable_keys() -> Vec<ConfigKeyInfo> {
    vec![
        // ==================== paths ====================
        ConfigKeyInfo {
            key: "paths.mode".to_string(),
            value_type: "string".to_string(),
            description: "运行模式: Video (动态壁纸) 或 Image (静态壁纸)".to_string(),
            default: serde_json::json!("Video"),
            constraints: Some(ConfigConstraints {
                min: None,
                max: None,
                enum_values: Some(vec![
                    "Video".to_string(),
                    "Image".to_string(),
                ]),
                pattern: None,
            }),
        },
        ConfigKeyInfo {
            key: "paths.video_dir".to_string(),
            value_type: "string".to_string(),
            description: "动态壁纸目录（支持 ~ 展开）".to_string(),
            default: serde_json::json!("~/Videos/lianwall"),
            constraints: None,
        },
        ConfigKeyInfo {
            key: "paths.image_dir".to_string(),
            value_type: "string".to_string(),
            description: "静态壁纸目录（支持 ~ 展开）".to_string(),
            default: serde_json::json!("~/Pictures/lianwall"),
            constraints: None,
        },
        
        // ==================== video_engine ====================
        ConfigKeyInfo {
            key: "video_engine.interval".to_string(),
            value_type: "integer".to_string(),
            description: "动态壁纸切换间隔（秒）".to_string(),
            default: serde_json::json!(600),
            constraints: Some(ConfigConstraints {
                min: Some(serde_json::json!(10)),
                max: Some(serde_json::json!(86400)),
                enum_values: None,
                pattern: None,
            }),
        },
        ConfigKeyInfo {
            key: "video_engine.display".to_string(),
            value_type: "string".to_string(),
            description: "目标显示器，\"*\" 表示所有显示器".to_string(),
            default: serde_json::json!("*"),
            constraints: None,
        },
        ConfigKeyInfo {
            key: "video_engine.mpvpaper_args".to_string(),
            value_type: "array".to_string(),
            description: "透传给 mpvpaper 的参数".to_string(),
            default: serde_json::json!([]),
            constraints: None,
        },
        ConfigKeyInfo {
            key: "video_engine.mpv_args".to_string(),
            value_type: "array".to_string(),
            description: "透传给 mpv 的参数（通过 mpvpaper -o 传递）".to_string(),
            default: serde_json::json!([
                "--no-audio",
                "--loop=inf",
                "--hwdec=auto",
                "--video-zoom=0",
                "--panscan=1.0"
            ]),
            constraints: None,
        },
        
        // ==================== image_engine ====================
        ConfigKeyInfo {
            key: "image_engine.interval".to_string(),
            value_type: "integer".to_string(),
            description: "静态壁纸切换间隔（秒）".to_string(),
            default: serde_json::json!(600),
            constraints: Some(ConfigConstraints {
                min: Some(serde_json::json!(10)),
                max: Some(serde_json::json!(86400)),
                enum_values: None,
                pattern: None,
            }),
        },
        ConfigKeyInfo {
            key: "image_engine.outputs".to_string(),
            value_type: "string".to_string(),
            description: "目标显示器，空字符串表示所有显示器，多个用逗号分隔".to_string(),
            default: serde_json::json!(""),
            constraints: None,
        },
        ConfigKeyInfo {
            key: "image_engine.swww_args".to_string(),
            value_type: "array".to_string(),
            description: "透传给 swww img 的参数".to_string(),
            default: serde_json::json!([
                "--transition-type=fade",
                "--transition-duration=2.0",
                "--transition-fps=60",
                "--transition-step=20",
                "--resize=crop"
            ]),
            constraints: None,
        },
        
        // ==================== vram ====================
        ConfigKeyInfo {
            key: "vram.enabled".to_string(),
            value_type: "boolean".to_string(),
            description: "是否启用显存监控".to_string(),
            default: serde_json::json!(true),
            constraints: None,
        },
        ConfigKeyInfo {
            key: "vram.threshold_percent".to_string(),
            value_type: "number".to_string(),
            description: "降级阈值：显存剩余低于此百分比时切换到静态壁纸".to_string(),
            default: serde_json::json!(25.0),
            constraints: Some(ConfigConstraints {
                min: Some(serde_json::json!(5.0)),
                max: Some(serde_json::json!(50.0)),
                enum_values: None,
                pattern: None,
            }),
        },
        ConfigKeyInfo {
            key: "vram.recovery_percent".to_string(),
            value_type: "number".to_string(),
            description: "恢复阈值：显存剩余高于此百分比时恢复动态壁纸".to_string(),
            default: serde_json::json!(40.0),
            constraints: Some(ConfigConstraints {
                min: Some(serde_json::json!(20.0)),
                max: Some(serde_json::json!(80.0)),
                enum_values: None,
                pattern: None,
            }),
        },
        ConfigKeyInfo {
            key: "vram.check_interval".to_string(),
            value_type: "integer".to_string(),
            description: "显存检测间隔（秒）".to_string(),
            default: serde_json::json!(2),
            constraints: Some(ConfigConstraints {
                min: Some(serde_json::json!(1)),
                max: Some(serde_json::json!(60)),
                enum_values: None,
                pattern: None,
            }),
        },
        ConfigKeyInfo {
            key: "vram.cooldown_seconds".to_string(),
            value_type: "integer".to_string(),
            description: "降级冷却时间（秒），防止频繁切换".to_string(),
            default: serde_json::json!(30),
            constraints: Some(ConfigConstraints {
                min: Some(serde_json::json!(10)),
                max: Some(serde_json::json!(600)),
                enum_values: None,
                pattern: None,
            }),
        },
        
        // ==================== daemon ====================
        ConfigKeyInfo {
            key: "daemon.socket_path".to_string(),
            value_type: "string".to_string(),
            description: "Unix Socket 路径".to_string(),
            default: serde_json::json!("/tmp/lianwall.sock"),
            constraints: None,
        },
        ConfigKeyInfo {
            key: "daemon.pid_path".to_string(),
            value_type: "string".to_string(),
            description: "PID 文件路径".to_string(),
            default: serde_json::json!("/tmp/lianwall.pid"),
            constraints: None,
        },
        ConfigKeyInfo {
            key: "daemon.log_level".to_string(),
            value_type: "string".to_string(),
            description: "日志级别".to_string(),
            default: serde_json::json!("info"),
            constraints: Some(ConfigConstraints {
                min: None,
                max: None,
                enum_values: Some(vec![
                    "error".to_string(),
                    "warn".to_string(),
                    "info".to_string(),
                    "debug".to_string(),
                    "trace".to_string(),
                ]),
                pattern: None,
            }),
        },
    ]
}

/// 获取壁纸空间
async fn get_space(state: &Arc<SharedState>, mode: Option<WallMode>) -> Response {
    let engine = state.get_engine_state().await;
    let mode = mode.unwrap_or(engine.mode);
    
    let space = if mode == WallMode::Video {
        state.get_video_space().await
    } else {
        state.get_image_space().await
    };
    
    let items: Vec<WallpaperPoint> = space.items.iter().enumerate().map(|(i, w)| {
        let filename = w.path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let in_cooldown = space.cooldown_queue.contains(&i);
        let is_current = space.current_index == Some(i);
        
        WallpaperPoint {
            index: i,
            filename,
            path: w.path.clone(),
            angle: w.angle,
            locked: w.locked,
            in_cooldown,
            is_current,
        }
    }).collect();
    
    Response::Space(SpaceSnapshot {
        mode,
        items,
        pointer_angle: space.pointer,
        cooldown_size: space.cooldown_queue.len(),
        current_index: space.current_index,
    })
}

/// 获取时间调度信息
async fn get_time_info(state: &Arc<SharedState>) -> Response {
    let video_space = state.get_video_space().await;
    let image_space = state.get_image_space().await;
    let time_points = state.get_time_points().await;
    
    let current_time = chrono::Local::now().format("%H:%M").to_string();
    let now = TimePoint::now();
    
    // 构建时间点列表（排序后的字符串）
    let time_points_vec: Vec<String> = time_points
        .iter()
        .map(|tp| format!("{:02}:{:02}", tp.hour, tp.minute))
        .collect();
    
    // 计算下一个时间点
    let next_time_point = time_points
        .iter()
        .find(|tp| **tp > now)
        .or_else(|| time_points.first()) // 如果当前已是最后一个点，循环到第一个
        .map(|tp| format!("{:02}:{:02}", tp.hour, tp.minute));
    
    let video_schedule = build_mode_schedule(&video_space, &time_points_vec, &next_time_point);
    let image_schedule = build_mode_schedule(&image_space, &time_points_vec, &next_time_point);
    
    Response::TimeInfo(TimeScheduleInfo {
        current_time,
        video_schedule,
        image_schedule,
    })
}

/// 为单个模式构建调度信息
fn build_mode_schedule(
    space: &WallpaperSpace,
    time_points: &[String],
    next_time_point: &Option<String>,
) -> ModeSchedule {
    let wallpaper_segments: Vec<WallpaperTimeSegment> = space
        .items
        .iter()
        .map(|item| {
            let filename = item.path.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            
            let all_day = item.time_constraints.is_empty();
            
            let active_ranges: Vec<TimeRangeInfo> = if all_day {
                vec![] // 全天可用时不返回具体范围
            } else {
                item.time_constraints
                    .iter()
                    .map(|range| TimeRangeInfo {
                        start: format!("{:02}:{:02}", range.start.hour, range.start.minute),
                        end: format!("{:02}:{:02}", range.end.hour, range.end.minute),
                        crosses_midnight: range.crosses_midnight(),
                    })
                    .collect()
            };
            
            WallpaperTimeSegment {
                filename,
                path: item.path.clone(),
                active_ranges,
                all_day,
            }
        })
        .collect();
    
    ModeSchedule {
        scanned_count: space.items.len(),
        active_count: space.items.iter().filter(|w| !w.locked).count(),
        time_points: time_points.to_vec(),
        next_time_point: next_time_point.clone(),
        wallpaper_segments,
    }
}
