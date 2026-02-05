//! 请求处理与状态管理

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Instant;

use lianwall_core::algorithm::{calc_cooldown, select_next, select_previous};
use lianwall_core::config::{read, Config, ConfigReadInput, WallMode};
use lianwall_core::engine::{self, EngineState};
use lianwall_core::gpu::{self, VramState};
use lianwall_core::socket::{
    ModeSchedule, Request, Response, ResponseData, SpaceSnapshot, StatusInfo,
    TimeRangeInfo, TimeScheduleInfo, WallpaperPoint, WallpaperTimeSegment, PROTOCOL_VERSION,
};
use lianwall_core::wallpaper::{
    export_to_persisted, filter_active, load_weights, rebuild_space, save_weights, scan_directory,
    next_key_point, ScanResult, ScannedWallpaper, TimePoint, WeightsFile, WallpaperSpace,
};

use super::error::DaemonError;

/// 守护进程全局状态
pub struct DaemonState {
    /// 配置
    pub config: Config,
    
    // === 扫描结果（持久） ===
    /// 视频模式扫描结果
    pub video_scanned: Vec<ScannedWallpaper>,
    /// 图片模式扫描结果
    pub image_scanned: Vec<ScannedWallpaper>,
    /// 视频模式关键时间点
    pub video_time_points: BTreeSet<TimePoint>,
    /// 图片模式关键时间点
    pub image_time_points: BTreeSet<TimePoint>,
    
    // === 向量空间（动态） ===
    /// 视频模式向量空间
    pub video_space: WallpaperSpace,
    /// 图片模式向量空间
    pub image_space: WallpaperSpace,
    
    // === 引擎与监控 ===
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
        let now = TimePoint::now();

        // 扫描壁纸目录（递归，含时间约束）
        let video_scan = scan_directory(&config.paths.video_dir, true)
            .unwrap_or_else(|_| ScanResult {
                wallpapers: vec![],
                time_points: BTreeSet::new(),
            });
        let image_scan = scan_directory(&config.paths.image_dir, false)
            .unwrap_or_else(|_| ScanResult {
                wallpapers: vec![],
                time_points: BTreeSet::new(),
            });

        tracing::info!(
            "扫描完成: {} 个视频 ({} 个时间点), {} 个图片 ({} 个时间点)",
            video_scan.wallpapers.len(),
            video_scan.time_points.len(),
            image_scan.wallpapers.len(),
            image_scan.time_points.len(),
        );

        // 过滤当前活跃的壁纸
        let video_active = filter_active(&video_scan.wallpapers, &now);
        let image_active = filter_active(&image_scan.wallpapers, &now);

        tracing::info!(
            "当前活跃: {} 个视频, {} 个图片",
            video_active.len(),
            image_active.len()
        );

        // 构建向量空间
        let video_space = rebuild_space(video_active, None, Some(&weights.video), 0);
        let image_space = rebuild_space(image_active, None, Some(&weights.image), 0);

        // 初始化引擎
        let engine = engine::init(&config).map_err(DaemonError::Engine)?;

        // 初始化 GPU 监控
        let gpu = gpu::init();

        Ok(Self {
            config,
            video_scanned: video_scan.wallpapers,
            image_scanned: image_scan.wallpapers,
            video_time_points: video_scan.time_points,
            image_time_points: image_scan.time_points,
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

    /// 获取合并后的所有关键时间点
    pub fn all_time_points(&self) -> BTreeSet<TimePoint> {
        self.video_time_points
            .union(&self.image_time_points)
            .copied()
            .collect()
    }

    /// 获取下一个关键时间点
    pub fn next_time_point(&self) -> Option<TimePoint> {
        let now = TimePoint::now();
        let all_points = self.all_time_points();
        next_key_point(&now, &all_points)
    }

    /// 刷新活跃壁纸（时间触发）
    ///
    /// 重新过滤活跃壁纸，重建向量空间
    pub fn refresh_active_wallpapers(&mut self) {
        let now = TimePoint::now();
        let weights = load_weights().unwrap_or_default();

        // 过滤活跃壁纸
        let video_active = filter_active(&self.video_scanned, &now);
        let image_active = filter_active(&self.image_scanned, &now);

        tracing::info!(
            "时间触发刷新: {} 个视频, {} 个图片 (时间 {:02}:{:02})",
            video_active.len(),
            image_active.len(),
            now.hour,
            now.minute
        );

        // 重建空间（保留锁定状态，但重置指针和冷却队列）
        self.video_space = rebuild_space(video_active, None, Some(&weights.video), 0);
        self.image_space = rebuild_space(image_active, None, Some(&weights.image), 0);

        // 检查冷却队列冲突
        self.check_cooldown_conflict();

        // 如果当前空间为空，清空壁纸
        if self.current_space().is_empty() {
            tracing::warn!("当前时间段没有可用壁纸，清空显示");
            let _ = engine::clear_wallpaper(&mut self.engine, &self.config);
        }
    }

    /// 检查并处理冷却队列冲突
    ///
    /// 如果可用壁纸数 <= 冷却队列大小，清空冷却队列
    fn check_cooldown_conflict(&mut self) {
        // 检查视频空间
        let video_available = self.video_space.available_count();
        let video_cooldown = calc_cooldown(self.video_space.len());
        if video_available > 0 && video_available <= video_cooldown {
            tracing::warn!(
                "视频空间可用壁纸({}) <= 冷却大小({})，清空冷却队列",
                video_available,
                video_cooldown
            );
            self.video_space.cooldown_queue.clear();
        }

        // 检查图片空间
        let image_available = self.image_space.available_count();
        let image_cooldown = calc_cooldown(self.image_space.len());
        if image_available > 0 && image_available <= image_cooldown {
            tracing::warn!(
                "图片空间可用壁纸({}) <= 冷却大小({})，清空冷却队列",
                image_available,
                image_cooldown
            );
            self.image_space.cooldown_queue.clear();
        }
    }
}

/// 处理单个请求
pub fn handle_request(state: &mut DaemonState, req: Request) -> Response {
    tracing::debug!("处理请求: {:?}", req.name());

    match req {
        Request::Ping => Response::with_data(ResponseData::Pong),
        Request::Status => handle_status(state),
        Request::GetSpace => handle_get_space(state),
        Request::GetTimeInfo => handle_get_time_info(state),
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

    // 获取当前模式的扫描数和时间信息
    let (scanned_count, time_points) = match state.engine.mode {
        WallMode::Video => (state.video_scanned.len(), &state.video_time_points),
        WallMode::Image => (state.image_scanned.len(), &state.image_time_points),
    };

    let next_tp = state.next_time_point();

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
        scanned_count,
        vram_used_mb: vram_info.as_ref().map(|v| v.used_mb).unwrap_or(0),
        vram_total_mb: vram_info.as_ref().map(|v| v.total_mb).unwrap_or(0),
        uptime_secs: state.start_time.elapsed().as_secs(),
        protocol_version: PROTOCOL_VERSION,
        next_time_point: next_tp.map(|tp| format!("{:02}:{:02}", tp.hour, tp.minute)),
        time_points_count: time_points.len(),
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

fn handle_get_time_info(state: &DaemonState) -> Response {
    let now = TimePoint::now();

    let info = TimeScheduleInfo {
        current_time: format!("{:02}:{:02}", now.hour, now.minute),
        video_schedule: build_mode_schedule(
            &state.video_scanned,
            &state.video_time_points,
            &now,
        ),
        image_schedule: build_mode_schedule(
            &state.image_scanned,
            &state.image_time_points,
            &now,
        ),
    };

    Response::with_data(ResponseData::TimeInfo(info))
}

/// 构建单个模式的调度信息
fn build_mode_schedule(
    scanned: &[ScannedWallpaper],
    time_points: &BTreeSet<TimePoint>,
    now: &TimePoint,
) -> ModeSchedule {
    // 计算活跃数
    let active_count = filter_active(scanned, now).len();

    // 时间点列表
    let points: Vec<String> = time_points
        .iter()
        .map(|tp| format!("{:02}:{:02}", tp.hour, tp.minute))
        .collect();

    // 下一个时间点
    let next_tp = next_key_point(now, time_points)
        .map(|tp| format!("{:02}:{:02}", tp.hour, tp.minute));

    // 构建壁纸时间段
    let wallpaper_segments: Vec<WallpaperTimeSegment> = scanned
        .iter()
        .map(|w| {
            let filename = w.path
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            let all_day = w.time_constraints.is_empty();

            let active_ranges: Vec<TimeRangeInfo> = w.time_constraints
                .iter()
                .map(|tr| {
                    let crosses_midnight = tr.start.to_minutes() > tr.end.to_minutes();
                    TimeRangeInfo {
                        start: format!("{:02}:{:02}", tr.start.hour, tr.start.minute),
                        end: format!("{:02}:{:02}", tr.end.hour, tr.end.minute),
                        crosses_midnight,
                    }
                })
                .collect();

            WallpaperTimeSegment {
                filename,
                path: w.path.clone(),
                active_ranges,
                all_day,
            }
        })
        .collect();

    ModeSchedule {
        scanned_count: scanned.len(),
        active_count,
        time_points: points,
        next_time_point: next_tp,
        wallpaper_segments,
    }
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

    let now = TimePoint::now();

    // 重新扫描目录（递归，含时间约束）
    let video_scan = scan_directory(&config.paths.video_dir, true)
        .unwrap_or_else(|_| ScanResult {
            wallpapers: vec![],
            time_points: BTreeSet::new(),
        });
    let image_scan = scan_directory(&config.paths.image_dir, false)
        .unwrap_or_else(|_| ScanResult {
            wallpapers: vec![],
            time_points: BTreeSet::new(),
        });

    // 更新扫描结果
    state.video_scanned = video_scan.wallpapers;
    state.image_scanned = image_scan.wallpapers;
    state.video_time_points = video_scan.time_points;
    state.image_time_points = image_scan.time_points;

    // 过滤当前活跃的壁纸
    let video_active = filter_active(&state.video_scanned, &now);
    let image_active = filter_active(&state.image_scanned, &now);

    // 重建空间（保留锁定状态）
    let weights = load_weights().unwrap_or_default();
    state.video_space = rebuild_space(video_active, Some(&state.video_space), Some(&weights.video), 0);
    state.image_space = rebuild_space(image_active, Some(&state.image_space), Some(&weights.image), 0);

    // 恢复指针
    state.video_space.pointer = old_video_pointer;
    state.image_space.pointer = old_image_pointer;

    // 检查冷却队列冲突
    state.check_cooldown_conflict();

    state.config = config;

    tracing::info!(
        "重载完成: {} 个视频 ({} 个时间点), {} 个图片 ({} 个时间点)",
        state.video_space.len(),
        state.video_time_points.len(),
        state.image_space.len(),
        state.image_time_points.len(),
    );

    Response::ok()
}

fn handle_shutdown(state: &mut DaemonState) -> Response {
    state.shutdown_requested = true;
    tracing::info!("收到关闭请求");
    Response::ok()
}
