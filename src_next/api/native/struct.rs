use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::api::native::debug::DebugTrace;
use crate::core::runtime::RunMode;

// --- 通用输出包装 ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub result: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<ApiDebugInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiDebugInfo {
    pub total_duration_ms: u64,
    pub trace: Vec<DebugTrace>,
}

impl<T> ApiResponse<T> {
    pub fn success(result: T, debug: Option<ApiDebugInfo>) -> Self {
        Self { result, debug }
    }
}

// --- 核心操作输出 ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiStartOutput {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiNextOutput {
    pub selected_path: PathBuf,
    pub mode: RunMode,
    pub normalized: bool,
    pub shuffled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSwitchModeOutput {
    pub old_mode: RunMode,
    pub new_mode: RunMode,
    pub wallpaper: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiReloadOutput {
    pub total_count: usize,
    pub active_count: usize,
    pub new_count: usize,
    pub removed_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiStopOutput {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiStatusOutput {
    pub current_mode: RunMode,
    pub current_wallpaper: Option<PathBuf>,
    pub is_running: bool,
    pub selection_count: u32,
    pub video_stats: Option<ModeStatsOutput>,
    pub image_stats: Option<ModeStatsOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeStatsOutput {
    pub total_count: usize,
    pub active_count: usize,
    pub locked_count: usize,
    pub min_value: f64,
    pub max_value: f64,
    pub avg_value: f64,
}

// --- 系统操作输出 ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiDiagnoseOutput {
    pub gpu_available: bool,
    pub gpu_type: String,
    pub mpvpaper_available: bool,
    pub swww_available: bool,
    pub config_path: PathBuf,
    pub vram_info: Option<VramInfoOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VramInfoOutput {
    pub total_mb: u32,
    pub used_mb: u32,
    pub free_mb: u32,
    pub usage_percent: f64,
    pub free_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiUninstallOutput {
    pub removed_items: Vec<String>,
    pub note: String,
}

// --- 配置操作输出 ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfigGetOutput {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfigSetOutput {
    pub key: String,
    pub old_value: String,
    pub new_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfigShowOutput {
    pub config_toml: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfigResetOutput {
    pub message: String,
    pub backup_path: Option<PathBuf>,
}
