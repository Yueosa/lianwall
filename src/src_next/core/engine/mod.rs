//! Engine 模块：壁纸引擎的统一适配入口。
//!
//! ## 公共接口（函数签名）
//! - detect(input: EngineDetectInput) -> Result<EngineDetectOutput, EngineError>
//! - set(input: EngineSetInput) -> Result<EngineSetOutput, EngineError>
//! - stop(input: EngineStopInput) -> Result<EngineStopOutput, EngineError>
//! - is_running(engine_type: EngineType) -> bool
//!
//! ## 输入/输出结构体
//! - EngineDetectInput / EngineDetectOutput
//! - EngineSetInput / EngineSetOutput
//! - EngineStopInput / EngineStopOutput
//! - EngineType
//!
//! ## 职责
//! - 统一封装 mpvpaper 与 swww 的调用接口
//! - 检测引擎可用性
//! - 显示器输出检测与回退
//! - 进程管理（需要时先停止再启动）
//!
//! ## 支持的引擎
//! - **mpvpaper**：动态壁纸（视频），依赖 mpvpaper
//! - **swww**：静态壁纸（图片），依赖 swww
//!
//! ## 设计原则
//! - **职责单一**：仅负责引擎调用与状态处理
//! - **互斥启动**：切换模式时由上层决定停止另一引擎
//! - **显示器回退**：hyprctl 不可用时回退到 "*" 通配符
//! - **参数透传**：用户参数完整传递给引擎命令
//!
//! ## 系统依赖
//! - mpvpaper（动态壁纸）
//! - swww（静态壁纸）
//! - hyprctl（可选，用于多显示器检测）

mod error;
mod mpvpaper;
mod r#struct;
mod swww;
mod utils;

// 导出错误类型
pub use error::EngineError;

// 导出结构体
pub use r#struct::{
    EngineDetectInput, EngineDetectOutput, EngineSetInput, EngineSetOutput, EngineStopInput,
    EngineStopOutput, EngineType,
};

/// 检测引擎可用性
pub fn detect(input: EngineDetectInput) -> Result<EngineDetectOutput, EngineError> {
    match input.engine_type {
        EngineType::MpvPaper => mpvpaper::detect(input),
        EngineType::Swww => swww::detect(input),
    }
}

/// 设置壁纸
pub fn set(input: EngineSetInput) -> Result<EngineSetOutput, EngineError> {
    match input.engine_type {
        EngineType::MpvPaper => mpvpaper::set(input),
        EngineType::Swww => swww::set(input),
    }
}

/// 停止引擎
pub fn stop(input: EngineStopInput) -> Result<EngineStopOutput, EngineError> {
    match input.engine_type {
        EngineType::MpvPaper => mpvpaper::stop(input),
        EngineType::Swww => swww::stop(input),
    }
}

/// 检查引擎是否正在运行
pub fn is_running(engine_type: EngineType) -> bool {
    match engine_type {
        EngineType::MpvPaper => utils::is_process_running("mpvpaper"),
        EngineType::Swww => utils::is_process_running("swww-daemon"),
    }
}
