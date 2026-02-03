use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 权重记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightRecord {
    pub path: PathBuf,
    pub value: f64,
    pub skip_streak: u32,
    pub last_played: Option<u64>,
    /// 是否被锁定（锁定后不参与轮换）
    #[serde(default)]
    pub locked: bool,
}

// --- 权重更新配置 ---

#[derive(Debug, Clone)]
pub struct WeightUpdateConfig {
    /// 选中惩罚值
    pub select_penalty: f64,
    /// 自动归一化阈值
    pub normalization_threshold: f64,
    /// 归一化目标值
    pub normalization_target: f64,
    /// 洗牌周期（0 表示禁用）
    pub shuffle_period: u32,
    /// 洗牌强度（0.0-1.0）
    pub shuffle_intensity: f64,
    /// 基础权重（用于洗牌时重置）
    pub base_weight: f64,
}

// --- 选择器 IO ---

#[derive(Debug, Clone)]
pub struct AlgorithmSelectInput {
    pub records: Vec<WeightRecord>,
    /// 容忍度（前 N 范围内的权重差异）
    pub tolerance: f64,
    /// 扰动比例（如 0.03 表示 ±3%）
    pub perturbation_ratio: f64,
}

#[derive(Debug, Clone)]
pub struct AlgorithmSelectOutput {
    /// 选中的索引
    pub selected_index: usize,
    /// 选中的路径
    pub selected_path: PathBuf,
    /// 扰动后的权重值（用于调试）
    pub perturbed_value: f64,
    /// 原始权重值
    pub original_value: f64,
}

// --- 权重更新 IO ---

#[derive(Debug, Clone)]
pub struct AlgorithmUpdateInput {
    pub records: Vec<WeightRecord>,
    pub selected_index: usize,
    pub config: WeightUpdateConfig,
    /// 当前选择计数（用于判断是否洗牌）
    pub selection_count: u32,
}

#[derive(Debug, Clone)]
pub struct AlgorithmUpdateOutput {
    pub updated_records: Vec<WeightRecord>,
    /// 是否触发了归一化
    pub normalized: bool,
    /// 是否触发了洗牌
    pub shuffled: bool,
    /// 归一化前的平均权重（如果触发）
    pub avg_before_normalize: Option<f64>,
    /// 洗牌重置的壁纸数量（如果触发）
    pub shuffle_count: Option<usize>,
}

// --- 初始化 IO ---

#[derive(Debug, Clone)]
pub struct AlgorithmInitInput {
    /// 扫描到的壁纸路径列表
    pub wallpapers: Vec<PathBuf>,
    /// 缓存的权重记录
    pub cached_records: Vec<WeightRecord>,
    /// 基础权重
    pub base_weight: f64,
}

#[derive(Debug, Clone)]
pub struct AlgorithmInitOutput {
    pub records: Vec<WeightRecord>,
    /// 新增文件数
    pub new_count: usize,
    /// 复用缓存数
    pub cached_count: usize,
    /// 平均权重
    pub avg_weight: f64,
}

// --- 统计信息 ---

#[derive(Debug, Clone)]
pub struct AlgorithmStatsOutput {
    pub count: usize,
    pub min_value: f64,
    pub max_value: f64,
    pub avg_value: f64,
    pub total_skips: u64,
}
