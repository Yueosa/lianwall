//! 定时调度器

use std::time::{Duration, Instant};

use lianwall_core::algorithm::select_next;
use lianwall_core::config::WallMode;
use lianwall_core::engine;
use lianwall_core::gpu::{check, VramAction};

use crate::error::DaemonError;
use crate::handler::DaemonState;

/// 调度器
pub struct Scheduler {
    /// 上次壁纸切换时间
    last_wallpaper_tick: Instant,
    /// 上次 GPU 检查时间
    last_gpu_check: Instant,
    /// 是否跳过 GPU 检查（检测失败后设置）
    skip_gpu_check: bool,
}

impl Scheduler {
    /// 创建新的调度器
    pub fn new() -> Self {
        Self {
            last_wallpaper_tick: Instant::now(),
            last_gpu_check: Instant::now(),
            skip_gpu_check: false,
        }
    }

    /// 获取下一个需要处理的截止时间
    pub fn next_deadline(&self, state: &DaemonState) -> Instant {
        let wallpaper_interval = self.get_wallpaper_interval(state);
        let wallpaper_deadline = self.last_wallpaper_tick + wallpaper_interval;

        if state.config.vram.enabled && !self.skip_gpu_check {
            let gpu_interval = Duration::from_secs(state.config.vram.check_interval);
            let gpu_deadline = self.last_gpu_check + gpu_interval;
            std::cmp::min(wallpaper_deadline, gpu_deadline)
        } else {
            wallpaper_deadline
        }
    }

    /// 获取当前模式的壁纸切换间隔
    fn get_wallpaper_interval(&self, state: &DaemonState) -> Duration {
        let secs = match state.engine.mode {
            WallMode::Video => state.config.video_engine.interval,
            WallMode::Image => state.config.image_engine.interval,
        };
        Duration::from_secs(secs)
    }

    /// 执行定时任务
    pub fn tick(&mut self, state: &mut DaemonState) -> Result<(), DaemonError> {
        let now = Instant::now();

        // GPU 检查
        if self.should_check_gpu(now, state) {
            self.check_gpu(state);
            self.last_gpu_check = now;
        }

        // 壁纸切换
        if self.should_switch_wallpaper(now, state) {
            self.switch_wallpaper(state)?;
            self.last_wallpaper_tick = now;
        }

        Ok(())
    }

    /// 是否应该检查 GPU
    fn should_check_gpu(&self, now: Instant, state: &DaemonState) -> bool {
        if !state.config.vram.enabled || self.skip_gpu_check {
            return false;
        }
        let interval = Duration::from_secs(state.config.vram.check_interval);
        now.duration_since(self.last_gpu_check) >= interval
    }

    /// 是否应该切换壁纸
    fn should_switch_wallpaper(&self, now: Instant, state: &DaemonState) -> bool {
        let interval = self.get_wallpaper_interval(state);
        now.duration_since(self.last_wallpaper_tick) >= interval
    }

    /// 检查 GPU 并执行降级/升级
    fn check_gpu(&mut self, state: &mut DaemonState) {
        match check(&mut state.gpu, &state.config.vram) {
            Ok(VramAction::Downgrade) => {
                tracing::warn!("VRAM 不足，降级到 Image 模式");
                if let Err(e) =
                    engine::switch_mode(&mut state.engine, WallMode::Image, &state.config)
                {
                    tracing::error!("降级失败: {}", e);
                }
            }
            Ok(VramAction::Upgrade) => {
                tracing::info!("VRAM 恢复，升级到 Video 模式");
                if let Err(e) =
                    engine::switch_mode(&mut state.engine, WallMode::Video, &state.config)
                {
                    tracing::error!("升级失败: {}", e);
                }
            }
            Ok(VramAction::Keep) => {}
            Err(e) => {
                tracing::warn!("GPU 检测失败: {}, 后续将跳过检测", e);
                self.skip_gpu_check = true;
            }
        }
    }

    /// 切换到下一张壁纸
    fn switch_wallpaper(&self, state: &mut DaemonState) -> Result<(), DaemonError> {
        let space = state.current_space_mut();

        if let Some(output) = select_next(space) {
            let path = space.items[output.index].path.clone();

            engine::set_wallpaper(&mut state.engine, &path, &state.config)
                .map_err(DaemonError::Engine)?;

            // 保存状态
            let _ = state.save_weights();

            tracing::info!("定时切换: {}", path.display());
        } else {
            tracing::warn!("没有可用的壁纸");
        }

        Ok(())
    }

    /// 立即触发一次壁纸切换（首次启动时）
    pub fn trigger_immediate(&mut self, state: &mut DaemonState) {
        if state.engine.current.is_none() {
            tracing::info!("首次启动，立即切换壁纸");
            let _ = self.switch_wallpaper(state);
            self.last_wallpaper_tick = Instant::now();
        }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}
