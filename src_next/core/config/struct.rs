use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub paths: PathsConfig,
    pub video_engine: VideoEngineConfig,
    pub image_engine: ImageEngineConfig,
    pub weight: WeightConfig,
    pub vram: VramConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    pub mode: String,
    pub video_dir: PathBuf,
    pub image_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoEngineConfig {
    pub interval: u64,
    pub mpv_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEngineConfig {
    pub interval: u64,
    pub swww_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightConfig {
    /// 权重下限
    pub weight_min: f64,
    /// 权重上限
    pub weight_max: f64,
    /// 选中惩罚值（每次选中后减少的权重）
    pub select_penalty: f64,
    /// Top-N 百分比（选择时考虑前 N% 的壁纸）
    pub top_n_percent: f64,
    /// 哈希混合字节数（0-8，前 x 字节与 seed 异或）
    pub hash_mix_bytes: u8,
    /// Seed 重置周期（小时，0 表示每次选择都重置）
    pub seed_reset_hours: u32,
    /// 归一化阈值（平均权重超过此值时触发）
    pub normalization_threshold: f64,
    /// 归一化目标值
    pub normalization_target: f64,
    /// 洗牌周期（选择次数，0 表示禁用）
    pub shuffle_period: u32,
    /// 洗牌强度（0.0-1.0，重置的壁纸比例）
    pub shuffle_intensity: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VramConfig {
    pub enabled: bool,
    pub threshold_percent: f32,
    pub recovery_percent: f32,
    pub check_interval: u64,
}

// --- CRUD IO structs ---

#[derive(Debug, Clone)]
pub struct ConfigCreateInput {
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ConfigCreateOutput {
    pub path: PathBuf,
    pub config: Config,
    pub created: bool,
}

#[derive(Debug, Clone)]
pub struct ConfigReadInput {
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ConfigReadOutput {
    pub path: PathBuf,
    pub config: Config,
}

#[derive(Debug, Clone)]
pub struct ConfigUpdateInput {
    pub path: Option<PathBuf>,
    pub config: Config,
}

#[derive(Debug, Clone)]
pub struct ConfigUpdateOutput {
    pub path: PathBuf,
    pub modified: bool,
}

#[derive(Debug, Clone)]
pub struct ConfigDeleteInput {
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ConfigDeleteOutput {
    pub path: PathBuf,
    pub deleted: bool,
}
