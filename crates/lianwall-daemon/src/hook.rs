//! HookManager — 事件驱动的 hook 执行系统
//!
//! 订阅 daemon 内部 EventBus，匹配用户配置的 hook 规则，
//! 在独立子进程中执行 shell 命令。
//!
//! # 设计要点
//! - 每个 hook 在独立 tokio::spawn 中执行，不阻塞事件循环
//! - 通过 `Arc<RwLock<Vec<HookEntry>>>` 支持热更新（ReloadHooks 命令）
//! - 有界等待队列和 JoinSet，并发、关闭预算均明确受控

use crate::event::{Event, EventBus, SpaceUpdateReason};
use lianwall_core::config::WallMode;
use lianwall_core::hook::{self, HookConfig, HookEntry, HookEvent};
use std::collections::VecDeque;
use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio::sync::{RwLock, broadcast, mpsc, oneshot};
use tokio::task::JoinSet;

#[derive(Default)]
struct Metrics {
    running: AtomicUsize,
    queued: AtomicUsize,
    dropped: AtomicU64,
}

#[derive(Debug)]
pub struct HookStats {
    pub running: usize,
    pub queued: usize,
    pub dropped: u64,
}

#[derive(Clone)]
pub struct HookHandle {
    hooks: Arc<RwLock<Vec<HookEntry>>>,
    shutdown_tx: mpsc::Sender<oneshot::Sender<()>>,
    metrics: Arc<Metrics>,
}

impl HookHandle {
    /// Rule edits are hot-reloaded; concurrency/queue limits still require a restart.
    pub async fn reload(&self) -> Result<usize, String> {
        let config = hook::reload_hooks()?;
        let count = config.hook.iter().filter(|entry| entry.enabled).count();
        *self.hooks.write().await = config.hook;
        Ok(count)
    }

    pub async fn list(&self) -> Vec<HookEntry> {
        self.hooks.read().await.clone()
    }

    pub fn stats(&self) -> HookStats {
        HookStats {
            running: self.metrics.running.load(Ordering::Relaxed),
            queued: self.metrics.queued.load(Ordering::Relaxed),
            dropped: self.metrics.dropped.load(Ordering::Relaxed),
        }
    }

    /// Stop ordinary jobs, then finish cleanup hooks under one shared five-second budget.
    pub async fn shutdown(&self) {
        let (tx, rx) = oneshot::channel();
        if self.shutdown_tx.send(tx).await.is_ok() {
            let _ = rx.await;
        }
    }
}

struct Job {
    entry: HookEntry,
    env: Vec<(String, String)>,
}

fn start_job(running: &mut JoinSet<()>, job: Job) {
    running.spawn(async move {
        hook::run_hook(
            &job.entry.display_name(),
            &job.entry.command,
            &job.env,
            job.entry.timeout,
        )
        .await;
    });
}

fn matches(
    entry: &HookEntry,
    event: HookEvent,
    env: &[(String, String)],
    mode: Option<&str>,
) -> bool {
    entry.enabled
        && entry.on == event
        && entry
            .mode
            .as_deref()
            .is_none_or(|filter| Some(filter) == mode)
        && entry.trigger.as_ref().is_none_or(|triggers| {
            env.iter()
                .find(|(key, _)| key == "LIANWALL_TRIGGER")
                .is_some_and(|(_, trigger)| triggers.contains(trigger))
        })
}

pub fn spawn(event_bus: &EventBus) -> HookHandle {
    spawn_with_config(event_bus, hook::load_or_create_hooks())
}

fn spawn_with_config(event_bus: &EventBus, config: HookConfig) -> HookHandle {
    let max_concurrent = config.max_concurrent.max(1);
    let max_pending = config.max_pending;
    let hooks = Arc::new(RwLock::new(config.hook));
    let metrics = Arc::new(Metrics::default());
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<oneshot::Sender<()>>(1);
    let handle = HookHandle {
        hooks: hooks.clone(),
        shutdown_tx,
        metrics: metrics.clone(),
    };
    let mut receiver = event_bus.subscribe();
    tokio::spawn(async move {
        let mut running = JoinSet::new();
        let mut pending = VecDeque::new();
        loop {
            tokio::select! {
                biased;
                request = shutdown_rx.recv() => {
                    tracing::info!(running = running.len(), queued = pending.len(), "Stopping ordinary hooks before cleanup");
                    pending.clear();
                    running.abort_all();
                    while running.join_next().await.is_some() {}
                    if let Some(completed) = request {
                        let entries = hooks.read().await.clone();
                        run_shutdown_hooks(entries, max_concurrent, Duration::from_secs(5)).await;
                        metrics.running.store(0, Ordering::Relaxed);
                        metrics.queued.store(0, Ordering::Relaxed);
                        let _ = completed.send(());
                    }
                    break;
                }
                result = running.join_next(), if !running.is_empty() => {
                    if let Some(Err(error)) = result { tracing::warn!("Hook task failed: {}", error); }
                }
                result = receiver.recv() => match result {
                    // Cleanup is explicitly owned by shutdown(), never scheduled twice from the bus.
                    Ok(Event::ShuttingDown) => {}
                    Ok(event) => if let Some((event, env, mode)) = map_event(&event) {
                        for entry in hooks.read().await.iter() {
                            if !matches(entry, event, &env, mode.as_deref()) { continue; }
                            let job = Job { entry: entry.clone(), env: env.clone() };
                            if running.len() < max_concurrent && pending.is_empty() {
                                start_job(&mut running, job);
                            } else if pending.len() < max_pending {
                                pending.push_back(job);
                            } else {
                                let dropped = metrics.dropped.fetch_add(1, Ordering::Relaxed) + 1;
                                tracing::warn!(hook = %entry.display_name(), dropped, "Hook queue full; newest job dropped");
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Hook manager missed {} bus events; scripts cannot be replayed safely", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            while running.len() < max_concurrent {
                if let Some(job) = pending.pop_front() {
                    start_job(&mut running, job);
                } else {
                    break;
                }
            }
            metrics.running.store(running.len(), Ordering::Relaxed);
            metrics.queued.store(pending.len(), Ordering::Relaxed);
        }
    });
    handle
}

async fn run_shutdown_hooks(entries: Vec<HookEntry>, concurrency: usize, budget: Duration) {
    let run = async {
        let mut pending = entries
            .into_iter()
            .filter(|entry| matches(entry, HookEvent::DaemonShutdown, &[], None));
        let mut running = JoinSet::new();
        loop {
            while running.len() < concurrency {
                if let Some(entry) = pending.next() {
                    start_job(
                        &mut running,
                        Job {
                            entry,
                            env: vec![("LIANWALL_EVENT".into(), "daemon_shutdown".into())],
                        },
                    );
                } else {
                    break;
                }
            }
            if running.is_empty() {
                break;
            }
            let _ = running.join_next().await;
        }
    };
    if tokio::time::timeout(budget, run).await.is_err() {
        tracing::warn!("Shutdown hook budget expired; remaining cleanup jobs stopped");
    }
}

/// 将内部事件映射到 (HookEvent, 环境变量, 模式提示)
///
/// 返回 None 表示该事件不触发任何 hook。
type HookContext = (HookEvent, Vec<(String, String)>, Option<String>);

fn map_event(event: &Event) -> Option<HookContext> {
    let mut env = vec![];

    match event {
        Event::WallpaperChanged {
            path,
            mode,
            trigger,
        } => {
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
        Event::SchedulerTick | Event::ScanProgress { .. } | Event::EngineStateChanged { .. } => {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(on: HookEvent, command: String) -> HookEntry {
        HookEntry {
            name: None,
            on,
            command,
            mode: None,
            trigger: None,
            timeout: 10,
            enabled: true,
        }
    }
    fn quote(path: &std::path::Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
    }

    #[tokio::test]
    async fn overload_has_bounded_queue_and_counts_drops() {
        let bus = EventBus::new(256);
        let handle = spawn_with_config(
            &bus,
            HookConfig {
                max_concurrent: 1,
                max_pending: 2,
                hook: vec![entry(HookEvent::WallpaperChanged, "sleep 10".into())],
            },
        );
        for _ in 0..100 {
            bus.publish(Event::WallpaperChanged {
                path: "/test.jpg".into(),
                mode: WallMode::Image,
                trigger: lianwall_core::socket::WallpaperTrigger::ManualNext,
            });
        }
        tokio::time::timeout(Duration::from_secs(2), async {
            while handle.stats().dropped < 97 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(handle.stats().running, 1);
        assert_eq!(handle.stats().queued, 2);
        assert_eq!(handle.stats().dropped, 97);
        handle.shutdown().await;
        assert_eq!(handle.stats().running, 0);
        assert_eq!(handle.stats().queued, 0);
    }

    #[tokio::test]
    async fn shutdown_waits_for_cleanup_once() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("cleanup");
        let bus = EventBus::new(16);
        let handle = spawn_with_config(
            &bus,
            HookConfig {
                hook: vec![entry(
                    HookEvent::DaemonShutdown,
                    format!("printf x >> {}", quote(&marker)),
                )],
                ..HookConfig::default()
            },
        );
        bus.publish(Event::ShuttingDown);
        handle.shutdown().await;
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "x");
        handle.shutdown().await;
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "x");
    }

    #[tokio::test]
    async fn shutdown_budget_cancels_cleanup_descendants() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("late");
        run_shutdown_hooks(
            vec![entry(
                HookEvent::DaemonShutdown,
                format!("(sleep 0.2; printf late > {}) & wait", quote(&marker)),
            )],
            1,
            Duration::from_millis(30),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!marker.exists());
    }

    #[test]
    fn filters_require_matching_context() {
        let mut entry = entry(HookEvent::WallpaperChanged, ":".into());
        entry.mode = Some("image".into());
        entry.trigger = Some(vec!["manual_next".into()]);
        let env = vec![("LIANWALL_TRIGGER".into(), "manual_next".into())];
        assert!(matches(
            &entry,
            HookEvent::WallpaperChanged,
            &env,
            Some("image")
        ));
        assert!(!matches(
            &entry,
            HookEvent::WallpaperChanged,
            &env,
            Some("video")
        ));
        assert!(!matches(
            &entry,
            HookEvent::WallpaperChanged,
            &[],
            Some("image")
        ));
        assert!(!matches(&entry, HookEvent::WallpaperChanged, &env, None));
    }
}
