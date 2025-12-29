use crate::manager::Wallpaper;

/// 壁纸选择器
/// 实现二分切割和权重过滤算法
#[allow(dead_code)]
pub struct WallpaperSelector;

impl WallpaperSelector {
    pub fn select(wallpapers: &mut [Wallpaper], tolerance: f64) -> Option<usize> {
        if wallpapers.is_empty() {
            return None;
        }

        wallpapers.sort_by(|a, b| {
            b.value.partial_cmp(&a.value).unwrap_or(std::cmp::Ordering::Equal)
        });

        let max_value = wallpapers[0].value;

        let top_indices: Vec<usize> = wallpapers
            .iter()
            .enumerate()
            .filter(|(_, w)| (max_value - w.value).abs() <= tolerance)
            .map(|(i, _)| i)
            .collect();

        if top_indices.is_empty() {
            return Some(0);
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_wallpapers() -> Vec<Wallpaper> {
        vec![
            Wallpaper {
                path: PathBuf::from("a.mp4"),
                value: 100.0,
                skip_streak: 0,
                last_played: None,
            },
            Wallpaper {
                path: PathBuf::from("b.mp4"),
                value: 105.0,
                skip_streak: 2,
                last_played: None,
            },
            Wallpaper {
                path: PathBuf::from("c.mp4"),
                value: 103.0,
                skip_streak: 1,
                last_played: None,
            },
            Wallpaper {
                path: PathBuf::from("d.mp4"),
                value: 80.0,
                skip_streak: 0,
                last_played: None,
            },
        ]
    }

    #[test]
    fn test_select_binary() {
        let mut wallpapers = create_test_wallpapers();
        
        let idx = WallpaperSelector::select(&mut wallpapers, 5.0);
        assert!(idx.is_some());
        
        assert_eq!(idx.unwrap(), 1);
    }

    #[test]
    fn test_stats() {
        let wallpapers = create_test_wallpapers();
        let stats = WallpaperSelector::get_stats(&wallpapers);
        
        assert_eq!(stats.count, 4);
        assert!((stats.min_value - 80.0).abs() < 0.001);
        assert!((stats.max_value - 105.0).abs() < 0.001);
        assert!((stats.avg_value - 97.0).abs() < 0.001);
        assert_eq!(stats.total_skips, 3);
    }
}
