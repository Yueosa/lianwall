use crate::core::algorithm::r#struct::{AlgorithmInitInput, AlgorithmInitOutput, WeightRecord};
use std::collections::HashMap;
use std::path::PathBuf;

/// 初始化权重记录
///
/// 逻辑：
/// 1. 已存在于缓存中的文件：复用缓存权重
/// 2. 新文件：计算基于文件修改时间的初始权重，并与平均权重混合
pub fn initialize(input: AlgorithmInitInput) -> AlgorithmInitOutput {
    // 构建缓存 map
    let cached_map: HashMap<PathBuf, WeightRecord> = input
        .cached_records
        .into_iter()
        .map(|r| (r.path.clone(), r))
        .collect();

    // 计算平均权重（用于新文件初始化）
    let avg_value = if cached_map.is_empty() {
        input.base_weight
    } else {
        let sum: f64 = cached_map.values().map(|r| r.value).sum();
        sum / cached_map.len() as f64
    };

    let mut new_count = 0;
    let mut cached_count = 0;

    let records: Vec<WeightRecord> = input
        .wallpapers
        .into_iter()
        .map(|path| {
            if let Some(cached_record) = cached_map.get(&path) {
                cached_count += 1;
                cached_record.clone()
            } else {
                new_count += 1;
                // 新文件：使用平均权重
                // TODO: 可以考虑基于文件修改时间计算权重（需要传入文件元数据）
                WeightRecord {
                    path,
                    value: avg_value,
                    skip_streak: 0,
                    last_played: None,
                }
            }
        })
        .collect();

    let total: f64 = records.iter().map(|r| r.value).sum();
    let final_avg = if records.is_empty() {
        0.0
    } else {
        total / records.len() as f64
    };

    AlgorithmInitOutput {
        records,
        new_count,
        cached_count,
        avg_weight: final_avg,
    }
}
