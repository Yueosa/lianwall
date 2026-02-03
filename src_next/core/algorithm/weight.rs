use crate::core::algorithm::error::AlgorithmError;
use crate::core::algorithm::r#struct::{AlgorithmUpdateInput, AlgorithmUpdateOutput, WeightRecord};
use rand::Rng;

/// 更新所有权重（零和博弈实现）
///
/// 核心逻辑：
/// - 选中壁纸减少 penalty
/// - 其他壁纸平均分配这个 penalty
/// - 总权重保持不变 ⇒ 零和博弈
pub fn update_weights(input: AlgorithmUpdateInput) -> Result<AlgorithmUpdateOutput, AlgorithmError> {
    if input.records.is_empty() {
        return Err(AlgorithmError::EmptyList);
    }

    if input.selected_index >= input.records.len() {
        return Err(AlgorithmError::InvalidIndex {
            index: input.selected_index,
            length: input.records.len(),
        });
    }

    let mut updated_records = input.records.clone();
    let penalty = input.config.select_penalty;
    let other_count = updated_records.len() - 1;

    if other_count > 0 {
        let reward_per_wallpaper = penalty / other_count as f64;

        for (idx, record) in updated_records.iter_mut().enumerate() {
            if idx == input.selected_index {
                // 选中壁纸：扣除惩罚
                record.value = (record.value - penalty).max(1.0);
                record.skip_streak = 0;
                // 更新播放时间
                record.last_played = Some(get_current_timestamp());
            } else {
                // 未选中壁纸：获得奖励
                record.value += reward_per_wallpaper;
                record.skip_streak += 1;
            }
        }
    } else {
        // 只有一张壁纸时，仍需要更新播放状态
        let record = &mut updated_records[input.selected_index];
        record.value = (record.value - penalty).max(1.0);
        record.skip_streak = 0;
        record.last_played = Some(get_current_timestamp());
    }

    let mut normalized = false;
    let mut shuffled = false;
    let mut avg_before_normalize = None;
    let mut shuffle_count = None;

    // 检查是否需要洗牌
    if input.config.shuffle_period > 0
        && input.selection_count > 0
        && input.selection_count % input.config.shuffle_period == 0
    {
        let count = apply_shuffle(&mut updated_records, &input.config);
        shuffled = true;
        shuffle_count = Some(count);
    }

    // 检查是否需要归一化
    let total: f64 = updated_records.iter().map(|r| r.value).sum();
    let avg = total / updated_records.len() as f64;

    if avg > input.config.normalization_threshold {
        avg_before_normalize = Some(avg);
        apply_normalization(&mut updated_records, avg, input.config.normalization_target);
        normalized = true;
    }

    Ok(AlgorithmUpdateOutput {
        updated_records,
        normalized,
        shuffled,
        avg_before_normalize,
        shuffle_count,
    })
}

/// 自动归一化：当平均权重超过阈值时，将所有权重按比例缩放
fn apply_normalization(records: &mut [WeightRecord], current_avg: f64, target: f64) {
    let scale_factor = target / current_avg;

    for record in records.iter_mut() {
        record.value *= scale_factor;
    }
}

/// 周期性洗牌：随机重置部分壁纸权重，打破生态锁定
///
/// 返回重置的壁纸数量
fn apply_shuffle(records: &mut [WeightRecord], config: &crate::core::algorithm::r#struct::WeightUpdateConfig) -> usize {
    if records.is_empty() || config.shuffle_intensity <= 0.0 {
        return 0;
    }

    let shuffle_count =
        ((records.len() as f64 * config.shuffle_intensity).ceil() as usize).min(records.len());

    if shuffle_count == 0 {
        return 0;
    }

    let mut rng = rand::thread_rng();
    let mut indices: Vec<usize> = (0..records.len()).collect();

    // Fisher-Yates 洗牌
    for i in (1..indices.len()).rev() {
        let j = rng.gen_range(0..=i);
        indices.swap(i, j);
    }

    for i in 0..shuffle_count {
        let idx = indices[i];
        // 重置为基础权重附近的随机值（±20%）
        let random_offset = rng.gen_range(-0.2..0.2);
        records[idx].value = config.base_weight * (1.0 + random_offset);
        records[idx].skip_streak = 0;
    }

    shuffle_count
}

/// 获取当前时间戳（秒）
fn get_current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
