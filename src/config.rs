#![allow(dead_code)]

use crate::transcode::config::VideoOptimizationConfig;
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
    /// 扰动幅度（相对百分比，如 0.03 表示 ±3%）
    #[serde(default = "default_perturbation_ratio")]
    pub perturbation_ratio: f64,
    /// 自动归一化阈值（平均权重超过此值时触发归一化）
    #[serde(default = "default_normalization_threshold")]
    pub normalization_threshold: f64,
    /// 归一化目标值（归一化后的平均权重）
    #[serde(default = "default_normalization_target")]
    pub normalization_target: f64,
    /// 洗牌周期（每N次选择后触发一次洗牌，0表示禁用）
    #[serde(default = "default_shuffle_period")]
    pub shuffle_period: u32,
    /// 洗牌强度（每次洗牌重置的壁纸比例，0.0-1.0）
    #[serde(default = "default_shuffle_intensity")]
    pub shuffle_intensity: f64,
}

fn default_perturbation_ratio() -> f64 {
    0.03
}
fn default_normalization_threshold() -> f64 {
    500.0
}
fn default_normalization_target() -> f64 {
    100.0
}
fn default_shuffle_period() -> u32 {
    100
}
fn default_shuffle_intensity() -> f64 {
    0.1
}

/// 总配置结构
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub paths: PathsConfig,
    pub video_engine: VideoEngineConfig,
    pub image_engine: ImageEngineConfig,
    pub weight: WeightConfig,
    #[serde(default)]
    pub video_optimization: VideoOptimizationConfig,
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
                perturbation_ratio: 0.03,
                normalization_threshold: 500.0,
                normalization_target: 100.0,
                shuffle_period: 100,
                shuffle_intensity: 0.1,
            },
            video_optimization: VideoOptimizationConfig::default(),
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
            let content = fs::read_to_string(&config_path).expect("无法读取配置文件");
            toml::from_str(&content).expect("配置文件格式错误")
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

        let content = self.to_toml_with_comments();
        fs::write(&config_path, content).expect("无法写入配置文件");
    }

    /// 生成带注释的 TOML 配置
    fn to_toml_with_comments(&self) -> String {
        format!(
            r#"# ===================================
# LianWall 配置文件
# ===================================

# === 路径配置 ===
# 定义缓存文件和壁纸目录的位置
[paths]
# 动态壁纸权重缓存文件路径
#     用于保存视频壁纸的权重状态，支持持久化记忆
video_cache = "{}"

# 静态壁纸权重缓存文件路径
#     用于保存图片壁纸的权重状态
image_cache = "{}"

# 动态壁纸目录
#     存放视频壁纸文件的目录（支持 mp4, mkv, webm 等格式）
video_dir = "{}"

# 静态壁纸目录
#     存放图片壁纸文件的目录（支持 jpg, png, gif 等格式）
image_dir = "{}"

# === 动态壁纸引擎配置 ===
# 控制视频壁纸的播放行为
[video_engine]
# 引擎类型：当前仅支持 "mpvpaper"
type = "{}"

# 切换间隔（秒）
#     每隔多少秒自动切换到下一个视频壁纸
#     默认 600 秒（10 分钟）
interval = {}

# === 静态壁纸引擎配置 ===
# 控制图片壁纸的切换和过渡效果
[image_engine]
# 引擎类型：当前仅支持 "swww"
type = "{}"

# 切换间隔（秒）
#     每隔多少秒自动切换到下一张图片壁纸
#     默认 300 秒（5 分钟）
interval = {}

# 过渡效果
#     可选值：fade, left, right, top, bottom, wipe, wave, grow, center, any, outer, random
transition = "{}"

# 过渡时长（秒）
#     切换壁纸时的动画持续时间
transition_duration = {}

# === 权重算法配置 ===
# 控制智能选择算法的行为（零和博弈机制）
[weight]
# 基础权重
#     新壁纸的初始权重基准值，所有计算以此为中心
#     建议保持默认值 100.0
base = {}

# 选中惩罚值
#     壁纸被播放后权重减少的数值
#     增大此值 → 冷却期更长，重复播放间隔更大
#     默认 10.0
select_penalty = {}

# 扰动幅度比例
#     选择时应用的随机扰动强度（相对于当前权重的百分比）
#     0.03 表示 ±3% 的随机波动
#     增大此值 → 随机性增强，打破确定性循环
#     减小此值 → 更倾向于严格按权重选择
#     建议范围：0.01 - 0.10
perturbation_ratio = {}

# 自动归一化阈值
#     当所有壁纸的平均权重超过此值时，自动触发权重缩放
#     防止权重无限膨胀导致数值溢出
#     默认 500.0（当平均权重达到 500 时触发归一化）
normalization_threshold = {}

# 归一化目标值
#     触发归一化后，将平均权重调整到此目标值
#     所有壁纸的权重按比例缩放
#     默认 100.0（与基础权重保持一致）
normalization_target = {}

# 洗牌周期（轮数）
#     每经过 N 次壁纸切换后，随机重置部分壁纸的权重
#     用于打破生态锁定，引入周期性的"重新洗牌"
#     设为 0 表示禁用洗牌机制
#     默认 100（每 100 次切换洗一次牌）
shuffle_period = {}

# 洗牌强度
#     每次洗牌时，重置多少比例的壁纸权重
#     0.1 表示随机重置 10% 的壁纸
#     增大此值 → 洗牌力度更大，随机性更强
#     减小此值 → 洗牌影响更温和
#     建议范围：0.05 - 0.20
shuffle_intensity = {}

# ================================================
# === 视频转码优化配置 ===
# ================================================
[video_optimization]

# 启用自动视频转码
#     当视频分辨率超过屏幕分辨率时，自动转码为合适的尺寸
#     可减少 mpvpaper 的显存占用（4K 视频可从 1.3GB 降至 300MB+）
#     默认启用
enabled = {}

# 转码缓存目录
#     存放转码后的视频文件
#     建议使用 SSD 路径以加快加载速度
cache_dir = "{}"

# 缓存大小限制（MB）
#     当缓存总大小超过此值时，自动清理最近使用过的文件
#     注意：删除"最近使用"而非"最久未使用"，因为零和博弈算法会让冷门文件权重回升
#     默认 10240 MB（10 GB）
max_cache_size_mb = {}

# 目标分辨率
#     转码后的视频宽度（像素）
#     默认 "auto" 表示自动检测屏幕最小分辨率
#     也可手动指定，如 "2560" 或 "1920"
target_resolution = "{}"

# 目标帧率（FPS）
#     转码后的视频帧率
#     可选值：24, 30, 60, 120, 144, 165, 180, 240
#     较低的帧率可显著减少文件大小和显存占用
#     默认 30
target_fps = {}

# 预加载队列大小
#     在后台提前转码接下来的 N 个视频
#     增大此值可减少等待时间，但会占用更多 CPU 和磁盘 I/O
#     默认 3
preload_count = {}

# 编码器
#     视频编码器类型
#     "auto"：自动检测（优先级：nvenc > vaapi > libx264）
#     "nvenc"：NVIDIA GPU 硬件加速（需要 NVIDIA 显卡）
#     "vaapi"：Intel/AMD GPU 硬件加速（Linux）
#     "libx264"：CPU 软编码（速度慢但兼容性最好）
#     默认 "auto"
encoder = "{}"

# 编码质量（CRF）
#     恒定质量因子，范围 0-51，数值越小质量越高
#     推荐值：
#       - 18-20：接近无损（文件较大）
#       - 23-25：高质量（推荐）
#       - 28-30：中等质量（显著减小文件）
#     默认 23
crf = {}

# 编码速度预设
#     可选值：ultrafast, superfast, veryfast, faster, fast, medium, slow, slower, veryslow
#     faster 预设 → 转码速度更快，压缩率略低
#     slower 预设 → 转码速度慢，压缩率更高
#     默认 "fast"
preset = "{}"
"#,
            self.paths.video_cache,
            self.paths.image_cache,
            self.paths.video_dir,
            self.paths.image_dir,
            self.video_engine.engine_type,
            self.video_engine.interval,
            self.image_engine.engine_type,
            self.image_engine.interval,
            self.image_engine.transition,
            self.image_engine.transition_duration,
            self.weight.base,
            self.weight.select_penalty,
            self.weight.perturbation_ratio,
            self.weight.normalization_threshold,
            self.weight.normalization_target,
            self.weight.shuffle_period,
            self.weight.shuffle_intensity,
            self.video_optimization.enabled,
            self.video_optimization.cache_dir,
            self.video_optimization.max_cache_size_mb,
            self.video_optimization.target_resolution,
            self.video_optimization.target_fps,
            self.video_optimization.preload_count,
            self.video_optimization.encoder,
            self.video_optimization.crf,
            self.video_optimization.preset,
        )
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
