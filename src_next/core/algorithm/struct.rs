use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 权重记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightRecord {
    pub path: PathBuf,
    pub value: f64,
    pub skip_streak: u32,
    pub last_played: Option<u64>,
    /// 文件内容哈希（用于选择算法）
    #[serde(default)]
    pub content_hash: u64,
    /// 是否被锁定（锁定后不参与轮换）
    #[serde(default)]
    pub locked: bool,
}

// --- 权重更新配置 ---

#[derive(Debug, Clone)]
pub struct WeightUpdateConfig {
    /// 权重下限
    pub weight_min: f64,
    /// 权重上限
    pub weight_max: f64,
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
}

// --- 选择器 IO ---

#[derive(Debug, Clone)]
pub struct AlgorithmSelectInput {
    pub records: Vec<WeightRecord>,
    /// Top-N 百分比（0.0-1.0）
    pub top_n_percent: f64,
    /// 哈希混合字节数（0-8）
    pub hash_mix_bytes: u8,
    /// 当前系统种子
    pub system_seed: u64,
}

#[derive(Debug, Clone)]
pub struct AlgorithmSelectOutput {
    /// 选中的索引
    pub selected_index: usize,
    /// 选中的路径
    pub selected_path: PathBuf,
    /// 原始权重值
    pub original_value: f64,
    /// 候选数量（Top-N）
    pub candidate_count: usize,
    /// 混合哈希值（调试用）
    pub mixed_hash: u64,
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
    /// 权重下限
    pub weight_min: f64,
    /// 权重上限
    pub weight_max: f64,
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
