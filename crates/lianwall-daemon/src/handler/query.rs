//! Query Handler - 只读请求处理
//!
//! 这些请求只读取状态，不修改，可以并发执行

use std::sync::Arc;

use lianwall_core::socket::{
    Request, Response, StatusInfo, ConfigSnapshot, SpaceSnapshot, WallpaperPoint, 
    TimeScheduleInfo, ModeSchedule, ErrorCode, PROTOCOL_VERSION,
};
use lianwall_core::config::WallMode;

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
        next_switch_secs: Some(config.image_engine.interval),
    })
}

/// 获取配置
async fn get_config(state: &Arc<SharedState>, key: Option<String>) -> Response {
    let config = state.get_config().await;
    
    if let Some(key) = key {
        // 获取特定配置项
        let value = match key.as_str() {
            "paths.video_dir" => serde_json::to_value(&config.paths.video_dir).ok(),
            "paths.image_dir" => serde_json::to_value(&config.paths.image_dir).ok(),
            "image_engine.interval" => serde_json::to_value(config.image_engine.interval).ok(),
            "video_engine.interval" => serde_json::to_value(config.video_engine.interval).ok(),
            "mode" => serde_json::to_value(&config.paths.mode).ok(),
            "vram.enabled" => serde_json::to_value(config.vram.enabled).ok(),
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
                modifiable_keys: None, // TODO: 添加可修改字段列表
            }),
            Err(e) => Response::error(ErrorCode::InternalError, format!("Serialize error: {}", e)),
        }
    }
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
    
    let current_time = chrono::Local::now().format("%H:%M").to_string();
    
    // 创建空的调度信息
    let video_schedule = ModeSchedule {
        scanned_count: video_space.items.len(),
        active_count: video_space.items.iter().filter(|w| !w.locked).count(),
        time_points: vec![],
        next_time_point: None,
        wallpaper_segments: vec![],
    };
    
    let image_schedule = ModeSchedule {
        scanned_count: image_space.items.len(),
        active_count: image_space.items.iter().filter(|w| !w.locked).count(),
        time_points: vec![],
        next_time_point: None,
        wallpaper_segments: vec![],
    };
    
    Response::TimeInfo(TimeScheduleInfo {
        current_time,
        video_schedule,
        image_schedule,
    })
}
