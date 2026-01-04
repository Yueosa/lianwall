use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 视频转码优化配置
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VideoOptimizationConfig {
    /// 是否启用自动优化
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// 转码缓存目录
    #[serde(default = "default_cache_dir")]
    pub cache_dir: String,

    /// 缓存大小限制（MB）
    #[serde(default = "default_max_cache_size_mb")]
    pub max_cache_size_mb: u64,

    /// 目标分辨率: "auto" | "1920" | "2560" | "3840" 等
    #[serde(default = "default_target_resolution")]
    pub target_resolution: String,

    /// 目标帧率: 24 | 30 | 60 | 120 | 144 | 165 | 180 | 240
    #[serde(default = "default_target_fps")]
    pub target_fps: u32,

    /// 预转码队列长度
    #[serde(default = "default_preload_count")]
    pub preload_count: usize,

    /// 视频编码器: "auto" | "nvenc" | "vaapi" | "libx264"
    #[serde(default = "default_encoder")]
    pub encoder: String,

    /// CRF 质量控制 (18-51)
    #[serde(default = "default_crf")]
    pub crf: u32,

    /// 编码速度预设
    #[serde(default = "default_preset")]
    pub preset: String,
}

fn default_enabled() -> bool {
    true
}
fn default_cache_dir() -> String {
    "~/.cache/lianwall/transcoded".to_string()
}
fn default_max_cache_size_mb() -> u64 {
    10240
}
fn default_target_resolution() -> String {
    "auto".to_string()
}
fn default_target_fps() -> u32 {
    30
}
fn default_preload_count() -> usize {
    3
}
fn default_encoder() -> String {
    "auto".to_string()
}
fn default_crf() -> u32 {
    23
}
fn default_preset() -> String {
    "fast".to_string()
}

impl Default for VideoOptimizationConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            cache_dir: default_cache_dir(),
            max_cache_size_mb: default_max_cache_size_mb(),
            target_resolution: default_target_resolution(),
            target_fps: default_target_fps(),
            preload_count: default_preload_count(),
            encoder: default_encoder(),
            crf: default_crf(),
            preset: default_preset(),
        }
    }
}

/// 运行时转码配置（解析后的）
#[derive(Debug, Clone)]
pub struct TranscodeConfig {
    pub target_width: u32,
    pub target_height: u32,
    pub target_fps: u32,
    pub encoder: String,
    pub crf: u32,
    pub preset: String,
    pub cache_dir: PathBuf,
    pub max_cache_size_mb: u64,
    pub preload_count: usize,
}

impl TranscodeConfig {
    /// 从 VideoOptimizationConfig 创建运行时配置
    pub fn from_video_optimization(vo_config: &VideoOptimizationConfig) -> Self {
        use crate::transcode::detector::detect_screen_resolution;

        // 解析目标分辨率
        let (width, height) = if vo_config.target_resolution == "auto" {
            detect_screen_resolution()
        } else {
            // 尝试解析数字（如 "2560"）或 "WxH" 格式
            if let Ok(w) = vo_config.target_resolution.parse::<u32>() {
                // 单个数字表示宽度，高度按 16:9 计算
                let h = (w as f32 * 9.0 / 16.0).round() as u32;
                (w, h)
            } else {
                // 尝试解析 "WxH" 格式
                let parts: Vec<&str> = vo_config.target_resolution.split('x').collect();
                if parts.len() == 2 {
                    if let (Ok(w), Ok(h)) = (parts[0].parse(), parts[1].parse()) {
                        (w, h)
                    } else {
                        detect_screen_resolution()
                    }
                } else {
                    detect_screen_resolution()
                }
            }
        };

        // 解析编码器
        let encoder = if vo_config.encoder == "auto" {
            use crate::transcode::detector::detect_available_encoder;
            detect_available_encoder()
        } else {
            vo_config.encoder.clone()
        };

        // 展开缓存目录路径
        let cache_dir = Config::expand_path(&vo_config.cache_dir);

        Self {
            target_width: width,
            target_height: height,
            target_fps: vo_config.target_fps,
            encoder,
            crf: vo_config.crf,
            preset: vo_config.preset.clone(),
            cache_dir,
            max_cache_size_mb: vo_config.max_cache_size_mb,
            preload_count: vo_config.preload_count,
        }
    }
}
