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

// --- 壁纸管理输出 ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiListOutput {
    pub mode: RunMode,
    pub active: Vec<ApiWallpaperInfo>,
    pub locked: Vec<ApiWallpaperInfo>,
    pub inactive: Vec<ApiWallpaperInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiWallpaperInfo {
    pub path: PathBuf,
    pub weight: f64,
    pub locked: bool,
    pub skip_streak: u32,
    pub last_played: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiLockOutput {
    pub path: PathBuf,
    pub locked: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiStatsOutput {
    pub mode: RunMode,
    pub total_count: usize,
    pub active_count: usize,
    pub locked_count: usize,
    pub min_value: f64,
    pub max_value: f64,
    pub avg_value: f64,
    pub total_skips: u64,
}

// --- 系统操作输出 ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiDiagnoseOutput {
    pub config_path: PathBuf,
    pub config_exists: bool,
    pub gpu_type: String,
    pub gpu_available: bool,
    pub gpu_reason: Option<String>,
    pub mpvpaper_installed: bool,
    pub swww_installed: bool,
    pub video_dir_exists: bool,
    pub video_count: usize,
    pub image_dir_exists: bool,
    pub image_count: usize,
    pub all_passed: bool,
    pub errors: Vec<String>,
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
