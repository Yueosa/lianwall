//! 壁纸选择器

use rand::{rngs::StdRng, SeedableRng};
use std::f64::consts::TAU;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::wallpaper::WallpaperSpace;

use super::r#struct::SelectOutput;
use super::r#struct::SelectionConfig;
use super::golden::{calc_cooldown, GOLDEN_ANGLE};

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
    select_next_with_config(space, SelectionConfig::default())
}

/// 使用指定策略参数选择下一张壁纸
pub fn select_next_with_config(
    space: &mut WallpaperSpace,
    selection: SelectionConfig,
) -> Option<SelectOutput> {
    if space.is_empty() {
        return None;
    }

    let cooldown = calc_cooldown(space.len());

    // 先在非冷却候选中抽样；若为空，再从冷却队列中回退选择
    let selected_idx = if let Some(idx) = choose_from_allowed(space, cooldown, selection) {
        idx
    } else if let Some(idx) = choose_from_cooldown_fallback(space, selection, true) {
        idx
    } else {
        choose_from_cooldown_fallback(space, selection, false)?
    };

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
    space.selector_nonce = space.selector_nonce.saturating_add(1);

    Some(SelectOutput {
        index: selected_idx,
        new_pointer,
    })
}

fn choose_from_allowed(
    space: &WallpaperSpace,
    cooldown: usize,
    selection: SelectionConfig,
) -> Option<usize> {
    let candidates: Vec<usize> = space
        .items
        .iter()
        .enumerate()
        .filter(|(i, item)| {
            !item.locked && !space.cooldown_queue.iter().take(cooldown).any(|&idx| idx == *i)
        })
        .map(|(i, _)| i)
        .collect();

    sample_biased_candidate(space, &candidates, selection)
}

fn choose_from_cooldown_fallback(
    space: &WallpaperSpace,
    selection: SelectionConfig,
    exclude_current: bool,
) -> Option<usize> {
    let candidates: Vec<usize> = space
        .cooldown_queue
        .iter()
        .copied()
        .filter(|&idx| idx < space.items.len())
        .filter(|&idx| !space.items[idx].locked)
        .filter(|&idx| !exclude_current || Some(idx) != space.current_index)
        .collect();

    sample_biased_candidate(space, &candidates, selection)
}

fn sample_biased_candidate(
    space: &WallpaperSpace,
    candidates: &[usize],
    selection: SelectionConfig,
) -> Option<usize> {
    if candidates.is_empty() {
        return None;
    }

    let temperature = selection.temperature.max(1e-6);
    let gap = (TAU / candidates.len() as f64).max(1e-9);

    let scores: Vec<f64> = candidates
        .iter()
        .map(|&idx| biased_score(space.pointer, space.items[idx].angle, gap, selection.bias_lambda))
        .collect();

    let max_logit = scores
        .iter()
        .map(|score| -score / temperature)
        .fold(f64::NEG_INFINITY, f64::max);

    let weights: Vec<f64> = scores
        .iter()
        .map(|score| (-score / temperature - max_logit).exp())
        .collect();

    let total_weight: f64 = weights.iter().sum();
    if !total_weight.is_finite() || total_weight <= 0.0 {
        return candidates
            .iter()
            .copied()
            .min_by(|&lhs, &rhs| {
                let lhs_score = biased_score(space.pointer, space.items[lhs].angle, gap, selection.bias_lambda);
                let rhs_score = biased_score(space.pointer, space.items[rhs].angle, gap, selection.bias_lambda);
                lhs_score
                    .partial_cmp(&rhs_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(lhs.cmp(&rhs))
            });
    }

    let mut draw = deterministic_unit_random(space.pointer, space.selector_nonce) * total_weight;
    for (&idx, weight) in candidates.iter().zip(weights.iter()) {
        draw -= *weight;
        if draw <= 0.0 {
            return Some(idx);
        }
    }

    candidates.last().copied()
}

fn biased_score(pointer: f64, angle: f64, gap: f64, bias_lambda: f64) -> f64 {
    let clockwise = clockwise_distance(pointer, angle);
    let counter_clockwise = counter_clockwise_distance(pointer, angle);
    if clockwise <= counter_clockwise {
        clockwise / gap
    } else {
        counter_clockwise / gap + bias_lambda.max(0.0)
    }
}

fn clockwise_distance(pointer: f64, angle: f64) -> f64 {
    (angle - pointer).rem_euclid(TAU)
}

fn counter_clockwise_distance(pointer: f64, angle: f64) -> f64 {
    (pointer - angle).rem_euclid(TAU)
}

fn deterministic_unit_random(pointer: f64, nonce: u64) -> f64 {
    let seed = pointer.to_bits()
        ^ nonce.wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ 0xd1b5_4a32_d192_ed03;
    let mut rng = StdRng::seed_from_u64(seed);
    rand::Rng::r#gen::<f64>(&mut rng)
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
    fn test_select_updates_nonce() {
        let mut space = build_space(make_test_wallpapers(5), 42);
        let old_nonce = space.selector_nonce;

        select_next(&mut space);

        assert_eq!(space.selector_nonce, old_nonce + 1);
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

    #[test]
    fn test_clockwise_bias_can_outweigh_small_left_advantage() {
        let mut space = build_space(make_test_wallpapers(10), 42);
        space.pointer = 17_f64.to_radians();
        space.selector_nonce = 0;
        space.cooldown_queue.clear();
        space.current_index = None;

        let result = select_next_with_config(&mut space, SelectionConfig {
            bias_lambda: 0.35,
            temperature: 1e-6,
        });

        assert!(result.is_some());
        assert_eq!(result.unwrap().index, 1);
    }

    #[test]
    fn test_angular_distance_still_matches_symmetry_expectation() {
        let left = 0_f64.to_radians();
        let right = 36_f64.to_radians();
        let pointer = 17_f64.to_radians();
        assert!(super::super::golden::angular_distance(pointer, left)
            < super::super::golden::angular_distance(pointer, right));
    }
}
