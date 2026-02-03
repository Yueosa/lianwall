use crate::core::gpu::{check_low, check_recovered, VramCheckLowInput, VramCheckRecoveredInput};
use crate::core::runtime::r#struct::{ModeAction, MonitorCheckInput, MonitorCheckOutput};
use crate::core::runtime::state::RunMode;

/// 检查 VRAM 状态并返回模式切换建议
pub fn check(input: MonitorCheckInput) -> MonitorCheckOutput {
    // Video 模式 → 检查是否需要降级
    if input.current_mode == RunMode::Video {
        let check_result = check_low(VramCheckLowInput {
            threshold_percent: input.threshold_percent,
        });

        if check_result.is_low {
            return MonitorCheckOutput {
                action: ModeAction::DowngradeToImage,
                reason: format!(
                    "显存不足：剩余 {:.1}% < 阈值 {}%",
                    check_result.current_percent.unwrap_or(0.0),
                    input.threshold_percent
                ),
            };
        } else if let Some(current) = check_result.current_percent {
            return MonitorCheckOutput {
                action: ModeAction::Keep,
                reason: format!("显存充足：剩余 {:.1}%", current),
            };
        } else {
            return MonitorCheckOutput {
                action: ModeAction::Keep,
                reason: "显存信息不可用".to_string(),
            };
        }
    }

    // Image 模式且是因 VRAM 降级导致的 → 检查是否可以恢复
    if input.current_mode == RunMode::Image && input.was_degraded {
        let check_result = check_recovered(VramCheckRecoveredInput {
            recovery_percent: input.recovery_percent,
        });

        if check_result.is_recovered {
            return MonitorCheckOutput {
                action: ModeAction::UpgradeToVideo,
                reason: format!(
                    "显存已恢复：剩余 {:.1}% > 阈值 {}%",
                    check_result.current_percent.unwrap_or(0.0),
                    input.recovery_percent
                ),
            };
        } else if let Some(current) = check_result.current_percent {
            return MonitorCheckOutput {
                action: ModeAction::Keep,
                reason: format!("显存未恢复：剩余 {:.1}%", current),
            };
        } else {
            return MonitorCheckOutput {
                action: ModeAction::Keep,
                reason: "显存信息不可用".to_string(),
            };
        }
    }

    // 其他情况：保持当前模式
    MonitorCheckOutput {
        action: ModeAction::Keep,
        reason: "无需切换".to_string(),
    }
}
