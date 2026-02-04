//! Socket 通信协议 - 请求与响应结构

use serde::{Deserialize, Serialize}
use std::path::PathBuf

/// 客户端请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum Request {
    /// 启动守护进程 
    Start,
    Stop,
    Next,
    SwitchMode,
    Reload,
    Status,
    List { filter: ListFilter },
    Lock { path: PathBuf },
    Unlock { path: PathBuf },
    Stats,
    TimeRanges,
    Diagnose,
    Config(ConfigRequest),
    Ping,
}

