//! Socket Server - Unix Socket 服务器
//!
//! 负责：
//! - 监听 Unix Socket
//! - 接受新连接
//! - 为每个连接 spawn Connection Task

use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::sync::mpsc;

use crate::command::CommandMsg;
use crate::connection;
use crate::event::EventBus;
use crate::state::SharedState;

/// 运行 Socket 服务器
pub async fn run(
    state: Arc<SharedState>,
    event_bus: EventBus,
    cmd_tx: mpsc::Sender<CommandMsg>,
) -> anyhow::Result<()> {
    // 从配置获取 socket 路径
    let socket_path = {
        let config = state.config.read().await;
        config.daemon.socket_path.clone()
    };
    
    // 确保 socket 目录存在
    if let Some(parent) = socket_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    
    // 删除可能存在的旧 socket 文件
    let _ = tokio::fs::remove_file(&socket_path).await;
    
    // 绑定 socket
    let listener = UnixListener::bind(&socket_path)?;
    tracing::info!("Server listening on {:?}", socket_path);
    
    // 连接 ID 计数器
    let mut conn_id: u64 = 0;
    
    // 获取 shutdown 信号
    let mut shutdown_rx = state.shutdown_receiver();
    
    loop {
        tokio::select! {
            // 接受新连接
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        conn_id += 1;
                        let id = conn_id;
                        
                        tracing::debug!("New connection: #{}", id);
                        
                        // Spawn connection task
                        let state = Arc::clone(&state);
                        let event_bus = event_bus.clone();
                        let cmd_tx = cmd_tx.clone();
                        
                        tokio::spawn(async move {
                            if let Err(e) = connection::handle(id, stream, state, event_bus, cmd_tx).await {
                                tracing::warn!("Connection #{} error: {}", id, e);
                            }
                            tracing::debug!("Connection #{} closed", id);
                        });
                    }
                    Err(e) => {
                        tracing::error!("Accept error: {}", e);
                    }
                }
            }
            
            // 收到关闭信号
            _ = shutdown_rx.recv() => {
                tracing::info!("Server shutting down");
                break;
            }
        }
    }
    
    // 清理 socket 文件
    let _ = tokio::fs::remove_file(&socket_path).await;
    
    Ok(())
}
