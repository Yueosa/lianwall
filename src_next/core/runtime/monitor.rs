use crate::core::gpu::{check_low, check_recovered, VramCheckLowInput, VramCheckRecoveredInput};
use crate::core::runtime::r#struct::{ModeAction, MonitorCheckInput, MonitorCheckOutput};
use crate::core::runtime::state::RunMode;

/// 检查 VRAM 状态并返回模式切换建议
pub fn check(input: MonitorCheckInput) -> MonitorCheckOutput {
    // Video 模式且未降级 → 检查是否需要降级
    if input.current_mode == RunMode::Video && !input.is_degraded {
        let check_result = check_low(VramCheckLowInput {
            threshold_percent: input.threshold_percent as f32,
        });

        if check_result.is_low {
            return MonitorCheckOutput {
                action: ModeAction::DowngradeToImage,
                vram_info: None, // VRAM info 不再在 check_low 输出中
                reason: format!(
                    "显存不足：剩余 {:.1}% < 阈值 {}%",
                    check_result.current_percent.unwrap_or(0.0),
                    input.threshold_percent
                ),
            };
        } else {
            return MonitorCheckOutput {
                action: ModeAction::Keep,
                vram_info: None,
                reason: "显存充足".to_string(),
            };
        }
    }

    // Image 模式且已降级 → 检查是否可以恢复
    if input.current_mode == RunMode::Image && input.is_degraded {
        let check_result = check_recovered(VramCheckRecoveredInput {
            recovery_percent: input.recovery_percent as f32,
        });

        if check_result.is_recovered {
            return MonitorCheckOutput {
                action: ModeAction::UpgradeToVideo,
                vram_info: None,
                reason: format!(
                    "显存已恢复：剩余 {:.1}% > 阈值 {}%",
                    check_result.current_percent.unwrap_or(0.0),
                    input.recovery_percent
                ),
            };
        } else {
            return MonitorCheckOutput {
                action: ModeAction::Keep,
                vram_info: None,
                reason: "显存未恢复".to_string(),
            };
        }
    }

    // 其他情况：保持当前模式
    MonitorCheckOutput {
        action: ModeAction::Keep,
        vram_info: None,
        reason: "无需切换".to_string(),
    }
}
