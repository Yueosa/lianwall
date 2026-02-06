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
use tokio::signal::unix::{signal, SignalKind};

use lianwall_daemon::{command, event::EventBus, scheduler, server, state::SharedState};
use lianwall_core::wallpaper::{scan_directory_async, rebuild_space, load_weights};
use lianwall_core::config::ConfigReadInput;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "lianwalld=info,lianwall_core=info".into()),
        )
        .init();

    tracing::info!(
        "LianWall Daemon v{} starting...",
        env!("CARGO_PKG_VERSION")
    );

    // 加载配置
    let config = match lianwall_core::config::read(ConfigReadInput {
        path: None,
    }) {
        Ok(result) => {
            tracing::info!("Config loaded from {:?}", result.path);
            result.config
        }
        Err(e) => {
            tracing::error!("Failed to load config: {}, daemon cannot start without config", e);
            return Err(anyhow::anyhow!("Config load failed: {}", e));
        }
    };

    // 初始化共享状态
    let state = SharedState::init(config.clone()).await?;
    tracing::info!("SharedState initialized");

    // 创建事件总线
    let event_bus = EventBus::new(1024);
    tracing::info!("EventBus created");

    // 创建命令队列
    let (cmd_queue, cmd_rx) = command::CommandQueue::new(256);
    let cmd_tx = cmd_queue.sender();
    tracing::info!("CommandQueue created");

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
    let video_paths: Vec<std::path::PathBuf> = video_wallpapers.iter().map(|w| w.path.clone()).collect();
    
    // 扫描图片壁纸  
    let image_result = scan_directory_async(config.paths.image_dir.clone(), false).await;
    let image_wallpapers = match &image_result {
        Ok(result) => result.wallpapers.clone(),
        Err(e) => {
            tracing::warn!("Failed to scan images: {}", e);
            vec![]
        }
    };
    let image_paths: Vec<std::path::PathBuf> = image_wallpapers.iter().map(|w| w.path.clone()).collect();

    // 收集时间点
    let mut all_wallpapers = video_wallpapers;
    all_wallpapers.extend(image_wallpapers);
    let time_points = lianwall_core::wallpaper::collect_time_points(&all_wallpapers);
    tracing::info!("Found {} time points", time_points.len());
    state.set_time_points(time_points).await;

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
            video_paths,
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
            image_paths,
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
        if let Err(e) = server::run(state_clone, event_bus_clone, cmd_tx_clone).await {
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

    // GPU Monitor Task (optional)
    let gpu_handle = if config.vram.enabled {
        let state_clone = Arc::clone(&state);
        let event_bus_clone = event_bus.clone();
        let cmd_tx_clone = cmd_queue.sender();
        Some(tokio::spawn(async move {
            scheduler::gpu_monitor(state_clone, event_bus_clone, cmd_tx_clone).await;
        }))
    } else {
        None
    };

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
                    space.current_index.and_then(|idx| space.items.get(idx).map(|i| i.path.clone()))
                }
                lianwall_core::config::WallMode::Image => {
                    let space = state.image_space.read().await;
                    space.current_index.and_then(|idx| space.items.get(idx).map(|i| i.path.clone()))
                }
            };
            
            if let Some(path) = path {
                tracing::info!("Restoring last wallpaper: {:?}", path);
                let (response_tx, _) = tokio::sync::oneshot::channel();
                let _ = cmd_queue.sender().send(command::CommandMsg {
                    request: lianwall_core::socket::Request::SetWallpaper { path },
                    response_tx,
                }).await;
            }
        } else {
            // 没有上次记录，抽一张新的
            tracing::info!("No previous wallpaper, selecting new one");
            let (response_tx, _) = tokio::sync::oneshot::channel();
            let _ = cmd_queue.sender().send(command::CommandMsg {
                request: lianwall_core::socket::Request::Next,
                response_tx,
            }).await;
        }
    }

    // 等待关闭信号
    let mut sigint = signal(SignalKind::interrupt())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut shutdown_rx = state.shutdown_receiver();

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

    // 保存状态到持久化文件
    {
        use lianwall_core::wallpaper::{export_to_persisted, save_weights, WeightsFile};
        
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

    // 停止引擎进程
    state.engine.swww_daemon.kill().await;
    state.engine.mpvpaper.kill().await;

    // 等待 task 结束（带超时）
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        async {
            let _ = server_handle.await;
            let _ = cmd_handle.await;
            let _ = scheduler_handle.await;
            if let Some(h) = gpu_handle {
                let _ = h.await;
            }
        },
    )
    .await;

    tracing::info!("Daemon stopped");

    Ok(())
}
