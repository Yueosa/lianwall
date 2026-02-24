//! HookManager — 事件驱动的 hook 执行系统
//!
//! 订阅 daemon 内部 EventBus，匹配用户配置的 hook 规则，
//! 在独立子进程中执行 shell 命令。
//!
//! # 设计要点
//! - 每个 hook 在独立 tokio::spawn 中执行，不阻塞事件循环
//! - 通过 `Arc<RwLock<Vec<HookEntry>>>` 支持热更新（ReloadHooks 命令）
//! - 全局并发限制（Semaphore），防止 hook 风暴

use std::sync::Arc;

use tokio::sync::{broadcast, RwLock, Semaphore};

use lianwall_core::config::WallMode;
use lianwall_core::hook::{self, HookEntry, HookEvent};

use crate::event::{Event, EventBus, SpaceUpdateReason};

/// Hook 管理器句柄（用于热更新）
#[derive(Clone)]
pub struct HookHandle {
    hooks: Arc<RwLock<Vec<HookEntry>>>,
}

impl HookHandle {
    /// 热更新 hook 配置
    ///
    /// 注意: `max_concurrent` 的变动需要重启 daemon 才生效，
    /// 这里只更新 hook 规则列表。
    pub async fn reload(&self) -> Result<usize, String> {
        let config = hook::reload_hooks()?;
        let count = config.hook.len();
        let enabled = config.hook.iter().filter(|h| h.enabled).count();
        *self.hooks.write().await = config.hook;
        tracing::info!("Hooks reloaded: {} total, {} enabled", count, enabled);
        Ok(enabled)
    }

    /// 获取当前 hook 列表（用于 CLI list）
    pub async fn list(&self) -> Vec<HookEntry> {
        self.hooks.read().await.clone()
    }
}

/// 启动 HookManager task
///
/// 返回 `HookHandle` 用于热更新和查询。
pub fn spawn(event_bus: &EventBus) -> HookHandle {
    let config = hook::load_or_create_hooks();
    let enabled = config.hook.iter().filter(|h| h.enabled).count();
    let max_concurrent = config.max_concurrent.max(1);
    tracing::info!(
        "Hook system initialized: {} hooks ({} enabled), max_concurrent={}",
        config.hook.len(),
        enabled,
        max_concurrent
    );

    let hooks = Arc::new(RwLock::new(config.hook));
    let handle = HookHandle {
        hooks: Arc::clone(&hooks),
    };

    let mut rx = event_bus.subscribe();
    let semaphore = Arc::new(Semaphore::new(max_concurrent));

    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    // 将内部事件映射到 hook 事件 + 环境变量
                    if let Some((hook_event, env, mode_hint)) = map_event(&event) {
                        let hooks = hooks.read().await;
                        for entry in hooks.iter() {
                            if !entry.enabled {
                                continue;
                            }
                            if entry.on != hook_event {
                                continue;
                            }
                            // 模式过滤
                            if let Some(ref filter_mode) = entry.mode {
                                if let Some(ref actual_mode) = mode_hint {
                                    if filter_mode != actual_mode {
                                        continue;
                                    }
                                }
                            }
                            // trigger 过滤（仅 wallpaper_changed）
                            if let Some(ref triggers) = entry.trigger {
                                if let Some(actual_trigger) = env
                                    .iter()
                                    .find(|(k, _)| k == "LIANWALL_TRIGGER")
                                    .map(|(_, v)| v.as_str())
                                {
                                    if !triggers.iter().any(|t| t == actual_trigger) {
                                        continue;
                                    }
                                }
                            }

                            let name = entry.display_name();
                            let command = entry.command.clone();
                            let timeout = entry.timeout;
                            let env = env.clone();
                            let sem = Arc::clone(&semaphore);

                            tokio::spawn(async move {
                                // 获取并发许可
                                let _permit = match sem.acquire().await {
                                    Ok(p) => p,
                                    Err(_) => return,
                                };
                                tracing::debug!("Running hook '{}'", name);
                                hook::run_hook(&name, &command, &env, timeout).await;
                            });
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Hook manager lagged {} events", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("Hook manager: event bus closed, exiting");
                    break;
                }
            }
        }
    });

    handle
}

/// 将内部事件映射到 (HookEvent, 环境变量, 模式提示)
///
/// 返回 None 表示该事件不触发任何 hook。
fn map_event(event: &Event) -> Option<(HookEvent, Vec<(String, String)>, Option<String>)> {
    let mut env = vec![];

    match event {
        Event::WallpaperChanged { path, mode, trigger } => {
            let mode_str = match mode {
                WallMode::Video => "video",
                WallMode::Image => "image",
            };
            let trigger_str = serde_json::to_value(trigger)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| format!("{:?}", trigger).to_lowercase());
            let filename = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();

            env.push(("LIANWALL_EVENT".into(), "wallpaper_changed".into()));
            env.push(("LIANWALL_PATH".into(), path.display().to_string()));
            env.push(("LIANWALL_FILENAME".into(), filename));
            env.push(("LIANWALL_MODE".into(), mode_str.to_string()));
            env.push(("LIANWALL_TRIGGER".into(), trigger_str));

            Some((HookEvent::WallpaperChanged, env, Some(mode_str.to_string())))
        }

        Event::ModeChanged { from, to } => {
            let from_str = match from {
                WallMode::Video => "video",
                WallMode::Image => "image",
            };
            let to_str = match to {
                WallMode::Video => "video",
                WallMode::Image => "image",
            };

            env.push(("LIANWALL_EVENT".into(), "mode_changed".into()));
            env.push(("LIANWALL_MODE_FROM".into(), from_str.to_string()));
            env.push(("LIANWALL_MODE_TO".into(), to_str.to_string()));

            Some((HookEvent::ModeChanged, env, Some(to_str.to_string())))
        }

        Event::SpaceUpdated {
            reason,
            mode,
            total,
            available,
            ..
        } => {
            let mode_str = match mode {
                WallMode::Video => "video",
                WallMode::Image => "image",
            };
            let reason_str = match reason {
                SpaceUpdateReason::InitialScan | SpaceUpdateReason::Rescan => "rescanned",
                SpaceUpdateReason::FileChange => "config_changed",
                SpaceUpdateReason::LockChange => "lock_changed",
            };

            env.push(("LIANWALL_EVENT".into(), "space_updated".into()));
            env.push(("LIANWALL_SPACE_MODE".into(), mode_str.to_string()));
            env.push(("LIANWALL_SPACE_REASON".into(), reason_str.into()));
            env.push(("LIANWALL_TOTAL".into(), total.to_string()));
            env.push(("LIANWALL_AVAILABLE".into(), available.to_string()));

            Some((HookEvent::SpaceUpdated, env, Some(mode_str.to_string())))
        }

        Event::ConfigChanged { key, .. } => {
            env.push(("LIANWALL_EVENT".into(), "config_changed".into()));
            env.push(("LIANWALL_CONFIG_KEY".into(), key.clone()));

            Some((HookEvent::ConfigChanged, env, None))
        }

        Event::GpuStateUpdated { action, vram_info } => {
            let action_str = match action {
                lianwall_core::gpu::VramAction::Downgrade => "downgrade",
                lianwall_core::gpu::VramAction::Upgrade => "upgrade",
                lianwall_core::gpu::VramAction::Keep => "keep",
            };
            let (used_mb, free_pct) = vram_info
                .as_ref()
                .map(|v| (v.used_mb, v.free_percent))
                .unwrap_or((0, 100.0));

            env.push(("LIANWALL_EVENT".into(), "vram_changed".into()));
            env.push(("LIANWALL_VRAM_ACTION".into(), action_str.to_string()));
            env.push(("LIANWALL_VRAM_USED_MB".into(), used_mb.to_string()));
            env.push(("LIANWALL_VRAM_FREE_PCT".into(), format!("{:.1}", free_pct)));

            Some((HookEvent::VramChanged, env, None))
        }

        Event::TimePointReached { time, next_time } => {
            env.push(("LIANWALL_EVENT".into(), "time_point_reached".into()));
            env.push(("LIANWALL_TIME".into(), time.clone()));
            env.push((
                "LIANWALL_NEXT_TIME".into(),
                next_time.clone().unwrap_or_default(),
            ));

            Some((HookEvent::TimePointReached, env, None))
        }

        Event::Error { message } => {
            env.push(("LIANWALL_EVENT".into(), "error".into()));
            env.push(("LIANWALL_ERROR_MSG".into(), message.clone()));

            Some((HookEvent::Error, env, None))
        }

        Event::ShuttingDown => {
            env.push(("LIANWALL_EVENT".into(), "daemon_shutdown".into()));

            Some((HookEvent::DaemonShutdown, env, None))
        }

        // 内部事件，不触发 hook
        Event::SchedulerTick
        | Event::ScanProgress { .. }
        | Event::EngineStateChanged { .. } => None,
    }
}
