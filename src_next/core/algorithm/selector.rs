use crate::core::algorithm::error::AlgorithmError;
use crate::core::algorithm::r#struct::{AlgorithmSelectInput, AlgorithmSelectOutput};

/// 混合哈希：前 mix_bytes 字节与 seed 异或，后面保持原哈希
///
/// mix_bytes: 0 = 完全确定性（只用 content_hash）
/// mix_bytes: 8 = 完全随机（完全异或）
fn mixed_hash(content_hash: u64, seed: u64, mix_bytes: u8) -> u64 {
    let mix_bytes = mix_bytes.min(8);
    if mix_bytes == 0 {
        return content_hash;
    }
    if mix_bytes == 8 {
        return content_hash ^ seed;
    }

    // 计算需要保留的低位数量
    let keep_bits = (8 - mix_bytes) * 8;
    let mask = (1u64 << keep_bits) - 1; // 低位 mask

    // 低位保持 content_hash，高位异或
    let kept_low = content_hash & mask;
    let mixed_high = (content_hash ^ seed) & !mask;

    mixed_high | kept_low
}

/// 选择壁纸（基于权重 Top-N + 哈希亲和度）
///
/// 算法流程：
/// 1. 过滤掉锁定的壁纸
/// 2. 按权重降序排序
/// 3. 取前 top_n_percent 的壁纸作为候选
/// 4. 计算每个候选的混合哈希
/// 5. 选择与 system_seed 亲和度最高（XOR 最小）的壁纸
pub fn select(input: AlgorithmSelectInput) -> Result<AlgorithmSelectOutput, AlgorithmError> {
    // 过滤掉锁定的壁纸
    let active_records: Vec<(usize, &_)> = input
        .records
        .iter()
        .enumerate()
        .filter(|(_, r)| !r.locked)
        .collect();

    if active_records.is_empty() {
        return Err(AlgorithmError::EmptyList);
    }

    // 按权重降序排序
    let mut sorted: Vec<(usize, f64, u64)> = active_records
        .iter()
        .map(|(idx, r)| (*idx, r.value, r.content_hash))
        .collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // 计算 Top-N 数量（至少 1 个，最多全部）
    let top_n = ((sorted.len() as f64 * input.top_n_percent).ceil() as usize)
        .max(1)
        .min(sorted.len());

    let candidates = &sorted[..top_n];

    // 计算混合哈希并找到亲和度最高的（XOR 最小）
    let (selected_idx, original_value, _content_hash, mixed) = candidates
        .iter()
        .map(|(idx, value, hash)| {
            let mixed = mixed_hash(*hash, input.system_seed, input.hash_mix_bytes);
            (*idx, *value, *hash, mixed)
        })
        .min_by_key(|(_, _, _, mixed)| *mixed ^ input.system_seed)
        .unwrap(); // candidates 非空，unwrap 安全

    Ok(AlgorithmSelectOutput {
        selected_index: selected_idx,
        selected_path: input.records[selected_idx].path.clone(),
        original_value,
        candidate_count: top_n,
        mixed_hash: mixed,
    })
}
