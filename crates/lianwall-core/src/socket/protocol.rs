//! Socket 通信协议 - 请求与响应结构
//!
//! 协议格式: 长度前缀帧
//! ```text
//! +----------------+------------------+
//! | 长度 (4 bytes) | JSON 数据        |
//! | u32 big-endian | UTF-8 字符串     |
//! +----------------+------------------+
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config::WallMode;

/// 协议版本（用于兼容性检查）
pub const PROTOCOL_VERSION: u32 = 1;

/// 最大消息大小 (1 MB)
pub const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

// ============================================================================
// Request - 客户端请求
// ============================================================================

/// 客户端请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", content = "data")]
pub enum Request {
    // === 状态查询 ===
    /// 心跳检测
    Ping,

    /// 获取完整状态
    Status,

    /// 获取向量空间快照（GUI 绘图用）
    GetSpace,

    // === 壁纸控制 ===
    /// 切换到下一张壁纸
    Next,

    /// 切换到上一张壁纸（反向黄金角）
    Previous,

    /// 指定壁纸（强制跳转指针）
    SetWallpaper { path: PathBuf },

    /// 切换模式
    SetMode { mode: WallMode },

    /// 锁定壁纸（不再被选中）
    Lock { path: PathBuf },

    /// 解锁壁纸
    Unlock { path: PathBuf },

    /// 重新扫描目录并重载配置
    Reload,

    /// 获取时间调度信息（GUI 时间轴用）
    GetTimeInfo,

    // === 生命周期 ===
    /// 优雅关闭守护进程
    Shutdown,
}

// ============================================================================
// Response - 服务端响应
// ============================================================================

/// 服务端响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// 是否成功
    pub success: bool,

    /// 响应数据（成功时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ResponseData>,

    /// 错误信息（失败时）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    /// 创建成功响应（无数据）
    pub fn ok() -> Self {
        Self {
            success: true,
            data: Some(ResponseData::Ok),
            error: None,
        }
    }

    /// 创建成功响应（带数据）
    pub fn with_data(data: ResponseData) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// 创建错误响应
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// 响应数据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum ResponseData {
    /// 简单确认
    Ok,

    /// 心跳响应
    Pong,

    /// 完整状态信息
    Status(StatusInfo),

    /// 向量空间快照
    Space(SpaceSnapshot),

    /// 时间调度信息
    TimeInfo(TimeScheduleInfo),
}

// ============================================================================
// 数据结构
// ============================================================================

/// 状态信息（status 命令响应）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusInfo {
    /// 当前模式
    pub mode: WallMode,

    /// 当前壁纸路径
    pub current: Option<PathBuf>,

    /// 当前引擎名称
    pub engine: String,

    /// 当前活跃壁纸数（时间过滤后，在向量空间中）
    pub total_wallpapers: usize,

    /// 锁定数量
    pub locked_count: usize,

    /// 可用数量（未锁定且不在冷却中）
    pub available_count: usize,

    /// 扫描的壁纸总数（含不活跃的）
    pub scanned_count: usize,

    /// 显存使用（MB）
    pub vram_used_mb: u64,

    /// 显存总量（MB）
    pub vram_total_mb: u64,

    /// 守护进程运行时间（秒）
    pub uptime_secs: u64,

    /// 协议版本
    pub protocol_version: u32,

    /// 下一个时间关键点（"HH:MM" 格式，None 表示无时间约束）
    pub next_time_point: Option<String>,

    /// 时间关键点数量
    pub time_points_count: usize,
}

/// 向量空间快照（GUI 绘图用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceSnapshot {
    /// 壁纸点列表
    pub items: Vec<WallpaperPoint>,

    /// 当前指针角度 [0, 2π)
    pub pointer_angle: f64,

    /// 冷却队列大小
    pub cooldown_size: usize,

    /// 当前壁纸索引
    pub current_index: Option<usize>,
}

/// 单个壁纸点（用于可视化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallpaperPoint {
    /// 索引
    pub index: usize,

    /// 文件名（不含路径）
    pub filename: String,

    /// 完整路径
    pub path: PathBuf,

    /// 角度 [0, 2π)
    pub angle: f64,

    /// 是否锁定
    pub locked: bool,

    /// 是否在冷却队列中
    pub in_cooldown: bool,
}

/// 时间调度信息（GUI 时间轴绘制用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeScheduleInfo {
    /// 当前时间 ("HH:MM" 格式)
    pub current_time: String,

    /// 视频模式时间段
    pub video_schedule: ModeSchedule,

    /// 图片模式时间段
    pub image_schedule: ModeSchedule,
}

/// 单个模式的调度信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeSchedule {
    /// 扫描的壁纸总数
    pub scanned_count: usize,

    /// 当前活跃数
    pub active_count: usize,

    /// 关键时间点列表 ("HH:MM" 格式，已排序)
    pub time_points: Vec<String>,

    /// 下一个关键时间点 ("HH:MM" 格式)
    pub next_time_point: Option<String>,

    /// 壁纸时间分布（用于时间轴可视化）
    pub wallpaper_segments: Vec<WallpaperTimeSegment>,
}

/// 壁纸时间段（用于时间轴上显示壁纸活跃区间）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallpaperTimeSegment {
    /// 文件名
    pub filename: String,

    /// 完整路径
    pub path: PathBuf,

    /// 活跃时间段列表（一个壁纸可能在多个时间段活跃）
    pub active_ranges: Vec<TimeRangeInfo>,

    /// 是否全天可用（无时间约束）
    pub all_day: bool,
}

/// 时间范围信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRangeInfo {
    /// 开始时间 ("HH:MM" 格式)
    pub start: String,

    /// 结束时间 ("HH:MM" 格式)
    pub end: String,

    /// 是否跨天
    pub crosses_midnight: bool,
}

// ============================================================================
// 辅助方法
// ============================================================================

impl Request {
    /// 获取命令名称（用于日志）
    pub fn name(&self) -> &'static str {
        match self {
            Request::Ping => "Ping",
            Request::Status => "Status",
            Request::GetSpace => "GetSpace",
            Request::GetTimeInfo => "GetTimeInfo",
            Request::Next => "Next",
            Request::Previous => "Previous",
            Request::SetWallpaper { .. } => "SetWallpaper",
            Request::SetMode { .. } => "SetMode",
            Request::Lock { .. } => "Lock",
            Request::Unlock { .. } => "Unlock",
            Request::Reload => "Reload",
            Request::Shutdown => "Shutdown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialize() {
        let req = Request::Next;
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"cmd\":\"Next\""));

        let req = Request::SetWallpaper {
            path: PathBuf::from("/test/wallpaper.mp4"),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"cmd\":\"SetWallpaper\""));
        assert!(json.contains("wallpaper.mp4"));
    }

    #[test]
    fn test_request_deserialize() {
        let json = r#"{"cmd":"Next"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert!(matches!(req, Request::Next));

        let json = r#"{"cmd":"SetMode","data":{"mode":"Video"}}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert!(matches!(req, Request::SetMode { mode: WallMode::Video }));
    }

    #[test]
    fn test_response_ok() {
        let resp = Response::ok();
        assert!(resp.success);
        assert!(matches!(resp.data, Some(ResponseData::Ok)));
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_response_err() {
        let resp = Response::err("something went wrong");
        assert!(!resp.success);
        assert!(resp.data.is_none());
        assert_eq!(resp.error.as_deref(), Some("something went wrong"));
    }

    #[test]
    fn test_status_info_serialize() {
        let status = StatusInfo {
            mode: WallMode::Video,
            current: Some(PathBuf::from("/test/a.mp4")),
            engine: "mpvpaper".to_string(),
            total_wallpapers: 10,
            locked_count: 2,
            available_count: 8,
            scanned_count: 15,
            vram_used_mb: 1024,
            vram_total_mb: 4096,
            uptime_secs: 3600,
            protocol_version: PROTOCOL_VERSION,
            next_time_point: Some("18:00".to_string()),
            time_points_count: 4,
        };

        let json = serde_json::to_string_pretty(&status).unwrap();
        assert!(json.contains("\"mode\": \"Video\""));
        assert!(json.contains("\"engine\": \"mpvpaper\""));
    }
}
