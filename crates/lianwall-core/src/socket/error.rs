//! Socket 通信错误定义

use std::io;
use std::path::PathBuf;
use thiserror::Error;

/// Socket 通信错误
#[derive(Debug, Error)]
pub enum SocketError {
    /// 连接失败
    #[error("无法连接到守护进程: {path}")]
    ConnectFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// 绑定失败
    #[error("无法绑定 Socket: {path}")]
    BindFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// Socket 已存在（守护进程可能已在运行）
    #[error("Socket 已存在: {path}，守护进程可能已在运行")]
    SocketExists { path: PathBuf },

    /// 发送失败
    #[error("发送数据失败")]
    SendFailed(#[source] io::Error),

    /// 接收失败
    #[error("接收数据失败")]
    RecvFailed(#[source] io::Error),

    /// 连接已关闭
    #[error("连接已关闭")]
    ConnectionClosed,

    /// 消息过大
    #[error("消息过大: {size} 字节 (最大 {max} 字节)")]
    MessageTooLarge { size: usize, max: usize },

    /// 序列化失败
    #[error("序列化失败")]
    SerializeFailed(#[source] serde_json::Error),

    /// 反序列化失败
    #[error("反序列化失败: {context}")]
    DeserializeFailed {
        context: String,
        #[source]
        source: serde_json::Error,
    },

    /// 超时
    #[error("操作超时")]
    Timeout,

    /// 守护进程未运行
    #[error("守护进程未运行")]
    DaemonNotRunning,

    /// 协议版本不匹配
    #[error("协议版本不匹配: 客户端 {client}, 服务端 {server}")]
    VersionMismatch { client: u32, server: u32 },
}
