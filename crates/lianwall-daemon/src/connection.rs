//! Connection - 单个客户端连接处理
//!
//! 每个连接一个 Task，负责：
//! - 读取请求
//! - 根据请求类型分发处理
//! - 管理订阅状态
//! - 发送响应和事件

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinSet;

use lianwall_core::socket::{
    ErrorCode, EventType, Request, Response, extract_request_id, response_frame,
};

use crate::command::CommandMsg;
use crate::event::{Event, EventBus, SpaceUpdateReason};
use crate::handler;
use crate::state::SharedState;

const MAX_PENDING_COMMANDS: usize = 64;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

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
    /// Response waiters are owned by the connection and aborted on disconnect.
    pending_commands: JoinSet<PendingResponse>,
    next_legacy_request: u64,
    next_legacy_response: u64,
    legacy_responses: BTreeMap<u64, PendingResponse>,
}

struct PendingResponse {
    id: u64,
    legacy_order: Option<u64>,
    response: Response,
    immediate_sync: bool,
}

impl PendingResponse {
    fn new(id: u64, legacy_order: Option<u64>, response: Response) -> Self {
        Self {
            id,
            legacy_order,
            response,
            immediate_sync: false,
        }
    }
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
    // Keep partial frames outside the cancellation-safe reader future.
    let mut reader = BufReader::new(reader);
    let mut partial_frame = Vec::new();
    let mut conn_state = ConnectionState {
        id,
        subscribed: false,
        subscribed_events: HashSet::new(),
        event_rx: None,
        pending_commands: JoinSet::new(),
        next_legacy_request: 0,
        next_legacy_response: 0,
        legacy_responses: BTreeMap::new(),
    };

    let mut shutdown = state.shutdown_receiver();
    let mut closing = state.is_shutting_down();
    loop {
        if closing && conn_state.pending_commands.is_empty() {
            break;
        }
        tokio::select! {
            _ = shutdown.recv(), if !closing => { closing = true; }
            result = read_frame(&mut reader, &mut partial_frame, MAX_REQUEST_BYTES), if !closing => {
                match result {
                    Ok(None) => break,
                    Ok(Some(line)) => {
                        if let Some(response) = process_request(
                            &line, &state, &event_bus, &cmd_tx, &mut conn_state,
                        ).await {
                            deliver_response(&mut writer, &state, &mut conn_state, response).await?;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Connection #{} read error: {}", id, e);
                        break;
                    }
                }
            }
            result = conn_state.pending_commands.join_next(), if !conn_state.pending_commands.is_empty() => {
                if let Some(result) = result {
                    deliver_response(&mut writer, &state, &mut conn_state, result?).await?;
                }
            }
            result = receive_event(&mut conn_state.event_rx), if conn_state.subscribed => {
                match result {
                    Ok(event) => {
                        if conn_state.subscribed_events.contains(&event_to_type(&event)) {
                            let response = if matches!(event, Event::SchedulerTick) {
                                Some(handler::handle_query(&state, Request::GetStatus).await)
                            } else {
                                event_to_response(&event)
                            };
                            if let Some(response) = response {
                                send_response(&mut writer, 0, &response).await?;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Connection #{} lagged {} events; closing so the client can resynchronize", id, n);
                        break;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    // JoinSet aborts pending response waiters when the connection is dropped.
    // Commands already queued keep running: a disconnected client is not cancellation.
    Ok(())
}

async fn receive_event(
    receiver: &mut Option<broadcast::Receiver<Event>>,
) -> Result<Event, broadcast::error::RecvError> {
    match receiver {
        Some(receiver) => receiver.recv().await,
        None => std::future::pending().await,
    }
}

/// 处理单个请求
///
/// Immediate replies are returned; queued commands register a connection-owned waiter.
/// Tagged replies can finish independently; id-less replies retain their read order.
/// 旧客户端不带 id 时按 0 处理。
async fn process_request(
    line: &str,
    state: &Arc<SharedState>,
    event_bus: &EventBus,
    cmd_tx: &mpsc::Sender<CommandMsg>,
    conn_state: &mut ConnectionState,
) -> Option<PendingResponse> {
    // 先按 Value 解析，提取请求 id，再反序列化为 Request
    let raw: serde_json::Value = match serde_json::from_str(line.trim()) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Connection #{} parse error: {}", conn_state.id, e);
            let order = conn_state.next_legacy_request;
            conn_state.next_legacy_request += 1;
            return Some(PendingResponse::new(
                0,
                Some(order),
                Response::error(ErrorCode::InvalidRequest, format!("Parse error: {}", e)),
            ));
        }
    };
    let id = extract_request_id(&raw);
    let legacy_order = if id == 0 {
        let order = conn_state.next_legacy_request;
        conn_state.next_legacy_request += 1;
        Some(order)
    } else {
        None
    };
    let mut sync_requested = false;
    let request: Request = match serde_json::from_value(raw) {
        Ok(req) => req,
        Err(e) => {
            tracing::warn!("Connection #{} parse error: {}", conn_state.id, e);
            return Some(PendingResponse::new(
                id,
                legacy_order,
                Response::error(ErrorCode::InvalidRequest, format!("Parse error: {}", e)),
            ));
        }
    };

    tracing::debug!("Connection #{} request: {:?}", conn_state.id, request);

    // 根据请求类型分发
    let response: Option<Response> = match &request {
        // Query: 直接读取状态
        Request::Ping => Some(Response::Pong {
            uptime_secs: state.uptime_secs(),
            protocol_version: lianwall_core::socket::PROTOCOL_VERSION,
        }),

        Request::GetStatus
        | Request::GetConfig { .. }
        | Request::GetSpace { .. }
        | Request::GetTimeInfo
        | Request::GetRotationStatus => Some(handler::handle_query(state, request).await),

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
        Request::Subscribe {
            events,
            immediate_sync,
        } => {
            // 展开 All 为所有具体事件类型
            let expanded_events = EventType::expand(events);
            conn_state.subscribed_events = expanded_events.iter().cloned().collect();

            conn_state.subscribed = true;
            conn_state.event_rx = Some(event_bus.subscribe());

            // 保存 immediate_sync 标志，稍后处理
            sync_requested = *immediate_sync;

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
            let cmd_name = request.name();
            // 壁纸切换的权威结果是状态变更 + WallpaperChanged 事件。
            // 等待回执超时只反映引擎性能，不能当成业务失败。
            let wait_for_completion = is_wallpaper_mutation(&request);

            let (response_tx, response_rx) = oneshot::channel();
            let msg = CommandMsg {
                request,
                response_tx,
            };

            if conn_state.pending_commands.len() >= MAX_PENDING_COMMANDS {
                return Some(PendingResponse::new(
                    id,
                    legacy_order,
                    Response::error(
                        ErrorCode::InternalError,
                        "Too many pending commands; request was not queued",
                    ),
                ));
            }
            // Enqueue in read order without blocking the event pump on a full queue.
            if let Err(error) = cmd_tx.try_send(msg) {
                let (code, message) = match error {
                    mpsc::error::TrySendError::Full(_) => (
                        ErrorCode::InternalError,
                        "Command queue is full; request was not queued",
                    ),
                    mpsc::error::TrySendError::Closed(_) => {
                        (ErrorCode::InternalError, "Command queue closed")
                    }
                };
                return Some(PendingResponse::new(
                    id,
                    legacy_order,
                    Response::error(code, message),
                ));
            }

            conn_state.pending_commands.spawn(async move {
                let response = if wait_for_completion {
                    wait_wallpaper_command(cmd_name, response_rx).await
                } else {
                    let timeout = get_request_timeout_by_name(cmd_name);
                    match tokio::time::timeout(timeout, response_rx).await {
                        Ok(Ok(response)) => response,
                        Ok(Err(_)) => {
                            Response::error(ErrorCode::InternalError, "Response channel dropped")
                        }
                        Err(_) => Response::error(ErrorCode::Timeout, "Command timeout"),
                    }
                };
                PendingResponse::new(id, legacy_order, response)
            });
            None
        }
    };

    response.map(|response| PendingResponse {
        id,
        legacy_order,
        response,
        immediate_sync: sync_requested,
    })
}

/// Keep v2/id-less replies in FIFO order while tagged v3 requests may finish independently.
async fn deliver_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    state: &Arc<SharedState>,
    connection: &mut ConnectionState,
    response: PendingResponse,
) -> anyhow::Result<()> {
    if let Some(order) = response.legacy_order {
        connection.legacy_responses.insert(order, response);
        while let Some(response) = connection
            .legacy_responses
            .remove(&connection.next_legacy_response)
        {
            connection.next_legacy_response += 1;
            write_pending_response(writer, state, response).await?;
        }
        anyhow::ensure!(
            connection.legacy_responses.len() <= MAX_PENDING_COMMANDS,
            "Too many buffered legacy responses",
        );
    } else {
        write_pending_response(writer, state, response).await?;
    }
    Ok(())
}

async fn write_pending_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    state: &Arc<SharedState>,
    response: PendingResponse,
) -> anyhow::Result<()> {
    send_response(writer, response.id, &response.response).await?;
    if response.immediate_sync {
        let status = handler::handle_query(state, Request::GetStatus).await;
        send_response(writer, 0, &status).await?;
    }
    Ok(())
}

/// 壁纸类命令：等真正完成；慢只记性能日志，不回 Timeout 业务错误
async fn wait_wallpaper_command(
    cmd_name: &'static str,
    response_rx: oneshot::Receiver<Response>,
) -> Response {
    /// 超过该阈值只打 warn，不中断等待
    const SLOW_THRESHOLD: Duration = Duration::from_secs(5);
    const PROBE_INTERVAL: Duration = Duration::from_secs(5);

    let start = Instant::now();
    let mut probe = tokio::time::interval(PROBE_INTERVAL);
    probe.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // 跳过 interval 的立即首 tick，避免一进等待就打日志
    probe.tick().await;

    tokio::pin!(response_rx);

    loop {
        tokio::select! {
            result = &mut response_rx => {
                let elapsed = start.elapsed();
                if elapsed > SLOW_THRESHOLD {
                    tracing::warn!(
                        command = cmd_name,
                        elapsed_ms = elapsed.as_millis() as u64,
                        "Wallpaper command finished slowly (perf signal, not a business failure)"
                    );
                } else {
                    tracing::debug!(
                        command = cmd_name,
                        elapsed_ms = elapsed.as_millis() as u64,
                        "Wallpaper command finished"
                    );
                }
                return match result {
                    Ok(response) => response,
                    Err(_) => Response::error(
                        ErrorCode::InternalError,
                        "Response channel dropped",
                    ),
                };
            }
            _ = probe.tick() => {
                tracing::info!(
                    command = cmd_name,
                    elapsed_ms = start.elapsed().as_millis() as u64,
                    "Wallpaper command still in progress (perf probe)"
                );
            }
        }
    }
}

fn is_wallpaper_mutation(request: &Request) -> bool {
    matches!(
        request,
        Request::Next { .. }
            | Request::Prev { .. }
            | Request::SetWallpaper { .. }
            | Request::SetMode { .. }
    )
}

/// 非壁纸命令的回执等待上限（壁纸类走 wait_wallpaper_command）
fn get_request_timeout_by_name(cmd_name: &str) -> Duration {
    match cmd_name {
        "Ping" | "GetStatus" | "GetConfig" => Duration::from_secs(2),
        "GetSpace" | "GetTimeInfo" => Duration::from_secs(5),
        "Lock" | "Unlock" | "ToggleLock" | "ListHooks" => Duration::from_secs(2),
        "SetConfig" | "ReloadConfig" | "ReloadHooks" | "Subscribe" | "Unsubscribe" => {
            Duration::from_secs(5)
        }
        "Rescan" => Duration::from_secs(60),
        "Shutdown" | "VramOverride" => Duration::from_secs(10),
        _ => Duration::from_secs(10),
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
        Event::SchedulerTick => EventType::StatusChanged,
    }
}

/// 将内部事件转换为客户端响应
fn event_to_response(event: &Event) -> Option<Response> {
    use lianwall_core::socket::{
        Event as SocketEvent, SpaceSummary, SpaceUpdateReason as SocketSpaceUpdateReason,
    };

    match event {
        Event::WallpaperChanged {
            path,
            mode,
            trigger,
        } => {
            let filename = path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            Some(Response::Event(SocketEvent::WallpaperChanged {
                path: path.clone(),
                filename,
                mode: *mode,
                trigger: *trigger,
            }))
        }
        Event::ModeChanged { from: _, to } => Some(Response::Event(SocketEvent::StatusChanged {
            changes: vec![lianwall_core::socket::StatusChange::Mode(*to)],
        })),
        Event::SpaceUpdated {
            reason,
            mode,
            total,
            available,
            locked,
            in_cooldown,
        } => {
            let socket_reason = match reason {
                SpaceUpdateReason::InitialScan | SpaceUpdateReason::Rescan => {
                    SocketSpaceUpdateReason::Rescanned
                }
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
        Event::ScanProgress {
            mode,
            scanned,
            files_found,
            current_dir: _,
        } => Some(Response::Event(SocketEvent::ScanProgress {
            mode: *mode,
            dirs_scanned: *scanned,
            files_found: *files_found,
            completed: false,
        })),
        Event::ConfigChanged {
            key,
            old_value,
            new_value,
        } => Some(Response::Event(SocketEvent::ConfigChanged {
            key: key.clone(),
            old_value: old_value.clone(),
            new_value: new_value.clone(),
        })),
        Event::EngineStateChanged {
            swww_running,
            mpvpaper_running,
        } => {
            let engine = if *mpvpaper_running {
                "mpvpaper"
            } else if *swww_running {
                "awww"
            } else {
                "none"
            };
            Some(Response::Event(SocketEvent::StatusChanged {
                changes: vec![lianwall_core::socket::StatusChange::Engine(
                    engine.to_string(),
                )],
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
                        lianwall_core::gpu::VramAction::Downgrade => {
                            lianwall_core::socket::VramAction::Downgrade
                        }
                        lianwall_core::gpu::VramAction::Upgrade => {
                            lianwall_core::socket::VramAction::Upgrade
                        }
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
        Event::Error { message } => Some(Response::Event(SocketEvent::Error {
            code: ErrorCode::InternalError,
            message: message.clone(),
            recoverable: true,
        })),
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

/// 发送响应（响应帧回带请求 id；主动推送用 id=0）
async fn send_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    id: u64,
    response: &Response,
) -> anyhow::Result<()> {
    let mut json = response_frame(id, response).to_string();
    json.push('\n');
    write_frame(writer, json.as_bytes(), WRITE_TIMEOUT).await?;
    Ok(())
}

/// fill_buf is cancellation safe; consumed bytes live in the connection-owned buffer.
async fn read_frame(
    reader: &mut (impl AsyncBufRead + Unpin),
    partial: &mut Vec<u8>,
    limit: usize,
) -> std::io::Result<Option<String>> {
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            if partial.is_empty() {
                return Ok(None);
            }
            return String::from_utf8(std::mem::take(partial))
                .map(Some)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error));
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.unwrap_or(available.len());
        if partial.len() + count > limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Request exceeds 1 MiB frame limit",
            ));
        }
        partial.extend_from_slice(&available[..count]);
        reader.consume(count + usize::from(newline.is_some()));
        if newline.is_some() {
            return String::from_utf8(std::mem::take(partial))
                .map(Some)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error));
        }
    }
}

async fn write_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    bytes: &[u8],
    timeout: Duration,
) -> std::io::Result<()> {
    tokio::time::timeout(timeout, async {
        writer.write_all(bytes).await?;
        writer.flush().await
    })
    .await
    .map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "Client stopped reading responses",
        )
    })?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::Lines;
    use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

    #[tokio::test]
    async fn malformed_legacy_frame_waits_behind_earlier_command_reply() {
        let (mut client, _events, mut commands, task) = fixture().await;
        client.send("{\"cmd\":\"Next\"}\n{]\n").await;
        let command = commands.recv().await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(30), client.reader.next_line())
                .await
                .is_err()
        );
        command.response_tx.send(Response::Ok).unwrap();
        assert_eq!(client.receive().await["type"], "Ok");
        assert_eq!(client.receive().await["type"], "Error");
        task.abort();
    }

    #[tokio::test]
    async fn frame_limit_rejects_oversize_without_newline_and_retains_cancelled_utf8() {
        let (mut writer, reader) = tokio::io::duplex(128);
        let mut reader = BufReader::new(reader);
        let mut partial = Vec::new();
        writer.write_all(&[0xe4]).await.unwrap();
        assert!(
            tokio::time::timeout(
                Duration::from_millis(20),
                read_frame(&mut reader, &mut partial, 8)
            )
            .await
            .is_err()
        );
        writer.write_all(&[0xb8, 0xad, b'\n']).await.unwrap();
        assert_eq!(
            read_frame(&mut reader, &mut partial, 8)
                .await
                .unwrap()
                .unwrap(),
            "中"
        );
        writer.write_all(b"123456789").await.unwrap();
        assert_eq!(
            read_frame(&mut reader, &mut partial, 8)
                .await
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidData
        );
    }

    #[tokio::test]
    async fn stalled_writer_expires_without_holding_connection_forever() {
        let (mut writer, _unread) = tokio::io::duplex(8);
        let error = write_frame(&mut writer, &[b'x'; 32], Duration::from_millis(20))
            .await
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    }

    struct Client {
        reader: Lines<BufReader<OwnedReadHalf>>,
        writer: OwnedWriteHalf,
    }

    impl Client {
        async fn send(&mut self, frame: &str) {
            self.writer.write_all(frame.as_bytes()).await.unwrap();
        }

        async fn receive(&mut self) -> serde_json::Value {
            let line = tokio::time::timeout(Duration::from_secs(1), self.reader.next_line())
                .await
                .expect("connection did not respond")
                .unwrap()
                .expect("connection closed unexpectedly");
            serde_json::from_str(&line).unwrap()
        }

        async fn subscribe(&mut self) {
            self.send(
                "{\"id\":1,\"cmd\":\"Subscribe\",\"events\":[\"all\"],\"immediate_sync\":false}\n",
            )
            .await;
            assert_eq!(self.receive().await["type"], "Subscribed");
        }
    }

    async fn fixture() -> (
        Client,
        EventBus,
        mpsc::Receiver<CommandMsg>,
        tokio::task::JoinHandle<anyhow::Result<()>>,
    ) {
        let config = toml::from_str(lianwall_core::config::DEFAULT_CONFIG_TOML).unwrap();
        let state = SharedState::init(config).await.unwrap();
        let event_bus = EventBus::new(16);
        let (tx, rx) = mpsc::channel(4);
        let (client, server) = UnixStream::pair().unwrap();
        let task = tokio::spawn(handle(1, server, state, event_bus.clone(), tx));
        let (reader, writer) = client.into_split();
        let client = Client {
            reader: BufReader::new(reader).lines(),
            writer,
        };
        (client, event_bus, rx, task)
    }

    #[tokio::test]
    async fn test_fragmented_request_survives_event_push() {
        let (mut client, bus, _commands, task) = fixture().await;
        client.subscribe().await;
        client.send("{\"id\":2,\"cmd\":\"Pi").await;
        // Let the server consume the first fragment before an event cancels its read.
        tokio::time::sleep(Duration::from_millis(20)).await;
        bus.publish(Event::Error {
            message: "between fragments".into(),
        });
        assert_eq!(client.receive().await["type"], "Event");
        client.send("ng\"}\n").await;
        let response = client.receive().await;
        assert_eq!(response["id"], 2);
        assert_eq!(response["type"], "Pong");
        task.abort();
    }

    #[tokio::test]
    async fn test_pending_command_does_not_block_events_or_queries() {
        let (mut client, bus, mut commands, task) = fixture().await;
        client.subscribe().await;
        client.send("{\"id\":2,\"cmd\":\"Next\"}\n").await;
        let command = commands.recv().await.unwrap();
        bus.publish(Event::Error {
            message: "command is still running".into(),
        });
        assert_eq!(client.receive().await["type"], "Event");
        client.send("{\"id\":3,\"cmd\":\"Ping\"}\n").await;
        let response = client.receive().await;
        assert_eq!(response["id"], 3);
        assert_eq!(response["type"], "Pong");
        command.response_tx.send(Response::ok()).unwrap();
        let response = client.receive().await;
        assert_eq!(response["id"], 2);
        assert_eq!(response["type"], "Ok");
        task.abort();
    }

    #[tokio::test]
    async fn test_commands_enter_queue_in_request_order() {
        let (mut client, _bus, mut commands, task) = fixture().await;
        client
            .send("{\"id\":2,\"cmd\":\"Next\"}\n{\"id\":3,\"cmd\":\"Prev\"}\n")
            .await;
        let first = commands.recv().await.unwrap();
        let second = tokio::time::timeout(Duration::from_secs(1), commands.recv())
            .await
            .expect("second command was blocked by the first response")
            .unwrap();
        assert!(matches!(first.request, Request::Next { .. }));
        assert!(matches!(second.request, Request::Prev { .. }));
        // Responses may complete out of order; ids must remain attached to requests.
        second.response_tx.send(Response::ok()).unwrap();
        assert_eq!(client.receive().await["id"], 3);
        first.response_tx.send(Response::ok()).unwrap();
        assert_eq!(client.receive().await["id"], 2);
        task.abort();
    }

    #[tokio::test]
    async fn test_disconnect_releases_pending_response_waiters() {
        let (mut client, _bus, mut commands, task) = fixture().await;
        client.send("{\"id\":2,\"cmd\":\"Next\"}\n").await;
        let mut command = commands.recv().await.unwrap();
        drop(client);
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("connection kept waiting after client disconnect")
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), command.response_tx.closed())
            .await
            .expect("pending response waiter was not dropped");
    }

    #[tokio::test]
    async fn test_legacy_responses_keep_fifo_without_blocking_events() {
        let (mut client, bus, mut commands, task) = fixture().await;
        client.subscribe().await;
        client
            .send("{\"cmd\":\"Next\"}\n{\"cmd\":\"Prev\"}\n{\"cmd\":\"Ping\"}\n")
            .await;
        let first = commands.recv().await.unwrap();
        let second = tokio::time::timeout(Duration::from_secs(1), commands.recv())
            .await
            .unwrap()
            .unwrap();
        second.response_tx.send(Response::ok()).unwrap();
        bus.publish(Event::Error {
            message: "legacy event".into(),
        });
        // Neither the second reply nor Ping may overtake the first legacy command.
        assert_eq!(client.receive().await["type"], "Event");
        first
            .response_tx
            .send(Response::error(ErrorCode::EmptySpace, "first command"))
            .unwrap();
        let first = client.receive().await;
        assert_eq!(first["id"], 0);
        assert_eq!(first["type"], "Error");
        assert_eq!(client.receive().await["type"], "Ok");
        assert_eq!(client.receive().await["type"], "Pong");
        task.abort();
    }

    #[tokio::test]
    async fn test_full_command_queue_rejects_without_blocking_queries() {
        let (mut client, _bus, mut commands, task) = fixture().await;
        for id in 1..=5 {
            client
                .send(&format!("{{\"id\":{id},\"cmd\":\"Next\"}}\n"))
                .await;
        }
        let rejected = client.receive().await;
        assert_eq!(rejected["id"], 5);
        assert_eq!(rejected["type"], "Error");
        assert_eq!(
            rejected["payload"]["message"],
            "Command queue is full; request was not queued"
        );
        client.send("{\"id\":6,\"cmd\":\"Ping\"}\n").await;
        assert_eq!(client.receive().await["type"], "Pong");
        for _ in 0..4 {
            let command = commands.recv().await.unwrap();
            command.response_tx.send(Response::ok()).unwrap();
        }
        assert!(commands.try_recv().is_err());
        task.abort();
    }

    #[tokio::test]
    async fn test_pending_command_limit_keeps_queries_available() {
        let (mut client, _bus, mut commands, task) = fixture().await;
        let mut running = Vec::new();
        for id in 1..=MAX_PENDING_COMMANDS {
            client
                .send(&format!("{{\"id\":{id},\"cmd\":\"Next\"}}\n"))
                .await;
            running.push(commands.recv().await.unwrap());
        }
        client.send("{\"id\":100,\"cmd\":\"Next\"}\n").await;
        let rejected = client.receive().await;
        assert_eq!(rejected["id"], 100);
        assert_eq!(
            rejected["payload"]["message"],
            "Too many pending commands; request was not queued"
        );
        assert!(commands.try_recv().is_err());
        client.send("{\"id\":101,\"cmd\":\"Ping\"}\n").await;
        assert_eq!(client.receive().await["type"], "Pong");
        task.abort();
    }

    #[tokio::test]
    async fn test_subscription_ack_precedes_optional_sync() {
        let (mut client, _bus, _commands, task) = fixture().await;
        client
            .send("{\"id\":1,\"cmd\":\"Subscribe\",\"events\":[\"all\"],\"immediate_sync\":true}\n")
            .await;
        let ack = client.receive().await;
        assert_eq!(ack["id"], 1);
        assert_eq!(ack["type"], "Subscribed");
        let status = client.receive().await;
        assert_eq!(status["id"], 0);
        assert_eq!(status["type"], "Status");
        client.send("{\"id\":2,\"cmd\":\"Subscribe\",\"events\":[\"all\"],\"immediate_sync\":false}\n{\"id\":3,\"cmd\":\"Ping\"}\n").await;
        assert_eq!(client.receive().await["type"], "Subscribed");
        assert_eq!(client.receive().await["type"], "Pong");
        task.abort();
    }

    #[tokio::test]
    async fn test_dropped_command_response_retains_request_id() {
        let (mut client, _bus, mut commands, task) = fixture().await;
        client.send("{\"id\":7,\"cmd\":\"Next\"}\n").await;
        drop(commands.recv().await.unwrap());
        let response = client.receive().await;
        assert_eq!(response["id"], 7);
        assert_eq!(response["type"], "Error");
        client.send("{\"id\":8,\"cmd\":\"Ping\"}\n").await;
        assert_eq!(client.receive().await["type"], "Pong");
        task.abort();
    }
    #[tokio::test]
    async fn test_schedule_update_pushes_status_to_subscribers() {
        let (mut client, bus, _commands, task) = fixture().await;
        client.subscribe().await;
        bus.publish(Event::SchedulerTick);
        let response = client.receive().await;
        assert_eq!(response["id"], 0);
        assert_eq!(response["type"], "Status");
        assert!(response["payload"]["next_switch_secs"].is_number());
        task.abort();
    }
    #[tokio::test]
    async fn test_lagged_subscription_disconnects_for_resynchronization() {
        let (mut client, bus, _commands, task) = fixture().await;
        client.subscribe().await;
        for _ in 0..100 {
            bus.publish(Event::Error {
                message: "burst".into(),
            });
        }
        let line = tokio::time::timeout(Duration::from_secs(1), client.reader.next_line())
            .await
            .unwrap()
            .unwrap();
        assert!(
            line.is_none(),
            "lagged clients must not silently receive an incomplete stream"
        );
        task.await.unwrap().unwrap();
    }
}
