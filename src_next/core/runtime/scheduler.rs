use crate::core::runtime::error::RuntimeError;
use crate::core::runtime::monitor;
use crate::core::runtime::state::{RunMode, RuntimeState};
use crate::core::runtime::r#struct::{ModeAction, MonitorCheckInput, SchedulerConfig};
use std::time::{Duration, Instant};

/// 定时器
#[derive(Debug)]
struct Timer {
    last_tick: Instant,
    interval: Duration,
}

impl Timer {
    fn new(interval_secs: u64) -> Self {
        Self {
            last_tick: Instant::now(),
            interval: Duration::from_secs(interval_secs),
        }
    }

    /// 检查是否到期，如果到期则重置
    fn check_and_reset(&mut self) -> bool {
        if self.last_tick.elapsed() >= self.interval {
            self.last_tick = Instant::now();
            true
        } else {
            false
        }
    }

    /// 剩余时间（秒）
    #[allow(dead_code)]
    fn remaining_secs(&self) -> u64 {
        self.interval
            .checked_sub(self.last_tick.elapsed())
            .unwrap_or(Duration::ZERO)
            .as_secs()
    }
}

/// 调度器回调函数
pub struct SchedulerCallbacks<F1, F2, F3>
where
    F1: Fn(RunMode) -> Result<(), String>,
    F2: Fn() -> Result<(), String>,
    F3: Fn() -> Result<(), String>,
{
    /// 切换壁纸回调
    pub on_switch: F1,
    /// 降级到图片模式回调
    pub on_degrade: F2,
    /// 恢复到视频模式回调
    pub on_upgrade: F3,
}

/// 调度器运行输入
pub struct SchedulerRunInput<F1, F2, F3>
where
    F1: Fn(RunMode) -> Result<(), String>,
    F2: Fn() -> Result<(), String>,
    F3: Fn() -> Result<(), String>,
{
    pub config: SchedulerConfig,
    pub state: RuntimeState,
    pub callbacks: SchedulerCallbacks<F1, F2, F3>,
}

/// 运行调度器主循环
///
/// 注意：此函数会阻塞，直到外部中断（如 Ctrl+C）
pub fn run<F1, F2, F3>(input: SchedulerRunInput<F1, F2, F3>) -> Result<(), RuntimeError>
where
    F1: Fn(RunMode) -> Result<(), String>,
    F2: Fn() -> Result<(), String>,
    F3: Fn() -> Result<(), String>,
{
    let mut state = input.state;
    let config = input.config;
    let callbacks = input.callbacks;

    // 初始化定时器
    let mut wallpaper_timer = Timer::new(match state.current_mode {
        RunMode::Video => config.video_interval,
        RunMode::Image => config.image_interval,
    });

    let mut vram_timer = if config.vram_enabled {
        Some(Timer::new(config.vram_check_interval))
    } else {
        None
    };

    state.is_running = true;

    loop {
        std::thread::sleep(Duration::from_secs(1));

        // 检查壁纸切换定时器
        if wallpaper_timer.check_and_reset() {
            if let Err(e) = (callbacks.on_switch)(state.current_mode.clone()) {
                eprintln!("壁纸切换回调失败: {}", e);
            }
        }

        // 检查 VRAM 监控定时器
        if let Some(ref mut timer) = vram_timer {
            if timer.check_and_reset() {
                let monitor_result = monitor::check(MonitorCheckInput {
                    current_mode: state.current_mode.clone(),
                    is_degraded: state.current_mode == RunMode::Image && state.is_running,
                    threshold_percent: config.vram_threshold,
                    recovery_percent: config.vram_recovery,
                });

                match monitor_result.action {
                    ModeAction::DowngradeToImage => {
                        if let Err(e) = (callbacks.on_degrade)() {
                            eprintln!("降级回调失败: {}", e);
                        } else {
                            state.current_mode = RunMode::Image;
                            // 切换到图片模式后，更新定时器间隔
                            wallpaper_timer = Timer::new(config.image_interval);
                        }
                    }
                    ModeAction::UpgradeToVideo => {
                        if let Err(e) = (callbacks.on_upgrade)() {
                            eprintln!("恢复回调失败: {}", e);
                        } else {
                            state.current_mode = RunMode::Video;
                            // 切换到视频模式后，更新定时器间隔
                            wallpaper_timer = Timer::new(config.video_interval);
                        }
                    }
                    ModeAction::Keep => {
                        // 无需操作
                    }
                }
            }
        }
    }
}
