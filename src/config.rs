#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 路径配置
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PathsConfig {
    /// 动态壁纸权重缓存
    pub video_cache: String,
    /// 静态壁纸权重缓存
    pub image_cache: String,
    /// 动态壁纸目录
    pub video_dir: String,
    /// 静态壁纸目录
    pub image_dir: String,
}

/// 动态壁纸引擎配置
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VideoEngineConfig {
    /// 引擎类型: "mpvpaper"
    #[serde(rename = "type")]
    pub engine_type: String,
    /// 切换间隔（秒）
    pub interval: u64,
}

/// 静态壁纸引擎配置
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImageEngineConfig {
    /// 引擎类型: "swww"
    #[serde(rename = "type")]
    pub engine_type: String,
    /// 切换间隔（秒）
    pub interval: u64,
    /// 过渡效果
    pub transition: String,
    /// 过渡时长（秒）
    pub transition_duration: f32,
}

/// 权重配置
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WeightConfig {
    /// 基础权重
    pub base: f64,
    /// 选中惩罚值
    pub select_penalty: f64,
    /// 未选中最大奖励
    pub skip_reward_max: f64,
}

/// 总配置结构
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub paths: PathsConfig,
    pub video_engine: VideoEngineConfig,
    pub image_engine: ImageEngineConfig,
    pub weight: WeightConfig,
    #[serde(skip)]
    pub current_mode: Option<String>,
}

/// 壁纸模式
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WallpaperMode {
    Video,
    Image,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            paths: PathsConfig {
                video_cache: "~/.cache/lianwall/video.json".to_string(),
                image_cache: "~/.cache/lianwall/image.json".to_string(),
                video_dir: "~/Videos/background".to_string(),
                image_dir: "~/Pictures/wallpapers".to_string(),
            },
            video_engine: VideoEngineConfig {
                engine_type: "mpvpaper".to_string(),
                interval: 600,
            },
            image_engine: ImageEngineConfig {
                engine_type: "swww".to_string(),
                interval: 300,
                transition: "fade".to_string(),
                transition_duration: 2.0,
            },
            weight: WeightConfig {
                base: 100.0,
                select_penalty: 10.0,
                skip_reward_max: 5.0,
            },
            current_mode: None,
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("lianwall/config.toml")
    }

    pub fn load() -> Self {
        let config_path = Self::config_path();
        
        if config_path.exists() {
            let content = fs::read_to_string(&config_path)
                .expect("无法读取配置文件");
            toml::from_str(&content)
                .expect("配置文件格式错误")
        } else {
            let config = Config::default();
            config.save();
            config
        }
    }

    pub fn save(&self) {
        let config_path = Self::config_path();
        
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        
        let content = toml::to_string_pretty(self)
            .expect("配置序列化失败");
        fs::write(&config_path, content)
            .expect("无法写入配置文件");
    }

    pub fn expand_path(path: &str) -> PathBuf {
        if path.starts_with("~/") {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/"))
                .join(&path[2..])
        } else {
            PathBuf::from(path)
        }
    }

    /// 根据模式获取缓存文件路径
    pub fn cache_path(&self, mode: WallpaperMode) -> PathBuf {
        match mode {
            WallpaperMode::Video => Self::expand_path(&self.paths.video_cache),
            WallpaperMode::Image => Self::expand_path(&self.paths.image_cache),
        }
    }

    /// 根据模式获取壁纸目录路径
    pub fn wallpaper_dir(&self, mode: WallpaperMode) -> PathBuf {
        match mode {
            WallpaperMode::Video => Self::expand_path(&self.paths.video_dir),
            WallpaperMode::Image => Self::expand_path(&self.paths.image_dir),
        }
    }

    /// 根据模式获取引擎类型
    pub fn engine_type(&self, mode: WallpaperMode) -> &str {
        match mode {
            WallpaperMode::Video => &self.video_engine.engine_type,
            WallpaperMode::Image => &self.image_engine.engine_type,
        }
    }

    /// 根据模式获取切换间隔
    pub fn interval(&self, mode: WallpaperMode) -> u64 {
        match mode {
            WallpaperMode::Video => self.video_engine.interval,
            WallpaperMode::Image => self.image_engine.interval,
        }
    }

    /// 获取展开后的视频目录路径（兼容旧代码）
    pub fn video_path(&self) -> PathBuf {
        Self::expand_path(&self.paths.video_dir)
    }

    /// 获取展开后的图片目录路径（兼容旧代码）
    pub fn image_path(&self) -> Option<PathBuf> {
        Some(Self::expand_path(&self.paths.image_dir))
    }

    /// 获取当前模式状态文件路径
    pub fn mode_state_path() -> PathBuf {
        dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("~/.cache"))
            .join("lianwall/current_mode")
    }

    /// 保存当前模式
    pub fn save_current_mode(mode: WallpaperMode) {
        let path = Self::mode_state_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let mode_str = match mode {
            WallpaperMode::Video => "video",
            WallpaperMode::Image => "image",
        };
        fs::write(&path, mode_str).ok();
    }

    /// 读取当前模式
    pub fn load_current_mode() -> WallpaperMode {
        let path = Self::mode_state_path();
        if let Ok(content) = fs::read_to_string(&path) {
            match content.trim() {
                "image" => WallpaperMode::Image,
                _ => WallpaperMode::Video,
            }
        } else {
            WallpaperMode::Video
        }
    }
}
