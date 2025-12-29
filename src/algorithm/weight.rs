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

    pub fn calculate_initial_weight(&self, file_age_ratio: f64) -> f64 {
        let min_weight = self.config.base - 20.0;
        let max_weight = self.config.base + 20.0;
        
        max_weight - (file_age_ratio * (max_weight - min_weight))
    }

    pub fn apply_selection_penalty(&self, current_value: f64) -> f64 {
        current_value - self.config.select_penalty
    }

    pub fn calculate_skip_reward(&self, skip_streak: u32) -> f64 {
        let base_reward = match skip_streak {
            0..=2 => 1.0,
            3..=4 => 2.0,
            5..=6 => 3.0,
            7..=8 => 4.0,
            _ => self.config.skip_reward_max,
        };

        let mut rng = rand::thread_rng();
        let jitter = rng.gen_range(-0.3..0.3);
        
        base_reward + jitter
    }

    pub fn apply_skip_reward(&self, current_value: f64, skip_streak: u32) -> f64 {
        current_value + self.calculate_skip_reward(skip_streak)
    }

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
        
        assert!((calc.calculate_initial_weight(0.0) - 120.0).abs() < 0.001);
        
        assert!((calc.calculate_initial_weight(1.0) - 80.0).abs() < 0.001);
        
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
        
        let r1 = calc.calculate_skip_reward(1);
        let r5 = calc.calculate_skip_reward(5);
        let r10 = calc.calculate_skip_reward(10);
        
        assert!(r1 >= 0.7 && r1 <= 1.3);
        assert!(r5 >= 2.7 && r5 <= 3.3);
        assert!(r10 >= 4.7 && r10 <= 5.3);
    }
}
