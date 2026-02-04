//! GPU 模块数据结构

use std::time::Instant;

/// GPU 后端类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuBackend {
    /// nvidia-smi 命令（NVIDIA 显卡）
    NvidiaSmi,
    /// rocm-smi 命令（AMD 显卡）
    RocmSmi,
    /// 无可用后端
    None,
}

/// 显存信息
#[derive(Debug, Clone)]
pub struct VramInfo {
    /// 总显存（MB）
    pub total_mb: u64,
    /// 已用显存（MB）
    pub used_mb: u64,
    /// 剩余显存（MB）
    pub free_mb: u64,
    /// 剩余百分比（0.0 - 100.0）
    pub free_percent: f32,
}

impl VramInfo {
    /// 从总量和已用量创建
    pub fn new(total_mb: u64, used_mb: u64) -> Self {
        let free_mb = total_mb.saturating_sub(used_mb);
        let free_percent = if total_mb > 0 {
            (free_mb as f32 / total_mb as f32) * 100.0
        } else {
            0.0
        };

        Self {
            total_mb,
            used_mb,
            free_mb,
            free_percent,
        }
    }
}

/// 监控状态（由 daemon 持有）
pub struct VramState {
    /// 当前使用的后端
    pub backend: GpuBackend,
    /// 是否处于降级状态
    pub degraded: bool,
    /// 降级时间点（用于冷却计算）
    pub degraded_at: Option<Instant>,
}

impl VramState {
    /// 创建新状态
    pub fn new(backend: GpuBackend) -> Self {
        Self {
            backend,
            degraded: false,
            degraded_at: None,
        }
    }

    /// 检查是否在冷却期内
    pub fn is_in_cooldown(&self, cooldown_seconds: u64) -> bool {
        if let Some(degraded_at) = self.degraded_at {
            degraded_at.elapsed().as_secs() < cooldown_seconds
        } else {
            false
        }
    }

    /// 标记为降级
    pub fn mark_degraded(&mut self) {
        self.degraded = true;
        self.degraded_at = Some(Instant::now());
    }

    /// 标记为恢复
    pub fn mark_upgraded(&mut self) {
        self.degraded = false;
        self.degraded_at = None;
    }
}

/// 监控决策
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VramAction {
    /// 保持当前状态
    Keep,
    /// 降级到静态壁纸
    Downgrade,
    /// 恢复到动态壁纸
    Upgrade,
}
