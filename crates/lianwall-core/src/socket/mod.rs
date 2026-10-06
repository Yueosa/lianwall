//! Socket 通信模块
//!
//! 提供守护进程与客户端之间的 Unix Socket 通信功能
//!
//! # 架构 - 订阅 + 广播模式
//!
//! ```text
//! ┌─────────────┐                    ┌─────────────┐
//! │   Client    │  ── Request ──→    │   Server    │
//! │ (CLI/GUI)   │  ←─ Response ──    │  (Daemon)   │
//! │             │  ←─ Event ─────    │             │
//! └─────────────┘                    └─────────────┘
//!
//! 订阅模式:
//! ┌─────────────┐                    ┌─────────────┐
//! │   Client    │  ── Subscribe ──→  │   Server    │
//! │             │  ←─ Subscribed ──  │             │
//! │             │  ←─ Event ─────    │  (广播)     │
//! │             │  ←─ Event ─────    │             │
//! └─────────────┘                    └─────────────┘
//! ```
//!
//! # 协议格式
//!
//! 换行分隔 JSON（NDJSON）：每帧一行 JSON，以 `\n` 结尾。
//! 请求/响应帧顶层带 `id` 字段（u64，客户端自增，响应回带）；
//! 不带 `id` 或 `id=0` 的帧为服务端主动推送（Event、订阅后的同步状态）。
//!
//! # 消息分类
//!
//! - **Query**: 无状态查询，可并发处理
//! - **Command**: 状态修改，排队串行执行
//! - **Subscribe**: 建立订阅关系
//! - **Event**: 服务端推送
//!
//! # 模块结构
//!
//! - [`protocol`] - 协议定义（请求、响应、事件、帧辅助函数）
//! - [`error`] - 错误类型

pub mod error;
pub mod protocol;

// Re-exports
pub use error::SocketError;
pub use protocol::{
    // 常量
    PROTOCOL_VERSION, MAX_MESSAGE_SIZE,
    // 帧辅助
    extract_request_id, request_frame, response_frame,
    // 请求
    Request,
    // 响应
    Response, ErrorCode,
    // 事件
    Event, EventType,
    // 辅助枚举
    WallpaperTrigger, StatusChange, SpaceUpdateReason, VramAction, VramOverrideAction,
    // 数据结构
    StatusInfo, SpaceSnapshot, WallpaperPoint, SpaceSummary,
    TimeScheduleInfo, ModeSchedule, WallpaperTimeSegment, TimeRangeInfo,
    ConfigSnapshot, ConfigKeyInfo, ConfigConstraints,
    HookInfo, RotationStatusInfo,
};
