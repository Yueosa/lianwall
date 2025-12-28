use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use walkdir::WalkDir;

use crate::algorithm::{WeightCalculator, WallpaperSelector};
use crate::config::{Config, WallpaperMode};
use crate::paperengine::{create_engine, supported_extensions, PaperEngine};

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

        // 获取引擎支持的文件扩展名
        let extensions = supported_extensions(engine_type);

        // 读取现有缓存
        let cached: Vec<Wallpaper> = if cache_path.exists() {
            let content = fs::read_to_string(&cache_path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };

        // 创建路径到壁纸的映射
        let cached_map: std::collections::HashMap<PathBuf, Wallpaper> = cached
            .into_iter()
            .map(|w| (w.path.clone(), w))
            .collect();

        // 扫描目录中所有支持的文件
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
            println!("警告: 在 {} 中未找到支持的{}文件 ({})", 
                scan_dir.display(),
                mode_str,
                extensions.join(", "));
            return;
        }

        // 计算文件年龄比例（用于初始权重）
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

        // 计算现有壁纸的平均权重（用于新文件）
        let avg_value = if cached_map.is_empty() {
            self.weight_calc.base_weight()
        } else {
            let sum: f64 = cached_map.values().map(|w| w.value).sum();
            sum / cached_map.len() as f64
        };

        // 合并壁纸列表
        self.wallpapers = scanned_files
            .into_iter()
            .map(|(path, mtime)| {
                if let Some(cached_wallpaper) = cached_map.get(&path) {
                    // 保留旧权重
                    cached_wallpaper.clone()
                } else {
                    // 新文件：根据修改时间计算初始权重，或使用平均值
                    let file_age = newest
                        .duration_since(mtime)
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0);
                    let age_ratio = file_age / time_range;

                    // 新发现文件使用平均值和时间戳混合
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

        // 使用二分选择算法，tolerance 设为 5.0
        let idx = WallpaperSelector::select(&mut self.wallpapers, 5.0)?;
        Some(self.wallpapers[idx].clone())
    }

    /// 设置壁纸并更新权重
    pub fn set_wallpaper(&mut self, wallpaper: &Wallpaper) -> Result<(), String> {
        // 调用引擎设置壁纸
        self.engine.set_wallpaper(&wallpaper.path)?;

        // 更新权重
        self.update_weights(&wallpaper.path);

        Ok(())
    }

    /// 切换到下一张壁纸（pick_next + set_wallpaper）
    pub fn next(&mut self) -> Result<(), String> {
        let wallpaper = self.pick_next().ok_or("没有可用的壁纸")?;
        println!("切换到: {}", wallpaper.path.display());
        self.set_wallpaper(&wallpaper)
    }

    /// 更新所有壁纸的权重
    fn update_weights(&mut self, selected_path: &PathBuf) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        for wall in &mut self.wallpapers {
            if &wall.path == selected_path {
                // 被选中：权重惩罚，重置跳过计数
                wall.value = self.weight_calc.apply_selection_penalty(wall.value);
                wall.skip_streak = 0;
                wall.last_played = Some(now);
            } else {
                // 未被选中：权重奖励，增加跳过计数
                wall.value = self.weight_calc.apply_skip_reward(wall.value, wall.skip_streak);
                wall.skip_streak += 1;
            }
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
            let filename = w.path.file_name()
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

        // 确保目录存在
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent).ok();
        }

        let content = serde_json::to_string_pretty(&self.wallpapers)
            .expect("序列化失败");
        fs::write(&cache_path, content).expect("无法写入缓存文件");
    }

    /// 获取当前模式
    pub fn get_mode(&self) -> WallpaperMode {
        self.mode
    }
}
