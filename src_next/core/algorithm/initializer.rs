use crate::core::algorithm::r#struct::{AlgorithmInitInput, AlgorithmInitOutput, WeightRecord};
use std::collections::HashMap;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::PathBuf;

/// 精确到 2 位小数
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// 计算文件内容哈希（读取前 64KB）
///
/// 使用文件头部内容计算哈希，快速且足够区分不同文件
fn compute_content_hash(path: &PathBuf) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    // 读取文件前 64KB
    if let Ok(mut file) = File::open(path) {
        let mut buffer = [0u8; 65536];
        if let Ok(n) = file.read(&mut buffer) {
            buffer[..n].hash(&mut hasher);
        }
    }

    // 文件路径也参与哈希（确保即使内容相同，路径不同也有不同哈希）
    path.hash(&mut hasher);

    hasher.finish()
}

/// 初始化权重记录
///
/// 算法：
/// 1. 已存在于缓存中的文件：复用缓存权重（并更新哈希）
/// 2. 新文件：基于内容哈希均匀分布到 [weight_min, weight_max] 区间
///    - 哈希值映射到 [0, 1] 区间
///    - 线性映射到权重区间
///    - 精确到 2 位小数
pub fn initialize(input: AlgorithmInitInput) -> AlgorithmInitOutput {
    // 构建缓存 map
    let cached_map: HashMap<PathBuf, WeightRecord> = input
        .cached_records
        .into_iter()
        .map(|r| (r.path.clone(), r))
        .collect();

    let weight_min = input.weight_min;
    let weight_max = input.weight_max;
    let weight_span = weight_max - weight_min;

    let mut new_count = 0;
    let mut cached_count = 0;

    let records: Vec<WeightRecord> = input
        .wallpapers
        .into_iter()
        .map(|path| {
            // 计算内容哈希
            let content_hash = compute_content_hash(&path);

            if let Some(mut cached_record) = cached_map.get(&path).cloned() {
                cached_count += 1;
                // 更新哈希（文件可能被修改）
                cached_record.content_hash = content_hash;
                // 确保权重在区间内且精度正确
                cached_record.value = round2(cached_record.value.clamp(weight_min, weight_max));
                cached_record
            } else {
                new_count += 1;

                // 基于哈希均匀分布权重
                // 将 u64 哈希映射到 [0, 1]
                let hash_ratio = (content_hash as f64) / (u64::MAX as f64);
                // 映射到权重区间
                let initial_weight = round2(weight_min + hash_ratio * weight_span);

                WeightRecord {
                    path,
                    value: initial_weight,
                    skip_streak: 0,
                    last_played: None,
                    content_hash,
                    locked: false,
                }
            }
        })
        .collect();

    let total: f64 = records.iter().map(|r| r.value).sum();
    let avg_weight = if records.is_empty() {
        0.0
    } else {
        round2(total / records.len() as f64)
    };

    AlgorithmInitOutput {
        records,
        new_count,
        cached_count,
        avg_weight,
    }
}
