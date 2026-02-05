//! Socket 通信模块
//!
//! 提供守护进程与客户端之间的 Unix Socket 通信功能
//!
//! # 架构
//!
//! ```text
//! ┌─────────────┐                    ┌─────────────┐
//! │   Client    │  ── Request ──→    │   Server    │
//! │ (CLI/GUI)   │  ←─ Response ──    │  (Daemon)   │
//! └─────────────┘                    └─────────────┘
//! ```
//!
//! # 协议格式
//!
//! 使用长度前缀帧:
//! ```text
//! +----------------+------------------+
//! | 长度 (4 bytes) | JSON 数据        |
//! | u32 big-endian | UTF-8 字符串     |
//! +----------------+------------------+
//! ```
//!
//! # 模块结构
//!
//! - [`protocol`] - 请求/响应结构定义
//! - [`codec`] - 消息编解码
//! - [`client`] - 客户端 API（CLI/GUI 使用）
//! - [`server`] - 服务端 API（Daemon 使用）
//! - [`error`] - 错误类型
//!
//! # 使用示例
//!
//! ## 客户端
//! ```ignore
//! use lianwall_core::socket::{Client, quick};
//!
//! // 方式 1: 使用 Client 对象
//! let mut client = Client::connect("/tmp/lianwall.sock")?;
//! let status = client.status()?;
//! client.next()?;
//!
//! // 方式 2: 使用快捷函数
//! let status = quick::status("/tmp/lianwall.sock")?;
//! quick::next("/tmp/lianwall.sock")?;
//! ```
//!
//! ## 服务端
//! ```ignore
//! use lianwall_core::socket::{Server, Request, Response};
//!
//! let server = Server::bind("/tmp/lianwall.sock", true)?;
//!
//! loop {
//!     let mut conn = server.accept()?;
//!     conn.serve(|req| {
//!         let resp = match req {
//!             Request::Ping => Response::with_data(ResponseData::Pong),
//!             Request::Shutdown => return (Response::ok(), false),
//!             _ => Response::ok(),
//!         };
//!         (resp, true)
//!     })?;
//! }
//! ```

pub mod client;
pub mod codec;
pub mod error;
pub mod protocol;
pub mod server;

// Re-exports
pub use client::Client;
pub use error::SocketError;
pub use protocol::{
    Request, Response, ResponseData, SpaceSnapshot, StatusInfo, WallpaperPoint, PROTOCOL_VERSION,
};
pub use server::{Connection, Server};

/// 快捷函数
pub use client::quick;
