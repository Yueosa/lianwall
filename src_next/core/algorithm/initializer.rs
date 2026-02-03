use crate::core::algorithm::r#struct::{AlgorithmInitInput, AlgorithmInitOutput, WeightRecord};
use rand::Rng;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

/// 初始化权重记录
///
/// 逻辑：
/// 1. 已存在于缓存中的文件：复用缓存权重
/// 2. 新文件：基于文件修改时间 + 随机扰动计算差异化初始权重
///    - 较新的文件获得较高的基础权重
///    - 叠加随机扰动（±15%）确保即使同时复制的文件也有差异
///    - 总体区间为 avg_value ±30%
pub fn initialize(input: AlgorithmInitInput) -> AlgorithmInitOutput {
    // 构建缓存 map
    let cached_map: HashMap<PathBuf, WeightRecord> = input
        .cached_records
        .into_iter()
        .map(|r| (r.path.clone(), r))
        .collect();

    // 计算平均权重（用于新文件初始化的基准）
    let avg_value = if cached_map.is_empty() {
        input.base_weight
    } else {
        let sum: f64 = cached_map.values().map(|r| r.value).sum();
        sum / cached_map.len() as f64
    };

    let mut new_count = 0;
    let mut cached_count = 0;

    // 收集新文件的时间戳
    let new_files: Vec<(PathBuf, u64)> = input
        .wallpapers
        .iter()
        .filter(|path| !cached_map.contains_key(*path))
        .filter_map(|path| {
            let mtime = get_file_mtime(path)?;
            Some((path.clone(), mtime))
        })
        .collect();

    // 计算时间戳范围，用于归一化
    let (min_time, max_time) = if new_files.is_empty() {
        (0, 1)
    } else {
        let min = new_files.iter().map(|(_, t)| *t).min().unwrap_or(0);
        let max = new_files.iter().map(|(_, t)| *t).max().unwrap_or(1);
        // 确保不会除以零
        if min == max {
            (min, max + 1)
        } else {
            (min, max)
        }
    };

    // 新文件权重区间：avg_value ± 30%（确保新文件间有足够差异）
    // 时间戳贡献 ±15%，随机扰动贡献 ±15%
    let time_range = avg_value * 0.3; // 时间戳区间
    let time_base = avg_value - time_range / 2.0;
    let random_range = avg_value * 0.3; // 随机扰动区间

    // 构建新文件的时间戳映射
    let new_file_map: HashMap<PathBuf, u64> = new_files.into_iter().collect();
    let mut rng = rand::thread_rng();

    let records: Vec<WeightRecord> = input
        .wallpapers
        .into_iter()
        .map(|path| {
            if let Some(cached_record) = cached_map.get(&path) {
                cached_count += 1;
                cached_record.clone()
            } else {
                new_count += 1;

                // 基于时间戳计算基础权重
                let time_weight = if let Some(&mtime) = new_file_map.get(&path) {
                    let time_ratio =
                        (mtime - min_time) as f64 / (max_time - min_time).max(1) as f64;
                    time_base + time_ratio * time_range
                } else {
                    avg_value
                };

                // 叠加随机扰动（±15%），确保即使同时间戳的文件也有差异
                let random_offset = rng.gen_range(-0.5..0.5) * random_range;
                let initial_weight = (time_weight + random_offset).max(1.0);

                WeightRecord {
                    path,
                    value: initial_weight,
                    skip_streak: 0,
                    last_played: None,
                    locked: false,
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

/// 获取文件修改时间（Unix 时间戳）
fn get_file_mtime(path: &PathBuf) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}
