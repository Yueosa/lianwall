use serde::{Deserialize, Serialize};

/// GPU 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuType {
    Nvidia,
    Amd,
    Intel,
    Unknown,
}

/// 显存使用信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VramInfo {
    /// 已使用显存（MB）
    pub used_mb: u64,
    /// 总显存（MB）
    pub total_mb: u64,
    /// 使用率（0.0 - 100.0）
    pub usage_percent: f32,
    /// 剩余率（0.0 - 100.0）
    pub free_percent: f32,
}

// --- IO 结构体 ---

/// 检测 GPU 类型和可用性
#[derive(Debug, Clone)]
pub struct VramDetectInput {}

#[derive(Debug, Clone)]
pub struct VramDetectOutput {
    pub gpu_type: GpuType,
}

/// 获取显存信息
#[derive(Debug, Clone)]
pub struct VramGetInfoInput {}

#[derive(Debug, Clone)]
pub struct VramGetInfoOutput {
    pub info: VramInfo,
}

/// 检查显存是否紧张（低于阈值）
#[derive(Debug, Clone)]
pub struct VramCheckLowInput {
    pub threshold_percent: f32,
}

#[derive(Debug, Clone)]
pub struct VramCheckLowOutput {
    /// 是否低于阈值
    pub is_low: bool,
    /// 当前剩余百分比
    pub current_percent: f32,
    /// 阈值（回显输入）
    pub threshold_percent: f32,
}

/// 检查显存是否已恢复（高于阈值）
#[derive(Debug, Clone)]
pub struct VramCheckRecoveredInput {
    pub recovery_percent: f32,
}

#[derive(Debug, Clone)]
pub struct VramCheckRecoveredOutput {
    /// 是否高于恢复阈值
    pub is_recovered: bool,
    /// 当前剩余百分比
    pub current_percent: f32,
    /// 恢复阈值（回显输入）
    pub recovery_percent: f32,
}
