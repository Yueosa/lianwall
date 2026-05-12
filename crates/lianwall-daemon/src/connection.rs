//! Connection - 单个客户端连接处理
//!
//! 每个连接一个 Task，负责：
//! - 读取请求
//! - 根据请求类型分发处理
//! - 管理订阅状态
//! - 发送响应和事件

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{broadcast, mpsc, oneshot};

use lianwall_core::socket::{Request, Response, ErrorCode, EventType};

use crate::command::CommandMsg;
use crate::event::{Event, EventBus, SpaceUpdateReason};
use crate::handler;
use crate::state::SharedState;

/// 连接状态
struct ConnectionState {
    /// 连接 ID
    id: u64,
    /// 是否已订阅事件
    subscribed: bool,
    /// 订阅的事件类型集合
    subscribed_events: HashSet<EventType>,
    /// 事件接收器
    event_rx: Option<broadcast::Receiver<Event>>,
    /// 是否需要立即同步状态
    pending_sync: bool,
}

/// 处理单个连接
pub async fn handle(
    id: u64,
    stream: UnixStream,
    state: Arc<SharedState>,
    event_bus: EventBus,
    cmd_tx: mpsc::Sender<CommandMsg>,
) -> anyhow::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    
    let mut conn_state = ConnectionState {
        id,
        subscribed: false,
        subscribed_events: HashSet::new(),
        event_rx: None,
        pending_sync: false,
    };
    
    loop {
        line.clear();
        
        // 如果已订阅，同时监听请求和事件
        if conn_state.subscribed {
            if let Some(ref mut event_rx) = conn_state.event_rx {
                tokio::select! {
                    // 读取客户端请求
                    result = reader.read_line(&mut line) => {
                        match result {
                            Ok(0) => break, // EOF
                            Ok(_) => {
                                if let Some(response) = process_request(
                                    &line,
                                    &state,
                                    &event_bus,
                                    &cmd_tx,
                                    &mut conn_state,
                                ).await {
                                    send_response(&mut writer, &response).await?;
                                    
                                    // 处理 immediate_sync：发送当前状态
                                    if conn_state.pending_sync {
                                        conn_state.pending_sync = false;
                                        let status = handler::handle_query(&state, Request::GetStatus).await;
                                        send_response(&mut writer, &status).await?;
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Connection #{} read error: {}", id, e);
                                break;
                            }
                        }
                    }
                    
                    // 接收事件并推送给客户端
                    result = event_rx.recv() => {
                        match result {
                            Ok(event) => {
                                // 检查事件是否在订阅列表中
                                let event_type = event_to_type(&event);
                                if conn_state.subscribed_events.contains(&event_type) {
                                    if let Some(response) = event_to_response(&event) {
                                        send_response(&mut writer, &response).await?;
                                    }
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!("Connection #{} lagged {} events", id, n);
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                break;
                            }
                        }
                    }
                }
            }
        } else {
            // 未订阅时只监听请求
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    if let Some(response) = process_request(
                        &line,
                        &state,
                        &event_bus,
                        &cmd_tx,
                        &mut conn_state,
                    ).await {
                        send_response(&mut writer, &response).await?;
                        
                        // 处理 immediate_sync：发送当前状态
                        if conn_state.pending_sync {
                            conn_state.pending_sync = false;
                            let status = handler::handle_query(&state, Request::GetStatus).await;
                            send_response(&mut writer, &status).await?;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Connection #{} read error: {}", id, e);
                    break;
                }
            }
        }
    }
    
    Ok(())
}

/// 处理单个请求
async fn process_request(
    line: &str,
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    cmd_tx: &mpsc::Sender<CommandMsg>,
    conn_state: &mut ConnectionState,
) -> Option<Response> {
    // 解析请求
    let request: Request = match serde_json::from_str(line.trim()) {
        Ok(req) => req,
        Err(e) => {
            tracing::warn!("Connection #{} parse error: {}", conn_state.id, e);
            return Some(Response::error(ErrorCode::InvalidRequest, format!("Parse error: {}", e)));
        }
    };
    
    tracing::debug!("Connection #{} request: {:?}", conn_state.id, request);
    
    // 根据请求类型分发
    match &request {
        // Query: 直接读取状态
        Request::Ping => Some(Response::Pong { 
            uptime_secs: state.uptime_secs(),
            protocol_version: lianwall_core::socket::PROTOCOL_VERSION,
        }),
        
        Request::GetStatus
        | Request::GetConfig { .. }
        | Request::GetSpace { .. }
        | Request::GetTimeInfo => {
            Some(handler::handle_query(state, request).await)
        }
        
        // ListHooks: 直接读取 HookHandle
        Request::ListHooks => {
            let guard = state.hook_handle.read().await;
            match guard.as_ref() {
                Some(handle) => {
                    let entries = handle.list().await;
                    let hooks: Vec<lianwall_core::socket::HookInfo> = entries
                        .iter()
                        .map(|e| lianwall_core::socket::HookInfo {
                            name: e.display_name(),
                            on: e.on.to_string(),
                            command: e.command.clone(),
                            mode: e.mode.clone(),
                            trigger: e.trigger.clone(),
                            timeout: e.timeout,
                            enabled: e.enabled,
                        })
                        .collect();
                    Some(Response::HookList(hooks))
                }
                None => Some(Response::error(
                    ErrorCode::InternalError,
                    "Hook system not initialized",
                )),
            }
        }
        
        // Subscribe: 管理订阅状态
        Request::Subscribe { events, immediate_sync } => {
            // 展开 All 为所有具体事件类型
            let expanded_events = EventType::expand(events);
            conn_state.subscribed_events = expanded_events.iter().cloned().collect();
            
            conn_state.subscribed = true;
            conn_state.event_rx = Some(event_bus.subscribe());
            
            // 保存 immediate_sync 标志，稍后处理
            conn_state.pending_sync = *immediate_sync;
            
            Some(Response::Subscribed {
                session_id: format!("conn-{}", conn_state.id),
                subscribed_events: expanded_events,
            })
        }
        
        Request::Unsubscribe => {
            conn_state.subscribed = false;
            conn_state.event_rx = None;
            Some(Response::Unsubscribed)
        }
        
        // Command: 发送到命令队列
        _ => {
            let (response_tx, response_rx) = oneshot::channel();
            
            let msg = CommandMsg {
                request,
                response_tx,
            };
            
            // 获取超时时间
            let timeout = get_request_timeout(&msg.request);
            
            // 发送到命令队列
            if cmd_tx.send(msg).await.is_err() {
                return Some(Response::error(ErrorCode::InternalError, "Command queue closed"));
            }
            
            // 等待响应（带超时）
            match tokio::time::timeout(timeout, response_rx).await {
                Ok(Ok(response)) => Some(response),
                Ok(Err(_)) => Some(Response::error(ErrorCode::InternalError, "Response channel dropped")),
                Err(_) => Some(Response::error(ErrorCode::Timeout, "Command timeout")),
            }
        }
    }
}

/// 获取请求超时时间
///
/// 设计决策：不同命令设置不同超时
fn get_request_timeout(request: &Request) -> Duration {
    match request {
        // Query: 快速响应
        Request::Ping | Request::GetStatus | Request::GetConfig { .. } => Duration::from_secs(2),
        Request::GetSpace { .. } | Request::GetTimeInfo => Duration::from_secs(5),
        
        // Command: 根据操作类型
        Request::Next { .. } | Request::Prev { .. } | Request::SetWallpaper { .. } => Duration::from_secs(5),
        Request::SetMode { .. } => Duration::from_secs(10),
        Request::Lock { .. } | Request::Unlock { .. } | Request::ToggleLock { .. } => Duration::from_secs(2),
        Request::SetConfig { .. } | Request::ReloadConfig => Duration::from_secs(5),
        Request::ReloadHooks => Duration::from_secs(5),
        Request::ListHooks => Duration::from_secs(2),
        Request::Rescan => Duration::from_secs(60), // 大目录可能很慢
        Request::Shutdown => Duration::from_secs(10),
        Request::VramOverride { .. } => Duration::from_secs(10),
        
        // Subscribe: 无需长超时
        Request::Subscribe { .. } | Request::Unsubscribe => Duration::from_secs(5),
    }
}

/// 将内部事件映射到 EventType（用于订阅过滤）
fn event_to_type(event: &Event) -> EventType {
    match event {
        Event::WallpaperChanged { .. } => EventType::WallpaperChanged,
        Event::ModeChanged { .. } => EventType::StatusChanged,
        Event::EngineStateChanged { .. } => EventType::StatusChanged,
        Event::SpaceUpdated { .. } => EventType::SpaceUpdated,
        Event::ScanProgress { .. } => EventType::ScanProgress,
        Event::ConfigChanged { .. } => EventType::ConfigChanged,
        Event::GpuStateUpdated { .. } => EventType::VramChanged,
        Event::TimePointReached { .. } => EventType::TimePointReached,
        Event::Error { .. } => EventType::Error,
        Event::ShuttingDown => EventType::Error,
        Event::SchedulerTick => EventType::Error, // 内部事件，不会推送
    }
}

/// 将内部事件转换为客户端响应
fn event_to_response(event: &Event) -> Option<Response> {
    use lianwall_core::socket::{Event as SocketEvent, SpaceUpdateReason as SocketSpaceUpdateReason, SpaceSummary};
    
    match event {
        Event::WallpaperChanged { path, mode, trigger } => {
            let filename = path.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            Some(Response::Event(SocketEvent::WallpaperChanged {
                path: path.clone(),
                filename,
                mode: *mode,
                trigger: *trigger,
            }))
        }
        Event::ModeChanged { from: _, to } => {
            Some(Response::Event(SocketEvent::StatusChanged {
                changes: vec![lianwall_core::socket::StatusChange::Mode(*to)],
            }))
        }
        Event::SpaceUpdated { reason, mode, total, available, locked, in_cooldown } => {
            let socket_reason = match reason {
                SpaceUpdateReason::InitialScan | SpaceUpdateReason::Rescan => SocketSpaceUpdateReason::Rescanned,
                SpaceUpdateReason::FileChange => SocketSpaceUpdateReason::ConfigChanged,
                SpaceUpdateReason::LockChange => SocketSpaceUpdateReason::LockChanged,
            };
            Some(Response::Event(SocketEvent::SpaceUpdated {
                mode: *mode,
                reason: socket_reason,
                summary: SpaceSummary {
                    total: *total,
                    available: *available,
                    locked: *locked,
                    in_cooldown: *in_cooldown,
                },
            }))
        }
        Event::ScanProgress { mode, scanned, files_found, current_dir: _ } => {
            Some(Response::Event(SocketEvent::ScanProgress {
                mode: *mode,
                dirs_scanned: *scanned,
                files_found: *files_found,
                completed: false,
            }))
        }
        Event::ConfigChanged { key, old_value, new_value } => {
            Some(Response::Event(SocketEvent::ConfigChanged {
                key: key.clone(),
                old_value: old_value.clone(),
                new_value: new_value.clone(),
            }))
        }
        Event::EngineStateChanged { swww_running, mpvpaper_running } => {
            let engine = if *mpvpaper_running {
                "mpvpaper"
            } else if *swww_running {
                "awww"
            } else {
                "none"
            };
            Some(Response::Event(SocketEvent::StatusChanged {
                changes: vec![lianwall_core::socket::StatusChange::Engine(engine.to_string())],
            }))
        }
        Event::GpuStateUpdated { action, vram_info } => {
            // Keep 不需要通知客户端，只有 Downgrade/Upgrade 才发送事件
            if *action == lianwall_core::gpu::VramAction::Keep {
                return None;
            }
            
            if let Some(info) = vram_info {
                Some(Response::Event(SocketEvent::VramChanged {
                    action: match action {
                        lianwall_core::gpu::VramAction::Downgrade => lianwall_core::socket::VramAction::Downgrade,
                        lianwall_core::gpu::VramAction::Upgrade => lianwall_core::socket::VramAction::Upgrade,
                        lianwall_core::gpu::VramAction::Keep => unreachable!(), // 已在上面过滤
                    },
                    used_mb: info.used_mb,
                    total_mb: info.total_mb,
                    free_percent: info.free_percent,
                }))
            } else {
                None
            }
        }
        Event::Error { message } => {
            Some(Response::Event(SocketEvent::Error {
                code: ErrorCode::InternalError,
                message: message.clone(),
                recoverable: true,
            }))
        }
        Event::ShuttingDown => {
            // Shutdown 没有特定的 Event 类型，使用 Error 事件代替
            Some(Response::Event(SocketEvent::Error {
                code: ErrorCode::InternalError,
                message: "Daemon shutting down".to_string(),
                recoverable: false,
            }))
        }
        // 时间点到达事件
        Event::TimePointReached { time, next_time } => {
            Some(Response::Event(SocketEvent::TimePointReached {
                time: time.clone(),
                next_time: next_time.clone(),
            }))
        }
        // 内部事件不推送给客户端
        Event::SchedulerTick => None,
    }
}

/// 发送响应
async fn send_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    response: &Response,
) -> anyhow::Result<()> {
    let json = serde_json::to_string(response)?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}
