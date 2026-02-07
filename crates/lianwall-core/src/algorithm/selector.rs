//! 壁纸选择器

use std::f64::consts::TAU;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::wallpaper::WallpaperSpace;

use super::r#struct::SelectOutput;
use super::golden::{angular_distance, calc_cooldown, GOLDEN_ANGLE};

/// 选择下一张壁纸
///
/// 算法流程：
/// 1. 计算动态冷却值
/// 2. 找到距离指针最近的可用壁纸（排除锁定和冷却中的）
/// 3. 更新冷却队列
/// 4. 更新 last_played
/// 5. 旋转指针（黄金角）
///
/// 注意：历史记录管理已移至 daemon 层的 PlaybackHistory
///
/// # Returns
/// - `Some(SelectOutput)` - 选中结果
/// - `None` - 没有可用壁纸
pub fn select_next(space: &mut WallpaperSpace) -> Option<SelectOutput> {
    if space.is_empty() {
        return None;
    }

    let cooldown = calc_cooldown(space.len());

    // 找到距离指针最近的可用壁纸
    let selected_idx = find_nearest_available(space, cooldown)?;

    // 更新当前壁纸索引
    space.current_index = Some(selected_idx);

    // 更新冷却队列
    space.cooldown_queue.push_back(selected_idx);
    while space.cooldown_queue.len() > cooldown {
        space.cooldown_queue.pop_front();
    }

    // 更新 last_played
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    space.items[selected_idx].last_played = Some(now);

    // 旋转指针
    let new_pointer = (space.pointer + GOLDEN_ANGLE).rem_euclid(TAU);
    space.pointer = new_pointer;

    Some(SelectOutput {
        index: selected_idx,
        new_pointer,
    })
}

/// 找到距离指针最近的可用壁纸
///
/// 先尝试排除冷却中的壁纸；如果所有未锁定壁纸都在冷却中，
/// 则回退到从冷却队列中选最早进入冷却的（即冷却最久的）
fn find_nearest_available(space: &WallpaperSpace, cooldown: usize) -> Option<usize> {
    let mut best_idx: Option<usize> = None;
    let mut best_dist = f64::MAX;

    for (i, item) in space.items.iter().enumerate() {
        // 跳过锁定的
        if item.locked {
            continue;
        }

        // 跳过冷却中的
        if space.cooldown_queue.iter().take(cooldown).any(|&idx| idx == i) {
            continue;
        }

        // 计算角度距离
        let dist = angular_distance(space.pointer, item.angle);
        if dist < best_dist {
            best_dist = dist;
            best_idx = Some(i);
        }
    }

    // 回退：所有未锁定壁纸都在冷却中，从冷却队列中选最早的（冷却最久的）未锁定壁纸
    if best_idx.is_none() {
        for &idx in &space.cooldown_queue {
            if idx < space.items.len() && !space.items[idx].locked {
                return Some(idx);
            }
        }
    }

    best_idx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallpaper::build_space;
    use std::path::PathBuf;

    use crate::wallpaper::ScannedWallpaper;

    fn make_test_wallpapers(n: usize) -> Vec<ScannedWallpaper> {
        (0..n).map(|i| ScannedWallpaper {
            path: PathBuf::from(format!("/test/{}.jpg", i)),
            time_constraints: vec![],
        }).collect()
    }

    #[test]
    fn test_select_empty() {
        let mut space = build_space(vec![], 42);
        assert!(select_next(&mut space).is_none());
    }

    #[test]
    fn test_select_single() {
        let mut space = build_space(make_test_wallpapers(1), 42);
        
        let result = select_next(&mut space);
        assert!(result.is_some());
        assert_eq!(result.unwrap().index, 0);
    }

    #[test]
    fn test_select_updates_pointer() {
        let mut space = build_space(make_test_wallpapers(5), 42);
        let old_pointer = space.pointer;
        
        select_next(&mut space);
        
        let expected = (old_pointer + GOLDEN_ANGLE) % TAU;
        assert!((space.pointer - expected).abs() < 1e-10);
    }

    #[test]
    fn test_cooldown_prevents_repeat() {
        let mut space = build_space(make_test_wallpapers(10), 42);
        
        let mut selected: Vec<usize> = Vec::new();
        for _ in 0..20 {
            if let Some(result) = select_next(&mut space) {
                selected.push(result.index);
            }
        }

        // 检查冷却窗口内无重复
        let cooldown = calc_cooldown(10);
        for window in selected.windows(cooldown) {
            let unique: std::collections::HashSet<_> = window.iter().collect();
            assert_eq!(unique.len(), window.len(), "Found repeat in cooldown window");
        }
    }

    #[test]
    fn test_locked_skipped() {
        let mut space = build_space(make_test_wallpapers(3), 42);
        
        // 锁定所有但一个
        space.items[0].locked = true;
        space.items[1].locked = true;
        
        let result = select_next(&mut space);
        assert!(result.is_some());
        assert_eq!(result.unwrap().index, 2);
    }

    #[test]
    fn test_all_locked_returns_none() {
        let mut space = build_space(make_test_wallpapers(3), 42);
        
        for item in &mut space.items {
            item.locked = true;
        }
        
        assert!(select_next(&mut space).is_none());
    }
}
