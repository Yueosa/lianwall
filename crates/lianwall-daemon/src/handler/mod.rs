//! Handler - 请求处理器
//!
//! 分为三类：
//! - Query: 只读操作，直接读取 SharedState
//! - Command: 修改操作，通过 CommandQueue 串行执行
//! - Subscribe: 订阅管理，由 Connection 处理

mod query;
mod command;

pub use query::handle_query;
pub use command::handle_command;
