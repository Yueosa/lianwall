use std::path::PathBuf;

use crate::core::algorithm::WeightUpdateConfig;
use crate::core::config::{
    config_path, create, delete, read, Config, ConfigCreateInput, ConfigDeleteInput,
    ConfigReadInput,
};
use crate::core::engine::{
    detect as engine_detect, is_running as engine_is_running, set, stop, EngineDetectInput,
    EngineSetInput, EngineStopInput, EngineType,
};
use crate::core::gpu::{detect as gpu_detect, VramDetectInput};
use crate::core::manager::error::ManagerError;
use crate::core::manager::mode_manager::ModeManager;
use crate::core::manager::r#struct::{
    DiagnoseAllOutput, DiagnoseDirsOutput, DiagnoseEnginesOutput, DiagnoseGpuOutput, LockOutput,
    ManagerNextOutput, ManagerReloadOutput, ManagerStatusOutput, ModeStats, WallpaperListOutput,
};
use crate::core::runtime::{
    scheduler_run, RunMode, RuntimeState, SchedulerConfig, SchedulerEvent, SchedulerRunInput,
};
use crate::core::wallpaper::{scan, WallpaperScanInput};

/// 核心管理器
pub struct CoreManager {
    config: Config,
    video_manager: Option<ModeManager>,
    image_manager: Option<ModeManager>,
    state: RuntimeState,
    /// 系统种子（用于选择算法）
    system_seed: u64,
    /// 种子上次重置的时间戳
    seed_reset_time: u64,
}

/// 获取当前时间戳（秒）
fn get_current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// 生成随机种子
fn generate_seed() -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    get_current_timestamp().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    hasher.finish()
}

impl CoreManager {
    /// 创建 Manager（加载配置和持久化状态）
    pub fn new() -> Result<Self, ManagerError> {
        let output = create(ConfigCreateInput { path: None })?;

        // 加载持久化状态
        let state = RuntimeState::load();

        // 初始化种子
        let now = get_current_timestamp();

        Ok(Self {
            config: output.config,
            video_manager: None,
            image_manager: None,
            state,
            system_seed: generate_seed(),
            seed_reset_time: now,
        })
    }

    /// 获取系统种子（根据配置周期性重置）
    fn get_system_seed(&mut self) -> u64 {
        let reset_hours = self.config.weight.seed_reset_hours;
        
        if reset_hours == 0 {
            // 每次选择都重置
            self.system_seed = generate_seed();
        } else {
            // 检查是否需要重置
            let now = get_current_timestamp();
            let elapsed_hours = (now - self.seed_reset_time) / 3600;
            
            if elapsed_hours >= reset_hours as u64 {
                self.system_seed = generate_seed();
                self.seed_reset_time = now;
            }
        }
        
        self.system_seed
    }

    /// 启动守护进程（阻塞式，调用 runtime::scheduler_run）
    ///
    /// 流程：
    /// 1. reload Video 模式（初始化 + 文件检测）
    /// 2. 立即播放第一张壁纸
    /// 3. 启动调度器线程
    /// 4. 主线程处理调度器事件
    pub fn start(&mut self) -> Result<(), ManagerError> {
        use std::sync::mpsc;
        use std::thread;

        // 0. 启动前刷新配置（支持运行时更新）
        self.refresh_config()?;

        // 1. 根据配置模式设置初始运行模式
        let initial_mode = Self::parse_mode(&self.config.paths.mode);
        self.state.current_mode = initial_mode.clone();

        // 2. 初始化对应模式的 ModeManager（会自动 reload）
        self.ensure_mode_manager(initial_mode.clone())?;

        // 3. 立即播放第一张壁纸
        self.next(initial_mode)?;

        // 4. 构建调度器配置
        let scheduler_config = SchedulerConfig {
            video_interval: self.config.video_engine.interval,
            image_interval: self.config.image_engine.interval,
            vram_enabled: self.config.vram.enabled,
            vram_check_interval: self.config.vram.check_interval,
            vram_threshold: self.config.vram.threshold_percent,
            vram_recovery: self.config.vram.recovery_percent,
        };

        // 5. 创建事件通道
        let (tx, rx) = mpsc::channel::<SchedulerEvent>();

        // 6. 启动调度器线程
        let state = self.state.clone();
        thread::spawn(move || {
            if let Err(e) = scheduler_run(SchedulerRunInput {
                config: scheduler_config,
                state,
                event_sender: tx,
            }) {
                eprintln!("调度器错误: {:?}", e);
            }
        });

        // 7. 主线程处理事件
        for event in rx {
            match event {
                SchedulerEvent::SwitchWallpaper(mode) => {
                    if let Err(e) = self.next(mode) {
                        eprintln!("切换壁纸失败: {:?}", e);
                    }
                }
                SchedulerEvent::DegradeToImage => {
                    if let Err(e) = self.switch_to_image() {
                        eprintln!("降级到图片模式失败: {:?}", e);
                    }
                }
                SchedulerEvent::UpgradeToVideo => {
                    if let Err(e) = self.switch_to_video() {
                        eprintln!("恢复到视频模式失败: {:?}", e);
                    }
                }
                SchedulerEvent::RefreshActiveList(mode) => {
                    // 静默刷新活跃列表（用于时间段目录更新）
                    let _ = self.reload(mode);
                }
                SchedulerEvent::Shutdown => {
                    break;
                }
            }
        }

        Ok(())
    }

    /// 切换下一张壁纸
    pub fn next(&mut self, mode: RunMode) -> Result<ManagerNextOutput, ManagerError> {
        // 0. 刷新配置（支持运行时更新）
        self.refresh_config()?;

        // 1. 确保对应模式的 ModeManager 已初始化
        self.ensure_mode_manager(mode.clone())?;

        // 2. 先复制需要的配置值（避免借用冲突）
        let top_n_percent = self.config.weight.top_n_percent;
        let hash_mix_bytes = self.config.weight.hash_mix_bytes;
        let system_seed = self.get_system_seed();
        let engine_args = match mode {
            RunMode::Video => self.config.video_engine.mpv_args.clone(),
            RunMode::Image => self.config.image_engine.swww_args.clone(),
        };
        let weight_config = WeightUpdateConfig {
            weight_min: self.config.weight.weight_min,
            weight_max: self.config.weight.weight_max,
            select_penalty: self.config.weight.select_penalty,
            normalization_threshold: self.config.weight.normalization_threshold,
            normalization_target: self.config.weight.normalization_target,
            shuffle_period: self.config.weight.shuffle_period,
            shuffle_intensity: self.config.weight.shuffle_intensity,
        };

        // 3. 获取 mode_mgr 并选择壁纸
        let mode_mgr = self.get_mode_manager_mut(mode.clone())?;
        let (selected_index, selected_path) = mode_mgr.select(top_n_percent, hash_mix_bytes, system_seed)?;
        let engine_type = mode_mgr.engine_type;

        // 4. 设置壁纸
        set(EngineSetInput {
            engine_type,
            wallpaper_path: selected_path.clone(),
            extra_args: engine_args,
        })?;

        // 5. 更新选择计数
        let selection_count = self.state.increment_selection_count();

        // 6. 更新权重（重新获取 mode_mgr）
        let mode_mgr = self.get_mode_manager_mut(mode.clone())?;
        let (normalized, shuffled) =
            mode_mgr.update_and_save(selected_index, weight_config, selection_count)?;

        // 7. 更新状态
        self.state.current_wallpaper = Some(selected_path.clone());
        self.state.current_mode = mode.clone();

        // 8. 持久化状态
        self.state.save();

        Ok(ManagerNextOutput {
            selected_path,
            mode,
            normalized,
            shuffled,
        })
    }

    /// 重新扫描壁纸目录（热重载）
    pub fn reload(&mut self, mode: RunMode) -> Result<ManagerReloadOutput, ManagerError> {
        // 0. 刷新配置（支持运行时更新）
        self.refresh_config()?;

        self.ensure_mode_manager(mode.clone())?;

        // 先提取需要的值，避免借用冲突
        let weight_min = self.config.weight.weight_min;
        let weight_max = self.config.weight.weight_max;
        let scan_config = self.get_scan_config(mode.clone());
        let cache_path = self.get_cache_path(mode.clone());

        let mode_mgr = self.get_mode_manager_mut(mode)?;
        mode_mgr.scan_config = scan_config;
        mode_mgr.cache_path = cache_path;
        mode_mgr.reload(weight_min, weight_max)
    }

    /// 切换到图片模式（VRAM 降级时调用）
    pub fn switch_to_image(&mut self) -> Result<(), ManagerError> {
        // 0. 刷新配置（支持运行时更新）
        self.refresh_config()?;

        // 1. 停止 Video 引擎
        stop(EngineStopInput {
            engine_type: EngineType::MpvPaper,
        })?;

        // 2. 懒加载初始化 Image ModeManager
        self.ensure_mode_manager(RunMode::Image)?;

        // 3. 切换状态
        self.state.current_mode = RunMode::Image;

        // 4. 立即播放一张图片
        self.next(RunMode::Image)?;

        Ok(())
    }

    /// 切换到视频模式（VRAM 恢复时调用）
    pub fn switch_to_video(&mut self) -> Result<(), ManagerError> {
        // 0. 刷新配置（支持运行时更新）
        self.refresh_config()?;

        // 1. 停止 Image 引擎
        stop(EngineStopInput {
            engine_type: EngineType::Swww,
        })?;

        // 2. 确保 Video ModeManager 已初始化
        self.ensure_mode_manager(RunMode::Video)?;

        // 3. 切换状态
        self.state.current_mode = RunMode::Video;

        // 4. 立即播放一张视频
        self.next(RunMode::Video)?;

        Ok(())
    }

    /// 停止所有引擎
    pub fn stop(&mut self) -> Result<(), ManagerError> {
        stop(EngineStopInput {
            engine_type: EngineType::MpvPaper,
        })?;
        stop(EngineStopInput {
            engine_type: EngineType::Swww,
        })?;

        self.state.is_running = false;
        self.state.save();

        Ok(())
    }

    /// 获取当前状态
    pub fn get_status(&self) -> ManagerStatusOutput {
        let video_stats = self.video_manager.as_ref().map(|mgr| ModeStats {
            total_count: mgr.all_records.len(),
            active_count: mgr.active_records.len(),
            locked_count: mgr.locked_count(),
            algorithm_stats: mgr.get_stats(),
        });

        let image_stats = self.image_manager.as_ref().map(|mgr| ModeStats {
            total_count: mgr.all_records.len(),
            active_count: mgr.active_records.len(),
            locked_count: mgr.locked_count(),
            algorithm_stats: mgr.get_stats(),
        });

        // 通过进程检测来判断是否在运行
        let engine_type = match &self.state.current_mode {
            crate::core::runtime::RunMode::Video => EngineType::MpvPaper,
            crate::core::runtime::RunMode::Image => EngineType::Swww,
        };
        let is_running = engine_is_running(engine_type);

        ManagerStatusOutput {
            current_mode: self.state.current_mode.clone(),
            current_wallpaper: self.state.current_wallpaper.clone(),
            is_running,
            selection_count: self.state.selection_count,
            video_stats,
            image_stats,
        }
    }

    // --- 自检接口 ---

    /// 检测 GPU 类型和 VRAM 可用性
    pub fn check_gpu(&self) -> DiagnoseGpuOutput {
        let result = gpu_detect(VramDetectInput {});
        DiagnoseGpuOutput {
            gpu_type: result.gpu_type,
            vram_available: result.available,
            reason: result.reason,
        }
    }

    /// 检测引擎安装情况
    pub fn check_engines(&self) -> DiagnoseEnginesOutput {
        let mpvpaper = engine_detect(EngineDetectInput {
            engine_type: EngineType::MpvPaper,
        })
        .map(|r| r.available)
        .unwrap_or(false);

        let swww = engine_detect(EngineDetectInput {
            engine_type: EngineType::Swww,
        })
        .map(|r| r.available)
        .unwrap_or(false);

        DiagnoseEnginesOutput {
            mpvpaper_installed: mpvpaper,
            swww_installed: swww,
        }
    }

    /// 检测壁纸目录
    pub fn check_dirs(&self) -> DiagnoseDirsOutput {
        let video_dir = &self.config.paths.video_dir;
        let image_dir = &self.config.paths.image_dir;

        let video_result = scan(WallpaperScanInput {
            base_dir: video_dir.clone(),
            extensions: vec!["mp4".to_string(), "mkv".to_string(), "webm".to_string()],
            use_time_ranges: false,
        });

        let image_result = scan(WallpaperScanInput {
            base_dir: image_dir.clone(),
            extensions: vec![
                "jpg".to_string(),
                "jpeg".to_string(),
                "png".to_string(),
                "gif".to_string(),
                "webp".to_string(),
            ],
            use_time_ranges: false,
        });

        DiagnoseDirsOutput {
            video_dir_exists: video_dir.exists(),
            video_count: video_result.map(|r| r.wallpapers.len()).unwrap_or(0),
            image_dir_exists: image_dir.exists(),
            image_count: image_result.map(|r| r.wallpapers.len()).unwrap_or(0),
        }
    }

    /// 完整自检
    pub fn check_all(&self) -> DiagnoseAllOutput {
        let cfg_path = config_path(None);
        let config_exists = cfg_path.exists();

        let gpu = self.check_gpu();
        let engines = self.check_engines();
        let dirs = self.check_dirs();

        let mut errors = Vec::new();

        // 检查配置文件
        if !config_exists {
            errors.push("配置文件不存在".to_string());
        }

        // 检查 GPU（仅当启用 VRAM 监控时）
        if self.config.vram.enabled && !gpu.vram_available {
            errors.push(format!(
                "VRAM 监控已启用但 GPU 不支持: {:?}",
                gpu.gpu_type
            ));
        }

        // 检查引擎安装
        if !engines.mpvpaper_installed {
            errors.push("mpvpaper 未安装（视频壁纸不可用）".to_string());
        }
        if !engines.swww_installed {
            errors.push("swww 未安装（图片壁纸不可用）".to_string());
        }

        // 检查目录
        let current_mode = Self::parse_mode(&self.config.paths.mode);
        match current_mode {
            RunMode::Video => {
                if !dirs.video_dir_exists {
                    errors.push(format!(
                        "视频目录不存在: {}",
                        self.config.paths.video_dir.display()
                    ));
                } else if dirs.video_count == 0 {
                    errors.push("视频目录为空".to_string());
                }
            }
            RunMode::Image => {
                if !dirs.image_dir_exists {
                    errors.push(format!(
                        "图片目录不存在: {}",
                        self.config.paths.image_dir.display()
                    ));
                } else if dirs.image_count == 0 {
                    errors.push("图片目录为空".to_string());
                }
            }
        }

        let all_passed = errors.is_empty();

        DiagnoseAllOutput {
            config_path: cfg_path,
            config_exists,
            gpu,
            engines,
            dirs,
            all_passed,
            errors,
        }
    }

    // --- 配置管理接口 ---

    /// 重置配置为默认值
    pub fn config_reset(&mut self) -> Result<PathBuf, ManagerError> {
        // 删除现有配置
        delete(ConfigDeleteInput { path: None })?;

        // 重新创建默认配置
        let output = create(ConfigCreateInput { path: None })?;
        self.config = output.config;

        Ok(output.path)
    }

    /// 获取当前配置
    pub fn config_get(&self) -> &Config {
        &self.config
    }

    // --- 壁纸管理接口 ---

    /// 列出指定模式的壁纸
    pub fn list(&mut self, mode: RunMode) -> Result<WallpaperListOutput, ManagerError> {
        self.ensure_mode_manager(mode.clone())?;
        let mode_mgr = self.get_mode_manager(mode)?;
        Ok(mode_mgr.list())
    }

    /// 锁定指定壁纸
    pub fn lock(&mut self, mode: RunMode, path: PathBuf) -> Result<LockOutput, ManagerError> {
        self.ensure_mode_manager(mode.clone())?;
        let mode_mgr = self.get_mode_manager_mut(mode)?;
        mode_mgr.lock(&path)?;
        Ok(LockOutput { path, locked: true })
    }

    /// 解锁指定壁纸
    pub fn unlock(&mut self, mode: RunMode, path: PathBuf) -> Result<LockOutput, ManagerError> {
        self.ensure_mode_manager(mode.clone())?;
        let weight_min = self.config.weight.weight_min;
        let weight_max = self.config.weight.weight_max;
        let mode_mgr = self.get_mode_manager_mut(mode)?;
        mode_mgr.unlock(&path, weight_min, weight_max)?;
        Ok(LockOutput {
            path,
            locked: false,
        })
    }

    /// 手动切换模式
    pub fn switch(&mut self, mode: RunMode) -> Result<(), ManagerError> {
        match mode {
            RunMode::Video => self.switch_to_video(),
            RunMode::Image => self.switch_to_image(),
        }
    }

    /// 获取统计信息
    pub fn stats(&mut self, mode: RunMode) -> Result<ModeStats, ManagerError> {
        self.ensure_mode_manager(mode.clone())?;
        let mode_mgr = self.get_mode_manager(mode)?;
        Ok(ModeStats {
            total_count: mode_mgr.all_records.len(),
            active_count: mode_mgr.active_records.len(),
            locked_count: mode_mgr.locked_count(),
            algorithm_stats: mode_mgr.get_stats(),
        })
    }

    // --- 内部辅助方法 ---

    /// 确保对应模式的 ModeManager 已初始化
    fn ensure_mode_manager(&mut self, mode: RunMode) -> Result<(), ManagerError> {
        match mode {
            RunMode::Video if self.video_manager.is_none() => {
                let cache_path = self.get_cache_path(RunMode::Video);
                let scan_config = self.get_scan_config(RunMode::Video);

                self.video_manager = Some(ModeManager::new(
                    cache_path,
                    scan_config,
                    EngineType::MpvPaper,
                    RunMode::Video,
                    self.config.weight.weight_min,
                    self.config.weight.weight_max,
                )?);
            }
            RunMode::Image if self.image_manager.is_none() => {
                let cache_path = self.get_cache_path(RunMode::Image);
                let scan_config = self.get_scan_config(RunMode::Image);

                self.image_manager = Some(ModeManager::new(
                    cache_path,
                    scan_config,
                    EngineType::Swww,
                    RunMode::Image,
                    self.config.weight.weight_min,
                    self.config.weight.weight_max,
                )?);
            }
            _ => {}
        }

        Ok(())
    }

    /// 重新读取配置文件（支持运行时更新）
    fn refresh_config(&mut self) -> Result<(), ManagerError> {
        let output = read(ConfigReadInput { path: None })?;
        self.config = output.config;
        Ok(())
    }

    /// 从配置字符串解析运行模式
    fn parse_mode(mode: &str) -> RunMode {
        match mode.to_lowercase().as_str() {
            "image" => RunMode::Image,
            "video" => RunMode::Video,
            _ => RunMode::Video,
        }
    }

    /// 获取 ModeManager（不可变引用）
    fn get_mode_manager(&self, mode: RunMode) -> Result<&ModeManager, ManagerError> {
        match mode {
            RunMode::Video => self
                .video_manager
                .as_ref()
                .ok_or(ManagerError::ModeNotInitialized { mode }),
            RunMode::Image => self
                .image_manager
                .as_ref()
                .ok_or(ManagerError::ModeNotInitialized { mode }),
        }
    }

    /// 获取 ModeManager（可变引用）
    fn get_mode_manager_mut(&mut self, mode: RunMode) -> Result<&mut ModeManager, ManagerError> {
        match mode {
            RunMode::Video => self
                .video_manager
                .as_mut()
                .ok_or(ManagerError::ModeNotInitialized { mode }),
            RunMode::Image => self
                .image_manager
                .as_mut()
                .ok_or(ManagerError::ModeNotInitialized { mode }),
        }
    }

    /// 获取缓存路径
    fn get_cache_path(&self, mode: RunMode) -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let cache_dir = home.join(".cache/lianwall");

        match mode {
            RunMode::Video => cache_dir.join("video_weights.json"),
            RunMode::Image => cache_dir.join("image_weights.json"),
        }
    }

    /// 获取扫描配置
    fn get_scan_config(&self, mode: RunMode) -> WallpaperScanInput {
        let (base_dir, extensions) = match mode {
            RunMode::Video => (
                self.config.paths.video_dir.clone(),
                vec!["mp4".to_string(), "mkv".to_string(), "webm".to_string()],
            ),
            RunMode::Image => (
                self.config.paths.image_dir.clone(),
                vec![
                    "jpg".to_string(),
                    "jpeg".to_string(),
                    "png".to_string(),
                    "gif".to_string(),
                    "webp".to_string(),
                ],
            ),
        };

        WallpaperScanInput {
            base_dir,
            extensions,
            use_time_ranges: true,
        }
    }
}
