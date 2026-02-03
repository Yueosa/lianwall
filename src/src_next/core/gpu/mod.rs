//! GPU 模块：显存检测与状态判断的统一入口。
//!
//! ## 公共接口（函数签名）
//! - detect(input: VramDetectInput) -> VramDetectOutput
//! - get_info(input: VramGetInfoInput) -> VramGetInfoOutput
//! - check_low(input: VramCheckLowInput) -> VramCheckLowOutput
//! - check_recovered(input: VramCheckRecoveredInput) -> VramCheckRecoveredOutput
//!
//! ## 输入/输出结构体
//! - VramDetectInput / VramDetectOutput
//! - VramGetInfoInput / VramGetInfoOutput
//! - VramCheckLowInput / VramCheckLowOutput
//! - VramCheckRecoveredInput / VramCheckRecoveredOutput
//! - VramInfo / GpuType
//!
//! ## 职责
//! - 检测 GPU 类型（NVIDIA/AMD/Intel/Unknown）
//! - 获取显存使用信息
//! - 判断显存紧张/恢复状态（仅判断，不执行切换）
//! - 提供诊断信息供上层展示
//!
//! ## 支持的 GPU
//! - **NVIDIA**：通过 `nvidia-smi` 或 NVML 获取
//! - **AMD**：通过 `rocm-smi` 获取
//! - **Intel**：暂不支持（预留接口）
//!
//! ## 设计原则
//! - **职责单一**：只负责数据采集与状态判断
//! - **保守策略**：显存信息不可用时不触发切换
//! - **可诊断**：输出结构包含失败原因

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
