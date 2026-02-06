//! Socket 通信协议 - 订阅 + 广播模式
//!
//! ## 设计原则
//!
//! 1. **消息分类**:
//!    - Query: 无状态查询，可并发处理
//!    - Command: 状态修改，排队串行执行
//!    - Subscribe: 建立订阅关系
//!    - Event: 服务端推送
//!
//! 2. **可扩展性**:
//!    - 使用 `#[serde(tag = "...")]` 标签式枚举
//!    - 新增字段使用 `#[serde(default)]`
//!    - 协议版本号用于兼容性检查
//!
//! 3. **错误处理**:
//!    - 未知请求返回 `InvalidRequest` 错误
//!    - JSON 自动忽略未知字段//!
//! ## TODO 待改进
//!
//! - [ ] `Unsubscribe` 添加可选 `events` 参数支持部分取消订阅
//! - [ ] `GetStatus` 中的 `next_time_point` 和 `time_points_count` 与 `GetTimeInfo` 功能重复，考虑移除
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config::WallMode;

// ============================================================================
// 常量
// ============================================================================

/// 协议版本
pub const PROTOCOL_VERSION: u32 = 2;

/// 最大消息大小 (1 MB)
pub const MAX_MESSAGE_SIZE: usize = 1024 * 1024;

// ============================================================================
// 请求 (Request)
// ============================================================================

/// 客户端请求
///
/// 分为三类:
/// - **Query**: 无状态查询，立即响应
/// - **Command**: 状态修改，排队执行
/// - **Subscribe**: 订阅管理
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum Request {
    // ==================== Query (无状态查询) ====================
    /// 心跳检测
    Ping,

    /// 获取完整状态
    GetStatus,

    /// 获取向量空间快照
    /// - `mode`: None = 当前模式, Some = 指定模式
    GetSpace {
        #[serde(default)]
        mode: Option<WallMode>,
    },

    /// 获取时间调度信息
    GetTimeInfo,

    /// 获取配置
    /// - `key`: None = 全部配置
    /// - `key`: Some("section") = 某个配置节
    /// - `key`: Some("section.field") = 某个字段
    GetConfig {
        #[serde(default)]
        key: Option<String>,
    },

    // ==================== Command (状态修改) ====================
    /// 切换到下一张壁纸
    Next,

    /// 切换到上一张壁纸
    Prev,

    /// 指定壁纸
    SetWallpaper { path: PathBuf },

    /// 切换模式
    SetMode { mode: WallMode },

    /// 锁定壁纸
    Lock { path: PathBuf },

    /// 解锁壁纸
    Unlock { path: PathBuf },

    /// 切换锁定状态
    ToggleLock { path: PathBuf },

    /// 设置配置字段
    SetConfig {
        key: String,
        value: serde_json::Value,
    },

    /// 重新扫描壁纸目录
    Rescan,

    /// 重新加载配置文件
    ReloadConfig,

    /// 关闭守护进程
    Shutdown,

    // ==================== Subscribe (订阅管理) ====================
    /// 订阅事件
    Subscribe {
        /// 要订阅的事件类型列表
        events: Vec<EventType>,
        /// 是否在订阅成功后立即推送当前状态
        #[serde(default)]
        immediate_sync: bool,
    },

    /// 取消订阅
    Unsubscribe,
}

impl Request {
    /// 获取命令名称（用于日志）
    pub fn name(&self) -> &'static str {
        match self {
            // Query
            Request::Ping => "Ping",
            Request::GetStatus => "GetStatus",
            Request::GetSpace { .. } => "GetSpace",
            Request::GetTimeInfo => "GetTimeInfo",
            Request::GetConfig { .. } => "GetConfig",
            // Command
            Request::Next => "Next",
            Request::Prev => "Prev",
            Request::SetWallpaper { .. } => "SetWallpaper",
            Request::SetMode { .. } => "SetMode",
            Request::Lock { .. } => "Lock",
            Request::Unlock { .. } => "Unlock",
            Request::ToggleLock { .. } => "ToggleLock",
            Request::SetConfig { .. } => "SetConfig",
            Request::Rescan => "Rescan",
            Request::ReloadConfig => "ReloadConfig",
            Request::Shutdown => "Shutdown",
            // Subscribe
            Request::Subscribe { .. } => "Subscribe",
            Request::Unsubscribe => "Unsubscribe",
        }
    }

    /// 是否是查询请求（可并发处理）
    pub fn is_query(&self) -> bool {
        matches!(
            self,
            Request::Ping
                | Request::GetStatus
                | Request::GetSpace { .. }
                | Request::GetTimeInfo
                | Request::GetConfig { .. }
        )
    }

    /// 是否是修改状态的命令（需要排队）
    pub fn is_command(&self) -> bool {
        matches!(
            self,
            Request::Next
                | Request::Prev
                | Request::SetWallpaper { .. }
                | Request::SetMode { .. }
                | Request::Lock { .. }
                | Request::Unlock { .. }
                | Request::ToggleLock { .. }
                | Request::SetConfig { .. }
                | Request::Rescan
                | Request::ReloadConfig
                | Request::Shutdown
        )
    }

    /// 是否是订阅相关请求
    pub fn is_subscription(&self) -> bool {
        matches!(self, Request::Subscribe { .. } | Request::Unsubscribe)
    }
}

// ============================================================================
// 响应 (Response)
// ============================================================================

/// 服务端响应
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Response {
    // ==================== 基础响应 ====================
    /// 成功（无数据）
    Ok,

    /// 错误
    Error {
        code: ErrorCode,
        message: String,
    },

    /// 心跳响应
    Pong {
        /// 守护进程运行时间（秒）
        uptime_secs: u64,
        /// 协议版本
        protocol_version: u32,
    },

    // ==================== 数据响应 ====================
    /// 状态信息
    Status(StatusInfo),

    /// 向量空间快照
    Space(SpaceSnapshot),

    /// 时间调度信息
    TimeInfo(TimeScheduleInfo),

    /// 配置数据
    Config(ConfigSnapshot),

    // ==================== 订阅响应 ====================
    /// 订阅成功
    Subscribed {
        /// 会话 ID（用于识别连接）
        session_id: String,
        /// 实际订阅的事件类型
        subscribed_events: Vec<EventType>,
    },

    /// 取消订阅成功
    Unsubscribed,

    /// 事件推送（订阅后收到）
    Event(Event),
}

impl Response {
    /// 创建成功响应
    pub fn ok() -> Self {
        Response::Ok
    }

    /// 创建错误响应
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Response::Error {
            code,
            message: message.into(),
        }
    }

    /// 是否成功
    pub fn is_success(&self) -> bool {
        !matches!(self, Response::Error { .. })
    }

    /// 获取错误信息（如果是错误响应）
    pub fn error_message(&self) -> Option<&str> {
        match self {
            Response::Error { message, .. } => Some(message),
            _ => None,
        }
    }
}

// ============================================================================
// 错误码 (ErrorCode)
// ============================================================================

/// 错误码
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// 未知错误
    Unknown,
    /// 无效请求（命令不存在或参数错误）
    InvalidRequest,
    /// 资源不存在（壁纸路径不存在等）
    NotFound,
    /// 引擎错误（mpvpaper/swww 启动失败等）
    EngineError,
    /// 配置错误（无效配置值等）
    ConfigError,
    /// 权限错误
    PermissionDenied,
    /// 操作超时
    Timeout,
    /// 向量空间为空（没有可用壁纸）
    EmptySpace,
    /// 没有历史记录（prev 无法回退）
    NoHistory,
    /// 已经订阅
    AlreadySubscribed,
    /// 未订阅
    NotSubscribed,
    /// 内部错误
    InternalError,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCode::Unknown => write!(f, "unknown"),
            ErrorCode::InvalidRequest => write!(f, "invalid_request"),
            ErrorCode::NotFound => write!(f, "not_found"),
            ErrorCode::EngineError => write!(f, "engine_error"),
            ErrorCode::ConfigError => write!(f, "config_error"),
            ErrorCode::PermissionDenied => write!(f, "permission_denied"),
            ErrorCode::Timeout => write!(f, "timeout"),
            ErrorCode::EmptySpace => write!(f, "empty_space"),
            ErrorCode::NoHistory => write!(f, "no_history"),
            ErrorCode::AlreadySubscribed => write!(f, "already_subscribed"),
            ErrorCode::NotSubscribed => write!(f, "not_subscribed"),
            ErrorCode::InternalError => write!(f, "internal_error"),
        }
    }
}

// ============================================================================
// 事件类型 (EventType)
// ============================================================================

/// 可订阅的事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// 壁纸切换
    WallpaperChanged,
    /// 状态变化（模式切换、引擎状态等）
    StatusChanged,
    /// 配置变化
    ConfigChanged,
    /// 向量空间更新（壁纸锁定/解锁、重新扫描等）
    SpaceUpdated,
    /// 显存状态变化（降级/恢复）
    VramChanged,
    /// 时间点触发
    TimePointReached,
    /// 扫描进度（流式返回）
    ScanProgress,
    /// 错误发生
    Error,
    /// 全部事件
    All,
}

impl EventType {
    /// 展开 All 为所有具体事件类型
    pub fn expand(types: &[EventType]) -> Vec<EventType> {
        if types.contains(&EventType::All) {
            vec![
                EventType::WallpaperChanged,
                EventType::StatusChanged,
                EventType::ConfigChanged,
                EventType::SpaceUpdated,
                EventType::VramChanged,
                EventType::TimePointReached,
                EventType::ScanProgress,
                EventType::Error,
            ]
        } else {
            types.to_vec()
        }
    }
}

// ============================================================================
// 事件 (Event)
// ============================================================================

/// 服务端推送事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum Event {
    /// 壁纸切换
    WallpaperChanged {
        /// 壁纸路径
        path: PathBuf,
        /// 文件名
        filename: String,
        /// 当前模式
        mode: WallMode,
        /// 触发原因
        trigger: WallpaperTrigger,
    },

    /// 状态变化
    StatusChanged {
        /// 变化的字段列表
        changes: Vec<StatusChange>,
    },

    /// 配置变化
    ConfigChanged {
        /// 配置键
        key: String,
        /// 旧值
        old_value: serde_json::Value,
        /// 新值
        new_value: serde_json::Value,
    },

    /// 向量空间更新
    SpaceUpdated {
        /// 更新的模式
        mode: WallMode,
        /// 更新原因
        reason: SpaceUpdateReason,
        /// 空间摘要
        summary: SpaceSummary,
    },

    /// 显存状态变化
    VramChanged {
        /// 动作
        action: VramAction,
        /// 已用显存 (MB)
        used_mb: u64,
        /// 总显存 (MB)
        total_mb: u64,
        /// 剩余百分比
        free_percent: f32,
    },

    /// 时间点触发
    TimePointReached {
        /// 当前时间 ("HH:MM")
        time: String,
        /// 下一个时间点 ("HH:MM")
        next_time: Option<String>,
    },

    /// 扫描进度（流式返回）
    ScanProgress {
        /// 扫描的模式
        mode: WallMode,
        /// 已扫描目录数
        dirs_scanned: usize,
        /// 已发现文件数
        files_found: usize,
        /// 是否完成
        completed: bool,
    },

    /// 错误事件
    Error {
        /// 错误码
        code: ErrorCode,
        /// 错误信息
        message: String,
        /// 是否可恢复
        recoverable: bool,
    },
}

impl Event {
    /// 获取事件类型
    pub fn event_type(&self) -> EventType {
        match self {
            Event::WallpaperChanged { .. } => EventType::WallpaperChanged,
            Event::StatusChanged { .. } => EventType::StatusChanged,
            Event::ConfigChanged { .. } => EventType::ConfigChanged,
            Event::SpaceUpdated { .. } => EventType::SpaceUpdated,
            Event::VramChanged { .. } => EventType::VramChanged,
            Event::TimePointReached { .. } => EventType::TimePointReached,
            Event::ScanProgress { .. } => EventType::ScanProgress,
            Event::Error { .. } => EventType::Error,
        }
    }

    /// 获取事件名称（用于日志）
    pub fn name(&self) -> &'static str {
        match self {
            Event::WallpaperChanged { .. } => "WallpaperChanged",
            Event::StatusChanged { .. } => "StatusChanged",
            Event::ConfigChanged { .. } => "ConfigChanged",
            Event::SpaceUpdated { .. } => "SpaceUpdated",
            Event::VramChanged { .. } => "VramChanged",
            Event::TimePointReached { .. } => "TimePointReached",
            Event::ScanProgress { .. } => "ScanProgress",
            Event::Error { .. } => "Error",
        }
    }
}

// ============================================================================
// 辅助枚举
// ============================================================================

/// 壁纸切换触发原因
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallpaperTrigger {
    /// 定时切换
    Scheduled,
    /// 用户手动 Next
    ManualNext,
    /// 用户手动 Prev
    ManualPrev,
    /// 用户指定壁纸
    ManualSet,
    /// 模式切换后的首张壁纸
    ModeSwitch,
    /// 显存降级
    VramDowngrade,
    /// 显存恢复
    VramUpgrade,
    /// 时间点触发重建空间
    TimePointRefresh,
    /// 守护进程启动
    DaemonStart,
}

/// 状态变化字段
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "field", content = "value")]
pub enum StatusChange {
    /// 模式变化
    Mode(WallMode),
    /// 引擎变化
    Engine(String),
    /// 壁纸总数变化
    TotalWallpapers(usize),
    /// 可用数量变化
    AvailableCount(usize),
    /// 锁定数量变化
    LockedCount(usize),
    /// 显存降级状态变化
    VramDegraded(bool),
}

/// 空间更新原因
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpaceUpdateReason {
    /// 壁纸锁定/解锁
    LockChanged,
    /// 目录重新扫描
    Rescanned,
    /// 时间点刷新
    TimePointRefresh,
    /// 配置变更（目录改变）
    ConfigChanged,
}

/// 显存动作
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VramAction {
    /// 降级到静态壁纸
    Downgrade,
    /// 恢复到动态壁纸
    Upgrade,
}

// ============================================================================
// 数据结构
// ============================================================================

/// 状态信息（GetStatus 响应）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusInfo {
    /// 当前模式
    pub mode: WallMode,

    /// 当前壁纸路径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<PathBuf>,

    /// 当前壁纸文件名
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_filename: Option<String>,

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

    /// 是否处于显存降级状态
    pub vram_degraded: bool,

    /// 守护进程运行时间（秒）
    pub uptime_secs: u64,

    /// 协议版本
    pub protocol_version: u32,

    /// 下一个时间关键点（"HH:MM" 格式，None 表示无时间约束）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_time_point: Option<String>,

    /// 时间关键点数量
    pub time_points_count: usize,

    /// 下次壁纸切换倒计时（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_switch_secs: Option<u64>,
}

/// 向量空间快照（GetSpace 响应）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceSnapshot {
    /// 查询的模式
    pub mode: WallMode,

    /// 壁纸点列表
    pub items: Vec<WallpaperPoint>,

    /// 当前指针角度 [0, 2π)
    pub pointer_angle: f64,

    /// 冷却队列大小
    pub cooldown_size: usize,

    /// 当前壁纸索引
    #[serde(skip_serializing_if = "Option::is_none")]
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

    /// 是否是当前壁纸
    #[serde(default)]
    pub is_current: bool,
}

/// 空间摘要（用于事件推送，避免发送完整空间）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceSummary {
    /// 壁纸总数
    pub total: usize,
    /// 可用数量
    pub available: usize,
    /// 锁定数量
    pub locked: usize,
    /// 冷却队列大小
    pub in_cooldown: usize,
}

/// 时间调度信息（GetTimeInfo 响应）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeScheduleInfo {
    /// 当前时间 ("HH:MM" 格式)
    pub current_time: String,

    /// 视频模式调度
    pub video_schedule: ModeSchedule,

    /// 图片模式调度
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
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(default)]
    pub crosses_midnight: bool,
}

/// 配置快照（GetConfig 响应）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSnapshot {
    /// 请求的 key（None = 全部）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,

    /// 配置值（JSON 格式）
    pub value: serde_json::Value,

    /// 可修改字段列表（仅 key=None 时返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modifiable_keys: Option<Vec<ConfigKeyInfo>>,
}

/// 配置字段信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigKeyInfo {
    /// 配置键（如 "video_engine.interval"）
    pub key: String,

    /// 值类型: "string", "integer", "float", "boolean", "array", "enum"
    pub value_type: String,

    /// 字段描述
    pub description: String,

    /// 默认值
    pub default: serde_json::Value,

    /// 约束（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<ConfigConstraints>,
}

/// 配置约束
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigConstraints {
    /// 最小值（数字类型）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<serde_json::Value>,

    /// 最大值（数字类型）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<serde_json::Value>,

    /// 枚举值列表（enum 类型）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,

    /// 正则表达式（字符串类型）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialize() {
        // Query
        let req = Request::Ping;
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""cmd":"Ping""#));

        // Command with data
        let req = Request::SetMode { mode: WallMode::Video };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""cmd":"SetMode""#));
        assert!(json.contains(r#""mode":"Video""#));

        // Subscribe
        let req = Request::Subscribe {
            events: vec![EventType::WallpaperChanged, EventType::StatusChanged],
            immediate_sync: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""cmd":"Subscribe""#));
        assert!(json.contains(r#""wallpaper_changed""#));
    }

    #[test]
    fn test_request_deserialize() {
        let json = r#"{"cmd":"Ping"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert!(matches!(req, Request::Ping));

        // 使用 internally tagged，字段直接展平
        let json = r#"{"cmd":"GetSpace","mode":"Video"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert!(matches!(req, Request::GetSpace { mode: Some(WallMode::Video) }));

        // 无 mode 字段时使用默认值
        let json = r#"{"cmd":"GetSpace"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert!(matches!(req, Request::GetSpace { mode: None }));
    }

    #[test]
    fn test_response_serialize() {
        let resp = Response::ok();
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""type":"Ok""#));

        let resp = Response::error(ErrorCode::NotFound, "壁纸不存在");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""type":"Error""#));
        assert!(json.contains(r#""code":"not_found""#));
    }

    #[test]
    fn test_event_serialize() {
        let event = Event::WallpaperChanged {
            path: PathBuf::from("/wallpapers/test.mp4"),
            filename: "test.mp4".to_string(),
            mode: WallMode::Video,
            trigger: WallpaperTrigger::Scheduled,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"WallpaperChanged""#));
        assert!(json.contains(r#""trigger":"scheduled""#));

        // 测试扫描进度事件
        let event = Event::ScanProgress {
            mode: WallMode::Video,
            dirs_scanned: 10,
            files_found: 42,
            completed: false,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"ScanProgress""#));
        assert!(json.contains(r#""files_found":42"#));
    }

    #[test]
    fn test_event_type_expand() {
        let types = vec![EventType::All];
        let expanded = EventType::expand(&types);
        assert_eq!(expanded.len(), 8);  // 包含 ScanProgress
        assert!(expanded.contains(&EventType::WallpaperChanged));
        assert!(expanded.contains(&EventType::ScanProgress));
        assert!(!expanded.contains(&EventType::All));

        let types = vec![EventType::WallpaperChanged, EventType::StatusChanged];
        let expanded = EventType::expand(&types);
        assert_eq!(expanded.len(), 2);
    }

    #[test]
    fn test_request_classification() {
        assert!(Request::Ping.is_query());
        assert!(Request::GetStatus.is_query());
        assert!(!Request::Next.is_query());

        assert!(Request::Next.is_command());
        assert!(Request::SetMode { mode: WallMode::Video }.is_command());
        assert!(!Request::Ping.is_command());

        assert!(Request::Subscribe { events: vec![], immediate_sync: false }.is_subscription());
        assert!(Request::Unsubscribe.is_subscription());
        assert!(!Request::Ping.is_subscription());
    }
}
