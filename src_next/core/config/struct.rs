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
    pub video_dir: String,
    pub image_dir: String,
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
    pub base: f64,
    pub select_penalty: f64,
    pub perturbation_ratio: f64,
    pub tolerance: f64,
    pub normalization_threshold: f64,
    pub normalization_target: f64,
    pub shuffle_period: u32,
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
