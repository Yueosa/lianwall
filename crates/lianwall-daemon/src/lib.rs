//! LianWall Daemon Library
//!
//! 导出 daemon 核心组件，供测试和集成使用

pub mod backend;
pub mod command;
pub mod connection;
pub mod event;
pub mod handler;
pub mod hook;
pub mod ipc;
pub mod scheduler;
pub mod server;
pub mod state;

pub use command::CommandQueue;
pub use event::EventBus;
pub use state::SharedState;

#[cfg(feature = "native-image")]
pub mod native;

#[cfg(feature = "native-image")]
mod outputs;

pub mod library;

pub mod logging;
