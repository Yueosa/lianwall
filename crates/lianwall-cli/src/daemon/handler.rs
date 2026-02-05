//! 请求处理与状态管理

use std::path::PathBuf;
use std::time::Instant;

use lianwall_core::algorithm::{select_next, select_previous};
use lianwall_core::config::{read, Config, ConfigReadInput, WallMode};
use lianwall_core::engine::{self, EngineState};
use lianwall_core::gpu::{self, VramState};
use lianwall_core::socket::{
    Request, Response, ResponseData, SpaceSnapshot, StatusInfo, WallpaperPoint, PROTOCOL_VERSION,
};
use lianwall_core::wallpaper::{
    export_to_persisted, load_weights, rebuild_space, save_weights, scan_directory,
    WeightsFile, WallpaperSpace,
};

use super::error::DaemonError;

/// 守护进程全局状态
pub struct DaemonState {
    /// 配置
    pub config: Config,
    /// 视频模式向量空间
    pub video_space: WallpaperSpace,
    /// 图片模式向量空间
    pub image_space: WallpaperSpace,
    /// 引擎状态
    pub engine: EngineState,
    /// GPU 监控状态
    pub gpu: VramState,
    /// 启动时间
    pub start_time: Instant,
    /// 是否请求关闭
    pub shutdown_requested: bool,
}

impl DaemonState {
    /// 初始化守护进程状态
    pub fn init(config: Config) -> Result<Self, DaemonError> {
        // 加载持久化数据
        let weights = load_weights().unwrap_or_default();

        // 扫描壁纸目录
        let video_paths = scan_directory(&config.paths.video_dir, true).unwrap_or_default();
        let image_paths = scan_directory(&config.paths.image_dir, false).unwrap_or_default();

        tracing::info!(
            "扫描完成: {} 个视频, {} 个图片",
            video_paths.len(),
            image_paths.len()
        );

        // 构建向量空间
        let video_space = rebuild_space(video_paths, None, Some(&weights.video), 0);
        let image_space = rebuild_space(image_paths, None, Some(&weights.image), 0);

        // 初始化引擎
        let engine = engine::init(&config).map_err(DaemonError::Engine)?;

        // 初始化 GPU 监控
        let gpu = gpu::init();

        Ok(Self {
            config,
            video_space,
            image_space,
            engine,
            gpu,
            start_time: Instant::now(),
            shutdown_requested: false,
        })
    }

    /// 获取当前模式的空间引用
    pub fn current_space(&self) -> &WallpaperSpace {
        match self.engine.mode {
            WallMode::Video => &self.video_space,
            WallMode::Image => &self.image_space,
        }
    }

    /// 获取当前模式的空间可变引用
    pub fn current_space_mut(&mut self) -> &mut WallpaperSpace {
        match self.engine.mode {
            WallMode::Video => &mut self.video_space,
            WallMode::Image => &mut self.image_space,
        }
    }

    /// 保存所有权重到文件
    pub fn save_weights(&self) -> Result<(), DaemonError> {
        let file = WeightsFile {
            version: 1,
            video: export_to_persisted(&self.video_space),
            image: export_to_persisted(&self.image_space),
        };
        save_weights(&file).map_err(DaemonError::Wallpaper)
    }
}

/// 处理单个请求
pub fn handle_request(state: &mut DaemonState, req: Request) -> Response {
    tracing::debug!("处理请求: {:?}", req.name());

    match req {
        Request::Ping => Response::with_data(ResponseData::Pong),
        Request::Status => handle_status(state),
        Request::GetSpace => handle_get_space(state),
        Request::Next => handle_next(state),
        Request::Previous => handle_previous(state),
        Request::SetWallpaper { path } => handle_set_wallpaper(state, path),
        Request::SetMode { mode } => handle_set_mode(state, mode),
        Request::Lock { path } => handle_lock(state, path),
        Request::Unlock { path } => handle_unlock(state, path),
        Request::Reload => handle_reload(state),
        Request::Shutdown => handle_shutdown(state),
    }
}

// ============================================================================
// 请求处理函数
// ============================================================================

fn handle_status(state: &DaemonState) -> Response {
    let space = state.current_space();
    let vram_info = lianwall_core::gpu::query_vram(state.gpu.backend).ok();

    let status = StatusInfo {
        mode: state.engine.mode,
        current: state.engine.current.clone(),
        engine: match state.engine.mode {
            WallMode::Video => "mpvpaper".to_string(),
            WallMode::Image => "swww".to_string(),
        },
        total_wallpapers: space.len(),
        locked_count: space.items.iter().filter(|w| w.locked).count(),
        available_count: space.available_count(),
        vram_used_mb: vram_info.as_ref().map(|v| v.used_mb).unwrap_or(0),
        vram_total_mb: vram_info.as_ref().map(|v| v.total_mb).unwrap_or(0),
        uptime_secs: state.start_time.elapsed().as_secs(),
        protocol_version: PROTOCOL_VERSION,
    };

    Response::with_data(ResponseData::Status(status))
}

fn handle_get_space(state: &DaemonState) -> Response {
    let space = state.current_space();

    let items: Vec<WallpaperPoint> = space
        .items
        .iter()
        .enumerate()
        .map(|(i, w)| WallpaperPoint {
            index: i,
            filename: w
                .path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
            path: w.path.clone(),
            angle: w.angle,
            locked: w.locked,
            in_cooldown: space.cooldown_queue.contains(&i),
        })
        .collect();

    let current_index = state
        .engine
        .current
        .as_ref()
        .and_then(|p| space.items.iter().position(|w| &w.path == p));

    let snapshot = SpaceSnapshot {
        items,
        pointer_angle: space.pointer,
        cooldown_size: space.cooldown_queue.len(),
        current_index,
    };

    Response::with_data(ResponseData::Space(snapshot))
}

fn handle_next(state: &mut DaemonState) -> Response {
    let space = state.current_space_mut();

    match select_next(space) {
        Some(output) => {
            let path = space.items[output.index].path.clone();

            // 设置壁纸
            if let Err(e) = engine::set_wallpaper(&mut state.engine, &path, &state.config) {
                return Response::err(format!("设置壁纸失败: {}", e));
            }

            // 保存状态
            let _ = state.save_weights();

            tracing::info!("切换壁纸: {}", path.display());
            Response::ok()
        }
        None => Response::err("没有可用的壁纸"),
    }
}

fn handle_previous(state: &mut DaemonState) -> Response {
    let space = state.current_space_mut();

    match select_previous(space) {
        Some(output) => {
            let path = space.items[output.index].path.clone();

            // 设置壁纸（prev 忽略锁定状态，强制播放）
            if let Err(e) = engine::set_wallpaper(&mut state.engine, &path, &state.config) {
                return Response::err(format!("设置壁纸失败: {}", e));
            }

            // 保存状态
            let _ = state.save_weights();

            tracing::info!("回退壁纸: {}", path.display());
            Response::ok()
        }
        None => Response::err("没有播放历史，无法回退"),
    }
}

fn handle_set_wallpaper(state: &mut DaemonState, path: PathBuf) -> Response {
    if !path.exists() {
        return Response::err(format!("文件不存在: {}", path.display()));
    }

    if let Err(e) = engine::set_wallpaper(&mut state.engine, &path, &state.config) {
        return Response::err(format!("设置壁纸失败: {}", e));
    }

    tracing::info!("指定壁纸: {}", path.display());
    Response::ok()
}

fn handle_set_mode(state: &mut DaemonState, mode: WallMode) -> Response {
    if state.engine.mode == mode {
        return Response::ok();
    }

    if let Err(e) = engine::switch_mode(&mut state.engine, mode, &state.config) {
        return Response::err(format!("切换模式失败: {}", e));
    }

    tracing::info!("切换模式: {:?}", mode);

    // 切换模式后立即播放一张新壁纸
    let space = state.current_space_mut();
    if let Some(output) = select_next(space) {
        let path = space.items[output.index].path.clone();
        if let Err(e) = engine::set_wallpaper(&mut state.engine, &path, &state.config) {
            tracing::warn!("切换模式后设置壁纸失败: {}", e);
        } else {
            let _ = state.save_weights();
            tracing::info!("切换模式后播放壁纸: {}", path.display());
        }
    }

    Response::ok()
}

fn handle_lock(state: &mut DaemonState, path: PathBuf) -> Response {
    let space = state.current_space_mut();

    if let Some(item) = space.items.iter_mut().find(|w| w.path == path) {
        item.locked = true;
        let _ = state.save_weights();
        tracing::info!("锁定壁纸: {}", path.display());
        Response::ok()
    } else {
        Response::err(format!("壁纸不存在: {}", path.display()))
    }
}

fn handle_unlock(state: &mut DaemonState, path: PathBuf) -> Response {
    let space = state.current_space_mut();

    if let Some(item) = space.items.iter_mut().find(|w| w.path == path) {
        item.locked = false;
        let _ = state.save_weights();
        tracing::info!("解锁壁纸: {}", path.display());
        Response::ok()
    } else {
        Response::err(format!("壁纸不存在: {}", path.display()))
    }
}

fn handle_reload(state: &mut DaemonState) -> Response {
    // 重新加载配置
    let config = match read(ConfigReadInput { path: None }) {
        Ok(output) => output.config,
        Err(e) => return Response::err(format!("加载配置失败: {}", e)),
    };

    // 保存旧的指针位置
    let old_video_pointer = state.video_space.pointer;
    let old_image_pointer = state.image_space.pointer;

    // 重新扫描目录
    let video_paths = scan_directory(&config.paths.video_dir, true).unwrap_or_default();
    let image_paths = scan_directory(&config.paths.image_dir, false).unwrap_or_default();

    // 重建空间（保留锁定状态）
    let weights = load_weights().unwrap_or_default();
    state.video_space = rebuild_space(video_paths, Some(&state.video_space), Some(&weights.video), 0);
    state.image_space = rebuild_space(image_paths, Some(&state.image_space), Some(&weights.image), 0);

    // 恢复指针
    state.video_space.pointer = old_video_pointer;
    state.image_space.pointer = old_image_pointer;

    state.config = config;

    tracing::info!(
        "重载完成: {} 个视频, {} 个图片",
        state.video_space.len(),
        state.image_space.len()
    );

    Response::ok()
}

fn handle_shutdown(state: &mut DaemonState) -> Response {
    state.shutdown_requested = true;
    tracing::info!("收到关闭请求");
    Response::ok()
}
