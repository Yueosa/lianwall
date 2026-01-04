use crate::manager::Wallpaper;
use rand::Rng;

/// 壁纸选择器
/// 实现二分切割和权重过滤算法，加入动态扰动
#[allow(dead_code)]
pub struct WallpaperSelector;

impl WallpaperSelector {
    /// 选择壁纸（带动态扰动）
    ///
    /// 算法流程：
    /// 1. 对所有壁纸应用动态扰动
    /// 2. 按扰动后的权重排序
    /// 3. 找到前 tolerance 范围内的所有壁纸
    /// 4. 选择中间位置的壁纸（二分切割）
    pub fn select(
        wallpapers: &mut [Wallpaper],
        tolerance: f64,
        perturbation_ratio: f64,
    ) -> Option<usize> {
        if wallpapers.is_empty() {
            return None;
        }

        // 应用动态扰动
        let mut rng = rand::thread_rng();
        let perturbed_values: Vec<(usize, f64)> = wallpapers
            .iter()
            .enumerate()
            .map(|(idx, w)| {
                let random_factor = rng.gen_range(-1.0..1.0);
                let perturbation = w.value * perturbation_ratio * random_factor;
                let perturbed = (w.value + perturbation).max(1.0);
                (idx, perturbed)
            })
            .collect();

        // 按扰动后的权重排序
        let mut sorted_indices = perturbed_values.clone();
        sorted_indices.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let max_value = sorted_indices[0].1;

        // 找到前 tolerance 范围内的所有壁纸
        let top_indices: Vec<usize> = sorted_indices
            .iter()
            .filter(|(_, v)| (max_value - v).abs() <= tolerance)
            .map(|(idx, _)| *idx)
            .collect();

        if top_indices.is_empty() {
            return Some(sorted_indices[0].0);
        }

        // 二分切割：选择中间位置
        let mid_index = top_indices.len() / 2;
        Some(top_indices[mid_index])
    }

    pub fn get_stats(wallpapers: &[Wallpaper]) -> Stats {
        if wallpapers.is_empty() {
            return Stats::default();
        }

        let values: Vec<f64> = wallpapers.iter().map(|w| w.value).collect();
        let sum: f64 = values.iter().sum();
        let count = values.len() as f64;

        Stats {
            count: wallpapers.len(),
            min_value: values.iter().cloned().fold(f64::INFINITY, f64::min),
            max_value: values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            avg_value: sum / count,
            total_skips: wallpapers.iter().map(|w| w.skip_streak as u64).sum(),
        }
    }
}

/// 壁纸库统计信息
#[derive(Debug, Default)]
pub struct Stats {
    pub count: usize,
    pub min_value: f64,
    pub max_value: f64,
    pub avg_value: f64,
    pub total_skips: u64,
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "壁纸数量: {}\n权重范围: {:.2} ~ {:.2}\n平均权重: {:.2}\n总跳过次数: {}",
            self.count, self.min_value, self.max_value, self.avg_value, self.total_skips
        )
    }
}
