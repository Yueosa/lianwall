//! LianWall Daemon
//!
//! 壁纸管理守护进程
//!
//! # 架构
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                        lianwalld (Tokio)                                │
//! │                                                                         │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌────────────┐   │
//! │  │ Server Task  │  │ Command Task │  │ Scheduler    │  │ GPU Monitor│   │
//! │  │              │  │              │  │              │  │ (optional) │   │
//! │  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └─────┬──────┘   │
//! │         │                 │                 │                │          │
//! │         └─────────────────┼─────────────────┼────────────────┘          │
//! │                           │                 │                           │
//! │                    ┌──────┴─────────────────┴──────┐                    │
//! │                    │        SharedState            │                    │
//! │                    │   + EventBus (broadcast)      │                    │
//! │                    └───────────────────────────────┘                    │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```

use std::sync::Arc;
use tokio::signal::unix::{SignalKind, signal};

use lianwall_core::config::ConfigCreateInput;
use lianwall_core::wallpaper::{filter_active, load_weights, rebuild_space, scan_directory_async};
use lianwall_daemon::{command, event::EventBus, hook, scheduler, server, state::SharedState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Register before opening the server, so shutdown during startup is also captured.
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;

    // 加载配置（不存在则自动创建默认配置）
    let config = match lianwall_core::config::create(ConfigCreateInput { path: None }) {
        Ok(result) => result.config,
        Err(e) => {
            eprintln!("Failed to load/create config: {e}");
            return Err(anyhow::anyhow!("Config load failed: {}", e));
        }
    };

    // Refuse duplicate startup before touching engines, hooks, or persistent state.
    let bound_socket = server::bind(&config.daemon.socket_path)?;
    let _socket_lifetime = bound_socket.keep_alive();

    // 初始化共享状态
    let state = SharedState::init(config.clone()).await?;
    if let Some(directory) = dirs::state_dir() {
        state.logs.open(directory.join("lianwall/daemon.jsonl"));
    }
    lianwall_daemon::logging::init(&state.logs, &config.daemon.log_level)?;
    tracing::info!(
        "LianWall Daemon v{} starting; configuration ready",
        env!("CARGO_PKG_VERSION")
    );
    tracing::info!("SharedState initialized");
    if let Some(directory) = dirs::data_local_dir() {
        state
            .library
            .open(directory.join("lianwall/library.json"))
            .await;
    }
    let library_state = state.clone();
    let mut library_shutdown = state.shutdown_receiver();
    let library_checkpoint = tokio::spawn(async move {
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(60));
        timer.tick().await;
        loop {
            tokio::select! {
                _ = library_shutdown.recv() => break,
                _ = timer.tick() => library_state.library.checkpoint().await,
            }
        }
    });
    let mut shutdown_rx = state.shutdown_receiver();

    // 创建事件总线
    let event_bus = EventBus::new(1024);
    tracing::info!("EventBus created");

    // 创建命令队列
    let (cmd_queue, cmd_rx) = command::CommandQueue::new(256);
    let cmd_tx = cmd_queue.sender();
    tracing::info!("CommandQueue created");

    // 启动 Hook 管理器
    let hook_handle = hook::spawn(&event_bus);
    *state.hook_handle.write().await = Some(hook_handle.clone());

    // 初始扫描壁纸
    tracing::info!("Scanning wallpapers...");

    // 扫描视频壁纸
    let video_result = scan_directory_async(config.paths.video_dir.clone(), true).await;
    let video_wallpapers = match &video_result {
        Ok(result) => result.wallpapers.clone(),
        Err(e) => {
            tracing::warn!("Failed to scan videos: {}", e);
            vec![]
        }
    };

    // 扫描图片壁纸
    let image_result = scan_directory_async(config.paths.image_dir.clone(), false).await;
    let image_wallpapers = match &image_result {
        Ok(result) => result.wallpapers.clone(),
        Err(e) => {
            tracing::warn!("Failed to scan images: {}", e);
            vec![]
        }
    };

    state
        .library
        .discover(
            video_wallpapers
                .iter()
                .chain(image_wallpapers.iter())
                .map(|item| item.path.clone()),
        )
        .await;

    // 收集时间点（在过滤前收集，保留所有时间点信息）
    let mut all_wallpapers_for_time = video_wallpapers.clone();
    all_wallpapers_for_time.extend(image_wallpapers.clone());
    let time_points = lianwall_core::wallpaper::collect_time_points(&all_wallpapers_for_time);
    tracing::info!("Found {} time points", time_points.len());
    state.set_time_points(time_points).await;

    // 保存原始扫描总数（过滤前）
    *state.scanned_counts.write().await = (video_wallpapers.len(), image_wallpapers.len());

    // 根据当前时间过滤活跃壁纸
    let active_videos = filter_active(&video_wallpapers);
    let active_images = filter_active(&image_wallpapers);

    // 克隆活跃壁纸用于构建空间（保留时间约束信息）
    let active_videos_owned: Vec<_> = active_videos.into_iter().cloned().collect();
    let active_images_owned: Vec<_> = active_images.into_iter().cloned().collect();

    tracing::info!(
        "Time filter: {}/{} videos active, {}/{} images active",
        active_videos_owned.len(),
        video_wallpapers.len(),
        active_images_owned.len(),
        image_wallpapers.len()
    );

    // 加载持久化数据
    let weights = match load_weights() {
        Ok(w) => {
            tracing::info!("Loaded weights from cache");
            Some(w)
        }
        Err(e) => {
            tracing::warn!("Failed to load weights: {}, starting fresh", e);
            None
        }
    };

    // 构建向量空间（恢复历史状态）
    {
        let mut video_space = state.video_space.write().await;
        *video_space = rebuild_space(
            active_videos_owned,
            None,
            weights.as_ref().map(|w| &w.video),
            0,
        );
        tracing::info!(
            "Found {} video wallpapers, current_index: {:?}",
            video_space.items.len(),
            video_space.current_index
        );
    }

    {
        let mut image_space = state.image_space.write().await;
        *image_space = rebuild_space(
            active_images_owned,
            None,
            weights.as_ref().map(|w| &w.image),
            0,
        );
        tracing::info!(
            "Found {} image wallpapers, current_index: {:?}",
            image_space.items.len(),
            image_space.current_index
        );
    }

    // 启动各个 Task
    let state_clone = Arc::clone(&state);
    let event_bus_clone = event_bus.clone();
    let cmd_tx_clone = cmd_tx.clone();

    // Server Task
    let server_handle = tokio::spawn(async move {
        if let Err(e) =
            server::run_bound(bound_socket, state_clone, event_bus_clone, cmd_tx_clone).await
        {
            tracing::error!("Server error: {}", e);
        }
    });

    // Command Queue Task
    let state_clone = Arc::clone(&state);
    let event_bus_clone = event_bus.clone();
    let cmd_handle = tokio::spawn(async move {
        command::run(state_clone, event_bus_clone, cmd_rx).await;
    });

    // Scheduler Task
    let state_clone = Arc::clone(&state);
    let event_bus_clone = event_bus.clone();
    let scheduler_handle = tokio::spawn(async move {
        scheduler::run(state_clone, event_bus_clone, cmd_tx).await;
    });

    // Keep a cheap idle task so enabling monitoring from the GUI takes effect
    // without restarting the daemon. Disabled monitoring never executes commands.
    let state_clone = Arc::clone(&state);
    let event_bus_clone = event_bus.clone();
    let cmd_tx_clone = cmd_queue.sender();
    let gpu_handle = Some(tokio::spawn(async move {
        scheduler::gpu_monitor(state_clone, event_bus_clone, cmd_tx_clone).await;
    }));

    tracing::info!("All tasks started, daemon is ready");

    // 应用初始壁纸
    {
        let mode = *state.engine.mode.read().await;
        let has_current = match mode {
            lianwall_core::config::WallMode::Video => {
                state.video_space.read().await.current_index.is_some()
            }
            lianwall_core::config::WallMode::Image => {
                state.image_space.read().await.current_index.is_some()
            }
        };

        if has_current {
            // 恢复上次的壁纸：发送 SetWallpaper 命令
            let path = match mode {
                lianwall_core::config::WallMode::Video => {
                    let space = state.video_space.read().await;
                    space
                        .current_index
                        .and_then(|idx| space.items.get(idx).map(|i| i.path.clone()))
                }
                lianwall_core::config::WallMode::Image => {
                    let space = state.image_space.read().await;
                    space
                        .current_index
                        .and_then(|idx| space.items.get(idx).map(|i| i.path.clone()))
                }
            };

            if let Some(path) = path {
                tracing::info!("Restoring last wallpaper: {:?}", path);
                let (response_tx, _) = tokio::sync::oneshot::channel();
                let _ = cmd_queue
                    .sender()
                    .send(command::CommandMsg {
                        request: lianwall_core::socket::Request::SetWallpaper { path },
                        response_tx,
                    })
                    .await;
            }
        } else {
            // 没有上次记录，抽一张新的
            tracing::info!("No previous wallpaper, selecting new one");
            let (response_tx, _) = tokio::sync::oneshot::channel();
            let _ = cmd_queue
                .sender()
                .send(command::CommandMsg {
                    request: lianwall_core::socket::Request::Next {
                        trigger_hint: Some(lianwall_core::socket::WallpaperTrigger::DaemonStart),
                    },
                    response_tx,
                })
                .await;
        }
    }

    // 等待关闭信号

    tokio::select! {
        _ = sigint.recv() => {
            tracing::info!("Received SIGINT");
        }
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM");
        }
        _ = shutdown_rx.recv() => {
            tracing::info!("Received shutdown command");
        }
    }

    // 优雅关闭
    tracing::info!("Shutting down...");

    // Stop accepting ordinary work before cleanup, regardless of shutdown source.
    state.trigger_shutdown();
    // One cleanup path for CLI Shutdown, SIGINT and SIGTERM, before engine cleanup.
    event_bus.publish(lianwall_daemon::event::Event::ShuttingDown);
    hook_handle.shutdown().await;

    let _ = library_checkpoint.await;
    state.library.checkpoint().await;

    // 保存状态到持久化文件
    {
        use lianwall_core::wallpaper::{WeightsFile, export_to_persisted, save_weights};

        let video_space = state.video_space.read().await;
        let image_space = state.image_space.read().await;

        let weights = WeightsFile {
            version: 1,
            video: export_to_persisted(&video_space),
            image: export_to_persisted(&image_space),
        };

        if let Err(e) = save_weights(&weights) {
            tracing::warn!("Failed to save weights: {}", e);
        } else {
            tracing::info!("Saved weights to cache");
        }
    }

    // 触发关闭（通知所有 task）
    state.trigger_shutdown();

    lianwall_daemon::backend::shutdown_native(&state).await;

    // 停止引擎进程
    state.engine.swww_daemon.kill().await;
    state.engine.mpvpaper.kill().await;

    lianwall_daemon::backend::cleanup_ipc(&state).await;

    // 等待 task 结束（带超时）
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let _ = server_handle.await;
        let _ = cmd_handle.await;
        let _ = scheduler_handle.await;
        if let Some(h) = gpu_handle {
            let _ = h.await;
        }
    })
    .await;

    tracing::info!("Daemon stopped");
    state.logs.flush().await;

    Ok(())
}
