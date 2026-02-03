use crate::core::runtime::error::RuntimeError;
use crate::core::runtime::monitor;
use crate::core::runtime::state::{RunMode, RuntimeState};
use crate::core::runtime::r#struct::{ModeAction, MonitorCheckInput, SchedulerConfig, SchedulerEvent};
use std::sync::mpsc::Sender;
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

/// 调度器运行输入
pub struct SchedulerRunInput {
    pub config: SchedulerConfig,
    pub state: RuntimeState,
    pub event_sender: Sender<SchedulerEvent>,
}

/// 运行调度器主循环
///
/// 注意：此函数会阻塞，直到接收到 Shutdown 事件
pub fn run(input: SchedulerRunInput) -> Result<(), RuntimeError> {
    let mut state = input.state;
    let config = input.config;
    let event_sender = input.event_sender;

    validate_config(&config)?;

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
            if event_sender
                .send(SchedulerEvent::SwitchWallpaper(state.current_mode.clone()))
                .is_err()
            {
                eprintln!("事件发送失败，调度器退出");
                break;
            }
        }

        // 检查 VRAM 监控定时器
        if let Some(ref mut timer) = vram_timer {
            if timer.check_and_reset() {
                let monitor_result = monitor::check(MonitorCheckInput {
                    current_mode: state.current_mode.clone(),
                    was_degraded: state.was_degraded,
                    threshold_percent: config.vram_threshold,
                    recovery_percent: config.vram_recovery,
                });

                match monitor_result.action {
                    ModeAction::DowngradeToImage => {
                        if event_sender.send(SchedulerEvent::DegradeToImage).is_err() {
                            eprintln!("降级事件发送失败，调度器退出");
                            break;
                        }
                        state.current_mode = RunMode::Image;
                        state.was_degraded = true;
                        wallpaper_timer = Timer::new(config.image_interval);
                    }
                    ModeAction::UpgradeToVideo => {
                        if event_sender.send(SchedulerEvent::UpgradeToVideo).is_err() {
                            eprintln!("恢复事件发送失败，调度器退出");
                            break;
                        }
                        state.current_mode = RunMode::Video;
                        state.was_degraded = false;
                        wallpaper_timer = Timer::new(config.video_interval);
                    }
                    ModeAction::Keep => {
                        // 无需操作
                    }
                }
            }
        }
    }

    Ok(())
}

fn validate_config(config: &SchedulerConfig) -> Result<(), RuntimeError> {
    if config.video_interval == 0 {
        return Err(RuntimeError::InvalidConfig {
            field: "video_interval".to_string(),
            value: config.video_interval.to_string(),
            reason: "必须大于 0".to_string(),
        });
    }
    if config.image_interval == 0 {
        return Err(RuntimeError::InvalidConfig {
            field: "image_interval".to_string(),
            value: config.image_interval.to_string(),
            reason: "必须大于 0".to_string(),
        });
    }

    if config.vram_enabled {
        if config.vram_check_interval == 0 {
            return Err(RuntimeError::InvalidConfig {
                field: "vram_check_interval".to_string(),
                value: config.vram_check_interval.to_string(),
                reason: "必须大于 0".to_string(),
            });
        }
        if config.vram_threshold >= 100.0 {
            return Err(RuntimeError::InvalidConfig {
                field: "vram_threshold".to_string(),
                value: config.vram_threshold.to_string(),
                reason: "必须小于 100".to_string(),
            });
        }
        if config.vram_recovery > 100.0 {
            return Err(RuntimeError::InvalidConfig {
                field: "vram_recovery".to_string(),
                value: config.vram_recovery.to_string(),
                reason: "必须小于等于 100".to_string(),
            });
        }
        if config.vram_recovery <= config.vram_threshold {
            return Err(RuntimeError::InvalidConfig {
                field: "vram_recovery".to_string(),
                value: config.vram_recovery.to_string(),
                reason: "必须大于 vram_threshold".to_string(),
            });
        }
    }

    Ok(())
}