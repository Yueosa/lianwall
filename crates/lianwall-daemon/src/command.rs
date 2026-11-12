//! Command Queue - 命令队列
//!
//! 所有修改状态的命令都通过这个队列串行执行，保证状态一致性。
//!
//! 架构：
//! - Connection Task 发送 CommandMsg 到队列
//! - CommandQueue Task 串行处理每个命令
//! - 处理完成后通过 oneshot channel 返回结果

use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

use lianwall_core::socket::{Request, Response};

use crate::event::EventBus;
use crate::handler;
use crate::state::SharedState;

/// 命令消息
pub struct CommandMsg {
    /// 请求内容
    pub request: Request,
    /// 响应通道
    pub response_tx: oneshot::Sender<Response>,
}

/// 命令队列
pub struct CommandQueue {
    sender: mpsc::Sender<CommandMsg>,
}

impl CommandQueue {
    /// 创建命令队列
    ///
    /// 返回 (队列句柄, 接收端)
    pub fn new(buffer_size: usize) -> (Self, mpsc::Receiver<CommandMsg>) {
        let (sender, receiver) = mpsc::channel(buffer_size);
        (Self { sender }, receiver)
    }

    /// 发送命令并等待响应
    pub async fn send(&self, request: Request) -> Result<Response, CommandError> {
        let (response_tx, response_rx) = oneshot::channel();

        let msg = CommandMsg {
            request,
            response_tx,
        };

        self.sender
            .send(msg)
            .await
            .map_err(|_| CommandError::QueueClosed)?;

        response_rx.await.map_err(|_| CommandError::ResponseDropped)
    }

    /// 获取 sender clone（用于传递给其他组件）
    pub fn sender(&self) -> mpsc::Sender<CommandMsg> {
        self.sender.clone()
    }
}

impl Clone for CommandQueue {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

/// 命令错误
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Command queue closed")]
    QueueClosed,

    #[error("Response channel dropped")]
    ResponseDropped,
}

/// 运行命令队列处理 Task
///
/// 这个 Task 负责：
/// 1. 从队列接收命令
/// 2. 调用 handler 处理命令
/// 3. 发布相关事件
/// 4. 返回响应
pub async fn run(
    state: Arc<SharedState>,
    event_bus: EventBus,
    mut receiver: mpsc::Receiver<CommandMsg>,
) {
    tracing::info!("Command queue started");

    let mut shutdown = state.shutdown_receiver();
    while !state.is_shutting_down() {
        let msg = tokio::select! {
            biased;
            _ = shutdown.recv() => break,
            message = receiver.recv() => match message { Some(message) => message, None => break },
        };
        let CommandMsg {
            request,
            response_tx,
        } = msg;

        tracing::debug!("Processing command: {:?}", request);

        // 处理命令
        let response = handler::handle_command(&state, &event_bus, request).await;

        // 发送响应（忽略发送失败，可能客户端已断开）
        let _ = response_tx.send(response);
    }

    receiver.close();
    while let Ok(message) = receiver.try_recv() {
        let _ = message.response_tx.send(Response::error(
            lianwall_core::socket::ErrorCode::InternalError,
            "Daemon shutting down; request was not executed",
        ));
    }
    tracing::info!("Command queue stopped");
}

#[cfg(test)]
mod tests {
    use super::*;
    use lianwall_core::config::Config;

    fn test_config() -> Config {
        toml::from_str(lianwall_core::config::DEFAULT_CONFIG_TOML)
            .expect("Failed to parse default config")
    }

    #[tokio::test]
    async fn test_command_queue_ping() {
        let config = test_config();
        let state = SharedState::init(config).await.unwrap();
        let event_bus = EventBus::new(16);

        let (queue, receiver) = CommandQueue::new(16);

        // 启动处理 task
        tokio::spawn(run(state, event_bus, receiver));

        // 发送 Ping
        let response = queue.send(Request::Ping).await.unwrap();

        // Response::Pong 是带字段的变体
        assert!(matches!(response, Response::Pong { .. }));
    }

    #[tokio::test]
    async fn test_command_queue_closed() {
        let (queue, receiver) = CommandQueue::new(16);

        // 立即 drop receiver
        drop(receiver);

        // 发送应该失败
        let result = queue.send(Request::Ping).await;
        assert!(matches!(result, Err(CommandError::QueueClosed)));
    }
    #[tokio::test]
    async fn shutdown_stops_queue_even_when_senders_are_still_alive() {
        let state = SharedState::init(test_config()).await.unwrap();
        let (queue, receiver) = CommandQueue::new(4);
        // Trigger before run registers its receiver: the persistent shutdown flag matters.
        state.trigger_shutdown();
        let task = tokio::spawn(run(state, EventBus::new(4), receiver));
        tokio::time::timeout(std::time::Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            queue.send(Request::Ping).await,
            Err(CommandError::QueueClosed)
        ));
    }
}
