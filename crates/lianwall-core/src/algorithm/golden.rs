//! 黄金角常量与工具函数

use std::f64::consts::TAU;

/// 黄金角（弧度）
///
/// = 2π × (1 - 1/φ) ≈ 2.399963229728653
///
/// 其中 φ = (1 + √5) / 2 ≈ 1.618033988749895（黄金比例）
///
/// 这是向日葵种子排列所使用的角度，数学上证明是最均匀的遍历方式。
pub const GOLDEN_ANGLE: f64 = 2.399963229728653;

/// 计算动态冷却值
///
/// 根据壁纸总数计算合适的冷却窗口大小。
/// 目标是防止用户能预测下一张壁纸。
///
/// - 人的短期记忆约 5-7 个项目
/// - 冷却值 = min(N/2, 7)，至少 1
///
/// # Examples
/// ```
/// use lianwall_core::algorithm::calc_cooldown;
/// assert_eq!(calc_cooldown(3), 1);
/// assert_eq!(calc_cooldown(10), 5);
/// assert_eq!(calc_cooldown(20), 7);
/// assert_eq!(calc_cooldown(100), 7);
/// ```
pub fn calc_cooldown(n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    1.max((n / 2).min(7))
}

/// 计算两个角度之间的最短距离
///
/// 结果在 [0, π] 范围内
pub fn angular_distance(a: f64, b: f64) -> f64 {
    let diff = (a - b).abs() % TAU;
    if diff > std::f64::consts::PI {
        TAU - diff
    } else {
        diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_golden_angle_value() {
        // 验证黄金角的计算
        let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
        let expected = TAU * (1.0 - 1.0 / phi);
        assert!((GOLDEN_ANGLE - expected).abs() < 1e-10);
    }

    #[test]
    fn test_calc_cooldown() {
        assert_eq!(calc_cooldown(0), 0);
        assert_eq!(calc_cooldown(1), 1);
        assert_eq!(calc_cooldown(2), 1);
        assert_eq!(calc_cooldown(3), 1);
        assert_eq!(calc_cooldown(4), 2);
        assert_eq!(calc_cooldown(10), 5);
        assert_eq!(calc_cooldown(14), 7);
        assert_eq!(calc_cooldown(100), 7);
    }

    #[test]
    fn test_angular_distance() {
        use std::f64::consts::PI;

        // 相同角度
        assert!((angular_distance(0.0, 0.0)).abs() < 1e-10);

        // 对角
        assert!((angular_distance(0.0, PI) - PI).abs() < 1e-10);

        // 跨越 0/2π 边界
        assert!((angular_distance(0.1, TAU - 0.1) - 0.2).abs() < 1e-10);
    }
}
