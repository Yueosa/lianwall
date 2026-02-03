use crate::core::algorithm::r#struct::{AlgorithmStatsOutput, WeightRecord};

/// 获取权重统计信息
pub fn get_stats(records: &[WeightRecord]) -> AlgorithmStatsOutput {
    if records.is_empty() {
        return AlgorithmStatsOutput {
            count: 0,
            min_value: 0.0,
            max_value: 0.0,
            avg_value: 0.0,
            total_skips: 0,
        };
    }

    let values: Vec<f64> = records.iter().map(|r| r.value).collect();
    let sum: f64 = values.iter().sum();
    let count = values.len() as f64;

    AlgorithmStatsOutput {
        count: records.len(),
        min_value: values.iter().cloned().fold(f64::INFINITY, f64::min),
        max_value: values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        avg_value: sum / count,
        total_skips: records.iter().map(|r| r.skip_streak as u64).sum(),
    }
}
