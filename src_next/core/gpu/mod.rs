//! GPU 显存检测模块
//!
//! ## 职责
//! - 检测 GPU 类型（NVIDIA/AMD/Intel/Unknown）
//! - 获取显存使用信息
//! - 判断显存紧张/恢复状态（仅判断，不执行切换）
//! - 提供详细的诊断信息
//!
//! ## 支持的 GPU
//! - **NVIDIA**：通过 `nvidia-smi` 获取（完全支持）
//! - **AMD**：通过 `rocm-smi` 获取（基本支持，格式解析可能因版本而异）
//! - **Intel**：暂不支持（预留接口）
//!
//! ## 使用示例
//! ```rust
//! use crate::core::gpu::{detect, get_info, check_low};
//! use crate::core::gpu::{VramDetectInput, VramGetInfoInput, VramCheckLowInput};
//!
//! // 检测 GPU 类型
//! match detect(VramDetectInput {}) {
//!     Ok(result) => println!("检测到 GPU: {:?}", result.gpu_type),
//!     Err(e) => eprintln!("显存检测不可用: {}", e),
//! }
//!
//! // 获取显存信息
//! match get_info(VramGetInfoInput {}) {
//!     Ok(result) => println!("显存使用: {}/{} MB", result.info.used_mb, result.info.total_mb),
//!     Err(e) => eprintln!("获取显存失败: {}", e),
//! }
//!
//! // 检查是否紧张
//! match check_low(VramCheckLowInput { threshold_percent: 25.0 }) {
//!     Ok(result) if result.is_low => println!("显存紧张！剩余: {:.1}%", result.current_percent),
//!     Ok(_) => println!("显存充足"),
//!     Err(e) => eprintln!("检查失败: {}", e),
//! }
//! ```
//!
//! ## 设计原则
//! - **错误透传**：使用 Result + thiserror，提供函数级错误定位
//! - **职责单一**：只负责数据采集和状态判断，不执行切换操作
//! - **可诊断**：提供详细的失败原因，方便 API 层输出诊断信息

mod error;
mod r#struct;
mod vram;

// 导出核心函数
pub use vram::{check_low, check_recovered, detect, get_info};

// 导出错误类型
pub use error::VramError;

// 导出所有结构体
pub use r#struct::{
    GpuType, VramCheckLowInput, VramCheckLowOutput, VramCheckRecoveredInput,
    VramCheckRecoveredOutput, VramDetectInput, VramDetectOutput, VramGetInfoInput,
    VramGetInfoOutput, VramInfo,
};
