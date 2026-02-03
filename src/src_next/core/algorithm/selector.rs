use crate::core::algorithm::error::AlgorithmError;
use crate::core::algorithm::r#struct::{AlgorithmSelectInput, AlgorithmSelectOutput};
use rand::Rng;

/// 选择壁纸（带动态扰动）
///
/// 算法流程：
/// 1. 对所有权重应用动态扰动（扰动幅度与权重成比例）
/// 2. 按扰动后的权重排序
/// 3. 找到前 tolerance 范围内的所有壁纸
/// 4. 选择中间位置的壁纸（二分切割）
pub fn select(input: AlgorithmSelectInput) -> Result<AlgorithmSelectOutput, AlgorithmError> {
    if input.records.is_empty() {
        return Err(AlgorithmError::EmptyList);
    }

    // 应用动态扰动
    let mut rng = rand::thread_rng();
    let perturbed_values: Vec<(usize, f64, f64)> = input
        .records
        .iter()
        .enumerate()
        .map(|(idx, record)| {
            let random_factor = rng.gen_range(-1.0..1.0);
            let perturbation = record.value * input.perturbation_ratio * random_factor;
            let perturbed = (record.value + perturbation).max(1.0);
            (idx, perturbed, record.value) // (索引, 扰动后, 原始)
        })
        .collect();

    // 按扰动后的权重排序
    let mut sorted_indices = perturbed_values.clone();
    sorted_indices.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let max_value = sorted_indices[0].1;

    // 找到前 tolerance 范围内的所有壁纸
    let top_indices: Vec<(usize, f64, f64)> = sorted_indices
        .iter()
        .filter(|(_, perturbed, _)| (max_value - perturbed).abs() <= input.tolerance)
        .copied()
        .collect();

    // 二分切割：选择中间位置
    let selected = if top_indices.is_empty() {
        sorted_indices[0]
    } else {
        let mid_index = top_indices.len() / 2;
        top_indices[mid_index]
    };

    let (selected_index, perturbed_value, original_value) = selected;

    Ok(AlgorithmSelectOutput {
        selected_index,
        selected_path: input.records[selected_index].path.clone(),
        perturbed_value,
        original_value,
    })
}
