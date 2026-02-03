use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 引擎类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineType {
    MpvPaper,
    Swww,
}

// --- IO 结构体 ---

/// 检测引擎可用性
#[derive(Debug, Clone)]
pub struct EngineDetectInput {
    pub engine_type: EngineType,
}

#[derive(Debug, Clone)]
pub struct EngineDetectOutput {
    pub available: bool,
}

/// 设置壁纸
#[derive(Debug, Clone)]
pub struct EngineSetInput {
    pub engine_type: EngineType,
    pub wallpaper_path: PathBuf,
    /// 从配置文件读取的参数列表
    pub extra_args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EngineSetOutput {
    /// 进程 PID（mpvpaper 返回，swww 为 None）
    pub pid: Option<u32>,
}

/// 停止引擎
#[derive(Debug, Clone)]
pub struct EngineStopInput {
    pub engine_type: EngineType,
}

#[derive(Debug, Clone)]
pub struct EngineStopOutput {
    // 成功时返回 Ok(EngineStopOutput)
    // 失败时抛出 EngineError::StopFailed
}
