use crate::config::WeightConfig;
use rand::Rng;

/// 权重计算器
pub struct WeightCalculator {
    config: WeightConfig,
}

impl WeightCalculator {
    pub fn new(config: WeightConfig) -> Self {
        Self { config }
    }

    /// 根据文件修改时间计算初始权重
    /// 新文件获得更高权重（80-120范围）
    pub fn calculate_initial_weight(&self, file_age_ratio: f64) -> f64 {
        // file_age_ratio: 0.0 = 最新文件, 1.0 = 最旧文件
        // 新文件: 120, 旧文件: 80
        let min_weight = self.config.base - 20.0; // 80
        let max_weight = self.config.base + 20.0; // 120
        
        // 线性插值：新文件得高分
        max_weight - (file_age_ratio * (max_weight - min_weight))
    }

    /// 计算选中后的惩罚
    /// 被选中的壁纸权重大幅下降
    pub fn apply_selection_penalty(&self, current_value: f64) -> f64 {
        current_value - self.config.select_penalty
    }

    /// 计算未选中时的奖励
    /// 根据连续未选中次数递增奖励
    pub fn calculate_skip_reward(&self, skip_streak: u32) -> f64 {
        // 奖励递增规则：
        // 1-2 次: +1.0
        // 3-4 次: +2.0
        // 5-6 次: +3.0
        // 7-8 次: +4.0
        // 9+ 次:  +5.0 (封顶)
        let base_reward = match skip_streak {
            0..=2 => 1.0,
            3..=4 => 2.0,
            5..=6 => 3.0,
            7..=8 => 4.0,
            _ => self.config.skip_reward_max,
        };

        // 添加微小随机扰动，防止权重完全相同
        let mut rng = rand::thread_rng();
        let jitter = rng.gen_range(-0.3..0.3);
        
        base_reward + jitter
    }

    /// 应用未选中奖励
    pub fn apply_skip_reward(&self, current_value: f64, skip_streak: u32) -> f64 {
        current_value + self.calculate_skip_reward(skip_streak)
    }

    /// 获取基础权重（用于新发现文件的平均值计算）
    pub fn base_weight(&self) -> f64 {
        self.config.base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> WeightConfig {
        WeightConfig {
            base: 100.0,
            select_penalty: 10.0,
            skip_reward_max: 5.0,
        }
    }

    #[test]
    fn test_initial_weight() {
        let calc = WeightCalculator::new(test_config());
        
        // 最新文件应该得到 120
        assert!((calc.calculate_initial_weight(0.0) - 120.0).abs() < 0.001);
        
        // 最旧文件应该得到 80
        assert!((calc.calculate_initial_weight(1.0) - 80.0).abs() < 0.001);
        
        // 中等文件应该得到 100
        assert!((calc.calculate_initial_weight(0.5) - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_selection_penalty() {
        let calc = WeightCalculator::new(test_config());
        
        assert!((calc.apply_selection_penalty(100.0) - 90.0).abs() < 0.001);
    }

    #[test]
    fn test_skip_reward_scaling() {
        let calc = WeightCalculator::new(test_config());
        
        // 检查奖励是否随 skip_streak 递增
        let r1 = calc.calculate_skip_reward(1);
        let r5 = calc.calculate_skip_reward(5);
        let r10 = calc.calculate_skip_reward(10);
        
        // 由于有随机扰动，我们检查范围
        assert!(r1 >= 0.7 && r1 <= 1.3);  // 1.0 ± 0.3
        assert!(r5 >= 2.7 && r5 <= 3.3);  // 3.0 ± 0.3
        assert!(r10 >= 4.7 && r10 <= 5.3); // 5.0 ± 0.3
    }
}
