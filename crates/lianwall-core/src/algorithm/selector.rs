//! 壁纸选择器

use std::f64::consts::TAU;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::wallpaper::WallpaperSpace;

use super::r#struct::SelectOutput;
use super::golden::{angular_distance, calc_cooldown, GOLDEN_ANGLE};

/// 历史记录最大长度
const MAX_HISTORY_SIZE: usize = 100;

/// 选择下一张壁纸
///
/// 算法流程：
/// 1. 将当前壁纸（如果有）压入历史栈
/// 2. 计算动态冷却值
/// 3. 找到距离指针最近的可用壁纸（排除锁定和冷却中的）
/// 4. 更新冷却队列
/// 5. 更新 last_played
/// 6. 旋转指针（黄金角）
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

    // 将当前壁纸压入历史栈（用于 prev）
    if let Some(current) = space.current_index {
        space.history.push(current);
        // 限制历史栈大小
        if space.history.len() > MAX_HISTORY_SIZE {
            space.history.remove(0);
        }
    }

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

/// 返回上一张壁纸
///
/// 从历史栈中弹出上一张壁纸，忽略锁定状态（强制播放）
///
/// # Returns
/// - `Some(SelectOutput)` - 上一张壁纸
/// - `None` - 没有历史记录
pub fn select_previous(space: &mut WallpaperSpace) -> Option<SelectOutput> {
    // 从历史栈弹出
    let prev_idx = space.history.pop()?;

    // 检查索引是否有效
    if prev_idx >= space.items.len() {
        return None;
    }

    // 更新当前壁纸索引
    space.current_index = Some(prev_idx);

    // 更新 last_played
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    space.items[prev_idx].last_played = Some(now);

    // 指针反向旋转（回退）
    let new_pointer = (space.pointer - GOLDEN_ANGLE).rem_euclid(TAU);
    space.pointer = new_pointer;

    Some(SelectOutput {
        index: prev_idx,
        new_pointer,
    })
}

/// 找到距离指针最近的可用壁纸
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
