use std::path::PathBuf;

use crate::core::algorithm::WeightUpdateConfig;
use crate::core::config::{read, Config, ConfigReadInput};
use crate::core::engine::{set, stop, EngineSetInput, EngineStopInput, EngineType};
use crate::core::manager::error::ManagerError;
use crate::core::manager::mode_manager::ModeManager;
use crate::core::manager::r#struct::{
    ManagerNextOutput, ManagerReloadOutput, ManagerStatusOutput, ModeStats,
};
use crate::core::runtime::{
    scheduler_run, RunMode, RuntimeState, SchedulerConfig, SchedulerEvent, SchedulerRunInput,
};
use crate::core::wallpaper::WallpaperScanInput;

/// 核心管理器
pub struct CoreManager {
    config: Config,
    video_manager: Option<ModeManager>,
    image_manager: Option<ModeManager>,
    state: RuntimeState,
}

impl CoreManager {
    /// 创建 Manager（只加载配置，不扫描壁纸）
    pub fn new() -> Result<Self, ManagerError> {
        let config = read(ConfigReadInput {})?;

        Ok(Self {
            config,
            video_manager: None,
            image_manager: None,
            state: RuntimeState::new(),
        })
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

        // 1. 初始化 Video ModeManager（会自动 reload）
        self.ensure_mode_manager(RunMode::Video)?;

        // 2. 立即播放第一张壁纸
        self.next(RunMode::Video)?;

        // 3. 构建调度器配置
        let scheduler_config = SchedulerConfig {
            video_interval: self.config.video_engine.interval,
            image_interval: self.config.image_engine.interval,
            vram_enabled: self.config.vram.enabled,
            vram_check_interval: self.config.vram.check_interval,
            vram_threshold: self.config.vram.threshold_percent as u32,
            vram_recovery: self.config.vram.recovery_percent as u32,
        };

        // 4. 创建事件通道
        let (tx, rx) = mpsc::channel::<SchedulerEvent>();

        // 5. 启动调度器线程
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

        // 6. 主线程处理事件
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
                SchedulerEvent::Shutdown => {
                    break;
                }
            }
        }

        Ok(())
    }

    /// 切换下一张壁纸
    pub fn next(&mut self, mode: RunMode) -> Result<ManagerNextOutput, ManagerError> {
        // 1. 确保对应模式的 ModeManager 已初始化
        self.ensure_mode_manager(mode.clone())?;

        let mode_mgr = self.get_mode_manager_mut(mode.clone())?;

        // 2. 选择壁纸
        let (selected_index, selected_path) = mode_mgr.select(
            self.config.weight.tolerance,
            self.config.weight.perturbation_ratio,
        )?;

        // 3. 设置壁纸
        let engine_args = match mode {
            RunMode::Video => self.config.video_engine.mpv_args.clone(),
            RunMode::Image => self.config.image_engine.swww_args.clone(),
        };

        set(EngineSetInput {
            engine_type: mode_mgr.engine_type,
            wallpaper_path: selected_path.clone(),
            extra_args: engine_args,
        })?;

        // 4. 更新权重
        let weight_config = WeightUpdateConfig {
            select_penalty: self.config.weight.select_penalty,
            normalization_threshold: self.config.weight.normalization_threshold,
            normalization_target: self.config.weight.normalization_target,
            shuffle_period: self.config.weight.shuffle_period,
            shuffle_intensity: self.config.weight.shuffle_intensity,
            base_weight: self.config.weight.base,
        };

        let selection_count = self.state.increment_selection_count();

        let (normalized, shuffled) =
            mode_mgr.update_and_save(selected_index, weight_config, selection_count)?;

        // 5. 更新状态
        self.state.current_wallpaper = Some(selected_path.clone());
        self.state.current_mode = mode.clone();

        Ok(ManagerNextOutput {
            selected_path,
            mode,
            normalized,
            shuffled,
        })
    }

    /// 重新扫描壁纸目录（热重载）
    pub fn reload(&mut self, mode: RunMode) -> Result<ManagerReloadOutput, ManagerError> {
        self.ensure_mode_manager(mode.clone())?;

        let mode_mgr = self.get_mode_manager_mut(mode)?;
        mode_mgr.reload(self.config.weight.base)
    }

    /// 切换到图片模式（VRAM 降级时调用）
    pub fn switch_to_image(&mut self) -> Result<(), ManagerError> {
        // 1. 停止 Video 引擎
        stop(EngineStopInput {
            engine_type: EngineType::Mpvpaper,
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
            engine_type: EngineType::Mpvpaper,
        })?;
        stop(EngineStopInput {
            engine_type: EngineType::Swww,
        })?;

        self.state.is_running = false;

        Ok(())
    }

    /// 获取当前状态
    pub fn get_status(&self) -> ManagerStatusOutput {
        let video_stats = self.video_manager.as_ref().map(|mgr| ModeStats {
            total_count: mgr.all_records.len(),
            active_count: mgr.active_records.len(),
            locked_count: mgr.all_records.len() - mgr.active_records.len(),
            algorithm_stats: mgr.get_stats(),
        });

        let image_stats = self.image_manager.as_ref().map(|mgr| ModeStats {
            total_count: mgr.all_records.len(),
            active_count: mgr.active_records.len(),
            locked_count: mgr.all_records.len() - mgr.active_records.len(),
            algorithm_stats: mgr.get_stats(),
        });

        ManagerStatusOutput {
            current_mode: self.state.current_mode.clone(),
            current_wallpaper: self.state.current_wallpaper.clone(),
            is_running: self.state.is_running,
            selection_count: self.state.selection_count,
            video_stats,
            image_stats,
        }
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
                    EngineType::Mpvpaper,
                    self.config.weight.base,
                )?);
            }
            RunMode::Image if self.image_manager.is_none() => {
                let cache_path = self.get_cache_path(RunMode::Image);
                let scan_config = self.get_scan_config(RunMode::Image);

                self.image_manager = Some(ModeManager::new(
                    cache_path,
                    scan_config,
                    EngineType::Swww,
                    self.config.weight.base,
                )?);
            }
            _ => {}
        }

        Ok(())
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
