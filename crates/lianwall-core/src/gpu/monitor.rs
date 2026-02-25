//! 显存监控逻辑
//!
//! ## 错误处理策略
//!
//! 当 `check()` 返回 `Err` 时（如 nvidia-smi/rocm-smi 不可用或输出解析失败），
//! **不会自动修改用户的配置文件**。调用方（daemon）应该：
//!
//! 1. 记录警告日志
//! 2. 设置运行时标记，跳过后续的 VRAM 检测
//! 3. 继续正常运行（壁纸切换不受影响）
//!
//! 这样设计的原因：
//! - 自动修改用户配置是危险行为
//! - 用户可能只是临时卸载了驱动或在 SSH 环境中
//! - 下次启动时会重新检测后端

use crate::config::{VramBackend, VramConfig};

use super::error::GpuError;
use super::r#struct::{GpuBackend, VramAction, VramState};
use super::{detect_backend_sync, query_vram_sync};

/// 初始化监控状态（自动检测后端）
pub fn init() -> VramState {
    let backend = detect_backend_sync();
    VramState::new(backend)
}

/// 初始化监控状态（根据配置选择后端）
///
/// - `VramBackend::Auto`：自动检测 nvidia-smi / rocm-smi
/// - `VramBackend::Custom`：使用 `config.custom_command`
pub fn init_with_config(config: &VramConfig) -> VramState {
    let backend = match config.backend {
        VramBackend::Auto => detect_backend_sync(),
        VramBackend::Custom => GpuBackend::Custom {
            command: config.custom_command.clone(),
        },
    };
    VramState::new(backend)
}

/// 检查显存状态并做出决策
///
/// ## 决策逻辑
///
/// 1. 如果未启用监控或无后端 → Keep
/// 2. 查询显存信息
/// 3. 如果剩余 < threshold 且未降级 → Downgrade
/// 4. 如果剩余 >= recovery 且已降级且不在冷却期 → Upgrade
/// 5. 其他情况 → Keep
pub fn check(state: &mut VramState, config: &VramConfig) -> Result<VramAction, GpuError> {
    // 未启用或无后端
    if !config.enabled || state.backend == GpuBackend::None {
        return Ok(VramAction::Keep);
    }

    // 查询显存
    let info = query_vram_sync(state.backend.clone())?;

    // 决策
    let action = if info.free_percent < config.threshold_percent {
        // 显存不足
        if state.degraded {
            VramAction::Keep // 已经降级了
        } else {
            VramAction::Downgrade
        }
    } else if info.free_percent >= config.recovery_percent {
        // 显存充足
        if state.degraded {
            // 检查冷却期
            if state.is_in_cooldown(config.cooldown_seconds) {
                VramAction::Keep // 冷却期内，不恢复
            } else {
                VramAction::Upgrade
            }
        } else {
            VramAction::Keep // 本来就没降级
        }
    } else {
        // 中间状态，保持
        VramAction::Keep
    };

    // 更新状态
    match action {
        VramAction::Downgrade => state.mark_degraded(),
        VramAction::Upgrade => state.mark_upgraded(),
        VramAction::Keep => {}
    }

    Ok(action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(enabled: bool, threshold: f32, recovery: f32, cooldown: u64) -> VramConfig {
        VramConfig {
            enabled,
            threshold_percent: threshold,
            recovery_percent: recovery,
            check_interval: 2,
            cooldown_seconds: cooldown,
            backend: VramBackend::Auto,
            custom_command: String::new(),
        }
    }

    #[test]
    fn test_check_disabled() {
        let mut state = VramState::new(GpuBackend::None);
        let config = make_config(false, 25.0, 40.0, 30);

        let action = check(&mut state, &config).unwrap();
        assert_eq!(action, VramAction::Keep);
    }

    #[test]
    fn test_check_no_backend() {
        let mut state = VramState::new(GpuBackend::None);
        let config = make_config(true, 25.0, 40.0, 30);

        let action = check(&mut state, &config).unwrap();
        assert_eq!(action, VramAction::Keep);
    }

    #[test]
    fn test_cooldown_logic() {
        let mut state = VramState::new(GpuBackend::None);

        // 模拟降级
        state.mark_degraded();
        assert!(state.degraded);
        assert!(state.degraded_at.is_some());

        // 检查冷却
        assert!(state.is_in_cooldown(30)); // 刚降级，应在冷却期内

        // 模拟恢复
        state.mark_upgraded();
        assert!(!state.degraded);
        assert!(state.degraded_at.is_none());
    }
}
