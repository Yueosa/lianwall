use crate::config::WeightConfig;
use crate::manager::Wallpaper;
use rand::Rng;

/// 权重计算器
///
/// 新算法特性：
/// 1. 零和博弈：总权重守恒，选中者的惩罚均分给其他壁纸
/// 2. 动态扰动：扰动幅度与权重成比例（如±3%）
/// 3. 自动归一化：平均权重超过阈值时自动缩放
/// 4. 周期性洗牌：每N轮随机重置部分壁纸权重
pub struct WeightCalculator {
    config: WeightConfig,
    selection_count: u32, // 用于跟踪洗牌周期
}

impl WeightCalculator {
    pub fn new(config: WeightConfig) -> Self {
        Self {
            config,
            selection_count: 0,
        }
    }

    pub fn calculate_initial_weight(&self, file_age_ratio: f64) -> f64 {
        let min_weight = self.config.base - 20.0;
        let max_weight = self.config.base + 20.0;

        max_weight - (file_age_ratio * (max_weight - min_weight))
    }

    /// 更新所有壁纸权重（零和博弈实现）
    ///
    /// 核心逻辑：
    /// - 选中壁纸减少 penalty
    /// - 其他壁纸平均分配这个 penalty
    /// - 总权重保持不变 ⇒ 零和博弈
    pub fn update_weights_zero_sum(&mut self, wallpapers: &mut [Wallpaper], selected_index: usize) {
        if wallpapers.is_empty() {
            return;
        }

        let penalty = self.config.select_penalty;
        let other_count = wallpapers.len() - 1;

        if other_count == 0 {
            // 只有一张壁纸，不需要调整
            return;
        }

        let reward_per_wallpaper = penalty / other_count as f64;

        for (idx, wall) in wallpapers.iter_mut().enumerate() {
            if idx == selected_index {
                // 选中壁纸：扣除惩罚
                wall.value -= penalty;
                wall.skip_streak = 0;
            } else {
                // 未选中壁纸：获得奖励
                wall.value += reward_per_wallpaper;
                wall.skip_streak += 1;
            }
        }

        self.selection_count += 1;

        // 检查是否需要洗牌
        if self.config.shuffle_period > 0 && self.selection_count % self.config.shuffle_period == 0
        {
            self.apply_shuffle(wallpapers);
        }

        // 检查是否需要归一化
        self.auto_normalize(wallpapers);
    }

    /// 自动归一化：当平均权重超过阈值时，将所有权重按比例缩放
    ///
    /// 目标：将平均权重调整为 normalization_target
    ///
    /// 示例：
    /// - 当前平均：520
    /// - 阈值：500
    /// - 目标：100
    /// - 缩放因子：100 / 520 ≈ 0.192
    /// - 所有权重乘以 0.192
    fn auto_normalize(&self, wallpapers: &mut [Wallpaper]) {
        if wallpapers.is_empty() {
            return;
        }

        let total: f64 = wallpapers.iter().map(|w| w.value).sum();
        let avg = total / wallpapers.len() as f64;

        if avg > self.config.normalization_threshold {
            let scale_factor = self.config.normalization_target / avg;

            println!(
                "🔄 自动归一化触发：平均权重 {:.2} → {:.2}（缩放系数 {:.4}）",
                avg,
                avg * scale_factor,
                scale_factor
            );

            for wall in wallpapers.iter_mut() {
                wall.value *= scale_factor;
            }
        }
    }

    /// 周期性洗牌：随机重置部分壁纸权重，打破生态锁定
    ///
    /// 策略：
    /// - 选择 shuffle_intensity 比例的壁纸
    /// - 将它们的权重重置为基础值附近的随机值
    /// - 打破固定的权重梯度，引入新的随机性
    fn apply_shuffle(&self, wallpapers: &mut [Wallpaper]) {
        if wallpapers.is_empty() || self.config.shuffle_intensity <= 0.0 {
            return;
        }

        let shuffle_count = ((wallpapers.len() as f64 * self.config.shuffle_intensity).ceil()
            as usize)
            .min(wallpapers.len());

        if shuffle_count == 0 {
            return;
        }

        let mut rng = rand::thread_rng();
        let mut indices: Vec<usize> = (0..wallpapers.len()).collect();

        // Fisher-Yates 洗牌
        for i in (1..indices.len()).rev() {
            let j = rng.gen_range(0..=i);
            indices.swap(i, j);
        }

        println!(
            "🎲 周期性洗牌：重置 {} 张壁纸权重（强度 {:.0}%）",
            shuffle_count,
            self.config.shuffle_intensity * 100.0
        );

        for i in 0..shuffle_count {
            let idx = indices[i];
            // 重置为基础权重附近的随机值（±20%）
            let random_offset = rng.gen_range(-0.2..0.2);
            wallpapers[idx].value = self.config.base * (1.0 + random_offset);
            wallpapers[idx].skip_streak = 0;
        }
    }

    pub fn base_weight(&self) -> f64 {
        self.config.base
    }
}
