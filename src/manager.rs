use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use walkdir::WalkDir;

use crate::algorithm::{WallpaperSelector, WeightCalculator};
use crate::config::{Config, WallpaperMode};
use crate::paperengine::{PaperEngine, create_engine, supported_extensions};

/// 壁纸数据结构
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Wallpaper {
    pub path: PathBuf,
    pub value: f64,
    pub skip_streak: u32,
    pub last_played: Option<u64>,
}

/// 壁纸管理器
pub struct WallManager {
    pub config: Config,
    pub mode: WallpaperMode,
    pub wallpapers: Vec<Wallpaper>,
    pub engine: Box<dyn PaperEngine>,
    weight_calc: WeightCalculator,
}

impl WallManager {
    /// 初始化壁纸管理器
    pub fn new(config: Config, mode: WallpaperMode) -> Self {
        let engine_type = config.engine_type(mode);
        let engine = create_engine(engine_type);

        let weight_calc = WeightCalculator::new(config.weight.clone());

        let mut manager = Self {
            config,
            mode,
            wallpapers: Vec::new(),
            engine,
            weight_calc,
        };

        manager.load_and_scan();
        manager
    }

    /// 加载缓存文件并扫描目录，合并权重
    fn load_and_scan(&mut self) {
        let cache_path = self.config.cache_path(self.mode);
        let scan_dir = self.config.wallpaper_dir(self.mode);
        let engine_type = self.config.engine_type(self.mode);

        let extensions = supported_extensions(engine_type);

        let cached: Vec<Wallpaper> = if cache_path.exists() {
            let content = fs::read_to_string(&cache_path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        let cached_map: std::collections::HashMap<PathBuf, Wallpaper> =
            cached.into_iter().map(|w| (w.path.clone(), w)).collect();

        let mut scanned_files: Vec<(PathBuf, SystemTime)> = Vec::new();

        for entry in WalkDir::new(&scan_dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext_lower = ext.to_string_lossy().to_lowercase();
                    if extensions.iter().any(|&e| e == ext_lower) {
                        let mtime = fs::metadata(path)
                            .and_then(|m| m.modified())
                            .unwrap_or(SystemTime::UNIX_EPOCH);
                        scanned_files.push((path.to_path_buf(), mtime));
                    }
                }
            }
        }

        if scanned_files.is_empty() {
            let mode_str = match self.mode {
                WallpaperMode::Video => "动态壁纸",
                WallpaperMode::Image => "静态壁纸",
            };
            println!(
                "警告: 在 {} 中未找到支持的{}文件 ({})",
                scan_dir.display(),
                mode_str,
                extensions.join(", ")
            );
            return;
        }

        let (oldest, newest) = scanned_files.iter().fold(
            (SystemTime::UNIX_EPOCH, SystemTime::now()),
            |(oldest, newest), (_, mtime)| {
                (
                    if *mtime < oldest { oldest } else { *mtime },
                    if *mtime > newest { newest } else { *mtime },
                )
            },
        );

        let time_range = newest
            .duration_since(oldest)
            .map(|d| d.as_secs_f64())
            .unwrap_or(1.0)
            .max(1.0);

        let avg_value = if cached_map.is_empty() {
            self.weight_calc.base_weight()
        } else {
            let sum: f64 = cached_map.values().map(|w| w.value).sum();
            sum / cached_map.len() as f64
        };

        self.wallpapers = scanned_files
            .into_iter()
            .map(|(path, mtime)| {
                if let Some(cached_wallpaper) = cached_map.get(&path) {
                    cached_wallpaper.clone()
                } else {
                    let file_age = newest
                        .duration_since(mtime)
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0);
                    let age_ratio = file_age / time_range;

                    let time_based_weight = self.weight_calc.calculate_initial_weight(age_ratio);
                    let initial_value = (avg_value + time_based_weight) / 2.0;

                    Wallpaper {
                        path,
                        value: initial_value,
                        skip_streak: 0,
                        last_played: None,
                    }
                }
            })
            .collect();

        self.save();
    }

    /// 选择下一张壁纸
    pub fn pick_next(&mut self) -> Option<Wallpaper> {
        if self.wallpapers.is_empty() {
            return None;
        }

        let perturbation_ratio = self.config.weight.perturbation_ratio;
        let idx = WallpaperSelector::select(&mut self.wallpapers, 5.0, perturbation_ratio)?;
        let selected = self.wallpapers[idx].clone();

        Some(selected)
    }

    /// 设置壁纸并更新权重
    pub fn set_wallpaper(&mut self, wallpaper: &Wallpaper) -> Result<(), String> {
        self.engine.set_wallpaper(&wallpaper.path)?;

        // 找到选中壁纸的索引
        let selected_idx = self
            .wallpapers
            .iter()
            .position(|w| w.path == wallpaper.path)
            .ok_or("无法找到选中的壁纸")?;

        self.update_weights(selected_idx);

        Ok(())
    }

    /// 切换到下一张壁纸（pick_next + set_wallpaper）
    pub fn next(&mut self) -> Result<(), String> {
        let wallpaper = self.pick_next().ok_or("没有可用的壁纸")?;
        println!("切换到: {}", wallpaper.path.display());
        self.set_wallpaper(&wallpaper)
    }

    /// 更新所有壁纸的权重（零和博弈）
    fn update_weights(&mut self, selected_index: usize) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // 使用零和博弈算法更新权重
        self.weight_calc
            .update_weights_zero_sum(&mut self.wallpapers, selected_index);

        // 更新 last_played 时间
        if let Some(wall) = self.wallpapers.get_mut(selected_index) {
            wall.last_played = Some(now);
        }

        self.save();
    }

    /// 热重载：重新扫描目录并合并权重
    pub fn reset(&mut self) {
        let mode_str = match self.mode {
            WallpaperMode::Video => "动态壁纸",
            WallpaperMode::Image => "静态壁纸",
        };
        println!("重新扫描{}目录...", mode_str);
        self.load_and_scan();
        println!("发现 {} 个壁纸文件", self.wallpapers.len());
    }

    /// 获取状态信息
    pub fn status(&self) -> String {
        let stats = WallpaperSelector::get_stats(&self.wallpapers);
        let mode_str = match self.mode {
            WallpaperMode::Video => "动态壁纸 (Video)",
            WallpaperMode::Image => "静态壁纸 (Image)",
        };
        let interval = self.config.interval(self.mode);
        format!(
            "=== LianWall 状态 ===\n模式: {}\n引擎: {}\n切换间隔: {}秒\n\n{}\n\n--- 壁纸列表 ---",
            mode_str,
            self.engine.name(),
            interval,
            stats
        )
    }

    /// 获取详细壁纸列表
    pub fn list_wallpapers(&self) -> String {
        let mut output = String::new();
        let mut sorted = self.wallpapers.clone();
        sorted.sort_by(|a, b| b.value.partial_cmp(&a.value).unwrap());

        for (i, w) in sorted.iter().enumerate() {
            let filename = w
                .path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            output.push_str(&format!(
                "{:2}. [{:6.2}] (跳过:{}) {}\n",
                i + 1,
                w.value,
                w.skip_streak,
                filename
            ));
        }
        output
    }

    /// 保存壁纸数据到缓存文件
    fn save(&self) {
        let cache_path = self.config.cache_path(self.mode);

        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent).ok();
        }

        let content = serde_json::to_string_pretty(&self.wallpapers).expect("序列化失败");
        fs::write(&cache_path, content).expect("无法写入缓存文件");
    }
}
