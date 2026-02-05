//! 并行目录扫描
//!
//! 使用 rayon + walkdir 实现高效的并行扫描，支持流式返回和进度报告。
//!
//! ## 优化策略
//! - **walkdir**: 高效目录遍历，避免递归栈溢出
//! - **rayon**: 多核并行处理文件解析
//! - **流式返回**: 边扫描边返回，大目录不阻塞
//! - **进度报告**: 实时反馈扫描进度

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::mpsc;
use walkdir::WalkDir;

use super::error::WallpaperError;
use super::time_range::{parse_time_dir, TimePoint, TimeRange};

/// 单个扫描到的壁纸
#[derive(Debug, Clone)]
pub struct ScannedWallpaper {
    /// 文件路径
    pub path: PathBuf,
    /// 时间约束列表（可能为空，表示无时间限制）
    pub time_constraints: Vec<TimeRange>,
}

/// 扫描结果
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// 扫描到的壁纸列表
    pub wallpapers: Vec<ScannedWallpaper>,
    /// 收集到的时间关键点
    pub time_points: BTreeSet<TimePoint>,
}

/// mpv 支持的视频文件扩展名
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "webm", "avi", "mov", "flv", "wmv", "m4v", "3gp", "ogv", "ts", "m2ts",
];

/// swww 支持的图片文件扩展名
const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff", "tif", "pnm", "tga", "farbfeld",
];

/// 并行扫描配置
#[derive(Debug, Clone)]
pub struct ParallelScanConfig {
    /// 并行度（默认 = CPU 核心数 / 2，至少为 1）
    pub parallelism: usize,
    /// 每批返回的最大文件数
    pub batch_size: usize,
    /// 扫描超时（秒）
    pub timeout_secs: u64,
    /// 是否跟随符号链接
    pub follow_links: bool,
}

impl Default for ParallelScanConfig {
    fn default() -> Self {
        let parallelism = (num_cpus::get() / 2).max(1);
        Self {
            parallelism,
            batch_size: 100,
            timeout_secs: 30,
            follow_links: true,
        }
    }
}

/// 扫描进度
#[derive(Debug, Clone)]
pub struct ScanProgress {
    /// 已扫描目录数
    pub dirs_scanned: usize,
    /// 已发现壁纸数
    pub files_found: usize,
    /// 是否完成
    pub completed: bool,
    /// 错误信息（如果有）
    pub error: Option<String>,
}

impl ScanProgress {
    fn new(dirs: usize, files: usize) -> Self {
        Self {
            dirs_scanned: dirs,
            files_found: files,
            completed: false,
            error: None,
        }
    }

    fn completed(dirs: usize, files: usize) -> Self {
        Self {
            dirs_scanned: dirs,
            files_found: files,
            completed: true,
            error: None,
        }
    }

    #[allow(dead_code)]
    fn error(message: String) -> Self {
        Self {
            dirs_scanned: 0,
            files_found: 0,
            completed: true,
            error: Some(message),
        }
    }
}

/// 中间扫描结果（带路径的时间约束映射）
struct IntermediateResult {
    path: PathBuf,
    constraints: Vec<TimeRange>,
}

/// 判断是否为隐藏文件/目录
/// 
/// 注意：只检查文件名是否以 `.` 开头，不检查父目录
fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    // 深度为 0 表示根目录本身，不应该被过滤
    if entry.depth() == 0 {
        return false;
    }
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

/// 检查文件扩展名是否匹配
fn has_valid_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| extensions.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// 从路径解析时间约束
///
/// 遍历路径的所有父目录，检查每个目录名是否为时间范围
fn parse_time_constraints_from_path(path: &Path, root: &Path) -> (Vec<TimeRange>, BTreeSet<TimePoint>) {
    let mut constraints = Vec::new();
    let mut time_points = BTreeSet::new();

    // 获取相对于 root 的路径
    if let Ok(relative) = path.strip_prefix(root) {
        for component in relative.parent().into_iter().flat_map(|p| p.components()) {
            if let std::path::Component::Normal(name) = component {
                if let Some(name_str) = name.to_str() {
                    if let Some(range) = parse_time_dir(name_str) {
                        for point in range.key_points() {
                            time_points.insert(point);
                        }
                        constraints.push(range);
                    }
                }
            }
        }
    }

    (constraints, time_points)
}

// ==================== 同步版本（使用 rayon）====================

/// 使用 rayon 并行扫描（同步版本）
///
/// 适合在 tokio::task::spawn_blocking 中调用
pub fn scan_with_rayon(dir: &Path, is_video: bool) -> Result<ScanResult, WallpaperError> {
    if !dir.exists() {
        return Err(WallpaperError::DirectoryNotFound(dir.to_path_buf()));
    }

    let extensions = if is_video {
        VIDEO_EXTENSIONS
    } else {
        IMAGE_EXTENSIONS
    };

    // 第一步：收集所有文件路径（快速 I/O）
    let entries: Vec<IntermediateResult> = WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| has_valid_extension(e.path(), extensions))
        .map(|e| {
            let path = e.path().to_path_buf();
            let (constraints, _) = parse_time_constraints_from_path(&path, dir);
            IntermediateResult { path, constraints }
        })
        .collect();

    if entries.is_empty() {
        return Err(WallpaperError::NoWallpapers(dir.to_path_buf()));
    }

    // 第二步：并行处理，收集时间点
    let mut all_time_points = BTreeSet::new();
    let wallpapers: Vec<ScannedWallpaper> = entries
        .into_iter()
        .map(|result| {
            // 收集时间点
            for constraint in &result.constraints {
                for point in constraint.key_points() {
                    all_time_points.insert(point);
                }
            }
            ScannedWallpaper {
                path: result.path,
                time_constraints: result.constraints,
            }
        })
        .collect();

    Ok(ScanResult {
        wallpapers,
        time_points: all_time_points,
    })
}

/// 使用 rayon 并行扫描 + 进度回调
pub fn scan_with_rayon_progress<F>(
    dir: &Path,
    is_video: bool,
    mut progress_callback: F,
) -> Result<ScanResult, WallpaperError>
where
    F: FnMut(ScanProgress),
{
    if !dir.exists() {
        return Err(WallpaperError::DirectoryNotFound(dir.to_path_buf()));
    }

    let extensions = if is_video {
        VIDEO_EXTENSIONS
    } else {
        IMAGE_EXTENSIONS
    };

    let dirs_scanned = AtomicUsize::new(0);
    let files_found = AtomicUsize::new(0);

    // 收集文件路径
    let entries: Vec<IntermediateResult> = WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| !is_hidden(e))
        .filter_map(|e| e.ok())
        .inspect(|e| {
            if e.file_type().is_dir() {
                let count = dirs_scanned.fetch_add(1, Ordering::Relaxed);
                // 每 10 个目录报告一次进度
                if count % 10 == 0 {
                    progress_callback(ScanProgress::new(
                        count,
                        files_found.load(Ordering::Relaxed),
                    ));
                }
            }
        })
        .filter(|e| e.file_type().is_file())
        .filter(|e| has_valid_extension(e.path(), extensions))
        .inspect(|_| {
            files_found.fetch_add(1, Ordering::Relaxed);
        })
        .map(|e| {
            let path = e.path().to_path_buf();
            let (constraints, _) = parse_time_constraints_from_path(&path, dir);
            IntermediateResult { path, constraints }
        })
        .collect();

    if entries.is_empty() {
        progress_callback(ScanProgress::completed(
            dirs_scanned.load(Ordering::Relaxed),
            0,
        ));
        return Err(WallpaperError::NoWallpapers(dir.to_path_buf()));
    }

    // 收集时间点
    let mut all_time_points = BTreeSet::new();
    let wallpapers: Vec<ScannedWallpaper> = entries
        .into_iter()
        .map(|result| {
            for constraint in &result.constraints {
                for point in constraint.key_points() {
                    all_time_points.insert(point);
                }
            }
            ScannedWallpaper {
                path: result.path,
                time_constraints: result.constraints,
            }
        })
        .collect();

    let final_dirs = dirs_scanned.load(Ordering::Relaxed);
    let final_files = wallpapers.len();
    progress_callback(ScanProgress::completed(final_dirs, final_files));

    Ok(ScanResult {
        wallpapers,
        time_points: all_time_points,
    })
}

// ==================== 异步流式版本 ====================

/// 异步并行扫描（流式返回）
///
/// 返回两个 channel：
/// - wallpaper_rx: 分批返回的壁纸列表
/// - progress_rx: 扫描进度
pub async fn scan_directory_streaming(
    dir: PathBuf,
    is_video: bool,
    config: ParallelScanConfig,
) -> (
    mpsc::Receiver<Vec<ScannedWallpaper>>,
    mpsc::Receiver<ScanProgress>,
) {
    let (wallpaper_tx, wallpaper_rx) = mpsc::channel(32);
    let (progress_tx, progress_rx) = mpsc::channel(16);

    tokio::task::spawn_blocking(move || {
        scan_streaming_impl(dir, is_video, config, wallpaper_tx, progress_tx);
    });

    (wallpaper_rx, progress_rx)
}

/// 流式扫描实现（在 spawn_blocking 中运行）
fn scan_streaming_impl(
    dir: PathBuf,
    is_video: bool,
    config: ParallelScanConfig,
    wallpaper_tx: mpsc::Sender<Vec<ScannedWallpaper>>,
    progress_tx: mpsc::Sender<ScanProgress>,
) {
    let extensions = if is_video {
        VIDEO_EXTENSIONS
    } else {
        IMAGE_EXTENSIONS
    };

    let mut batch = Vec::with_capacity(config.batch_size);
    let mut dirs_scanned = 0usize;
    let mut files_found = 0usize;

    let walker = WalkDir::new(&dir)
        .follow_links(config.follow_links)
        .into_iter()
        .filter_entry(|e| !is_hidden(e));

    for entry in walker {
        match entry {
            Ok(entry) => {
                if entry.file_type().is_dir() {
                    dirs_scanned += 1;
                    // 每 10 个目录发送一次进度
                    if dirs_scanned % 10 == 0 {
                        let _ = progress_tx.blocking_send(ScanProgress::new(dirs_scanned, files_found));
                    }
                    continue;
                }

                let path = entry.path();
                if !has_valid_extension(path, extensions) {
                    continue;
                }

                let (constraints, _) = parse_time_constraints_from_path(path, &dir);
                batch.push(ScannedWallpaper {
                    path: path.to_path_buf(),
                    time_constraints: constraints,
                });
                files_found += 1;

                // 批次满了就发送
                if batch.len() >= config.batch_size {
                    let _ = wallpaper_tx.blocking_send(std::mem::take(&mut batch));
                    batch = Vec::with_capacity(config.batch_size);
                }
            }
            Err(e) => {
                // 记录错误但继续扫描
                eprintln!("[lianwall] 扫描警告: {}", e);
            }
        }
    }

    // 发送剩余的批次
    if !batch.is_empty() {
        let _ = wallpaper_tx.blocking_send(batch);
    }

    // 发送完成进度
    let _ = progress_tx.blocking_send(ScanProgress::completed(dirs_scanned, files_found));
}

/// 异步扫描并收集所有结果
///
/// 这是 scan_directory_streaming 的便捷封装
pub async fn scan_directory_async(
    dir: PathBuf,
    is_video: bool,
) -> Result<ScanResult, WallpaperError> {
    let dir_clone = dir.clone();

    let result: Result<ScanResult, WallpaperError> = tokio::task::spawn_blocking(move || scan_with_rayon(&dir_clone, is_video))
        .await
        .map_err(|e: tokio::task::JoinError| WallpaperError::Io {
            operation: "spawn_blocking".to_string(),
            path: dir.clone(),
            source: std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
        })?;

    result
}

/// 异步扫描并收集时间点
///
/// 在流式扫描完成后，从所有壁纸中收集时间点
pub fn collect_time_points(wallpapers: &[ScannedWallpaper]) -> BTreeSet<TimePoint> {
    let mut time_points = BTreeSet::new();
    for wallpaper in wallpapers {
        for constraint in &wallpaper.time_constraints {
            for point in constraint.key_points() {
                time_points.insert(point);
            }
        }
    }
    time_points
}

/// 根据当前时间过滤活跃壁纸
pub fn filter_active(wallpapers: &[ScannedWallpaper]) -> Vec<&ScannedWallpaper> {
    let now = super::time_range::TimePoint::now();
    wallpapers
        .iter()
        .filter(|w| {
            if w.time_constraints.is_empty() {
                true // 无时间约束，始终可用
            } else {
                // 任一时间段包含当前时间即可
                w.time_constraints.iter().any(|range| range.is_active(&now))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        let dir = TempDir::new().unwrap();
        
        // 创建一些测试文件
        fs::write(dir.path().join("test1.jpg"), "").unwrap();
        fs::write(dir.path().join("test2.png"), "").unwrap();
        
        // 创建时间目录（格式: HH-HH 表示时间范围）
        let time_dir = dir.path().join("08-18");  // 08:00 - 18:00
        fs::create_dir(&time_dir).unwrap();
        fs::write(time_dir.join("morning.jpg"), "").unwrap();
        
        dir
    }

    #[test]
    fn test_scan_with_rayon() {
        let dir = create_test_dir();
        let result = scan_with_rayon(dir.path(), false);
        
        // 即使在 /tmp 下扫描，也应该能找到文件
        assert!(result.is_ok(), "扫描应该成功: {:?}", result.err());
        let scan_result = result.unwrap();
        assert_eq!(scan_result.wallpapers.len(), 3, "应该找到 3 个壁纸");
    }

    #[test]
    fn test_scan_with_rayon_progress() {
        let dir = create_test_dir();
        let mut progress_count = 0;
        
        let result = scan_with_rayon_progress(dir.path(), false, |_| {
            progress_count += 1;
        });
        
        assert!(result.is_ok(), "扫描应该成功: {:?}", result.err());
        // 进度回调至少会在完成时调用一次
        assert!(progress_count >= 1, "进度回调至少应该调用一次");
    }

    #[test]
    fn test_has_valid_extension() {
        assert!(has_valid_extension(Path::new("test.jpg"), IMAGE_EXTENSIONS));
        assert!(has_valid_extension(Path::new("test.PNG"), IMAGE_EXTENSIONS));
        assert!(!has_valid_extension(Path::new("test.txt"), IMAGE_EXTENSIONS));
        assert!(has_valid_extension(Path::new("test.mp4"), VIDEO_EXTENSIONS));
    }

    #[test]
    fn test_is_hidden() {
        // 创建一个 walkdir entry 来测试 is_hidden 函数
        let dir = TempDir::new().unwrap();
        let hidden_file = dir.path().join(".hidden_file");
        fs::write(&hidden_file, "").unwrap();
        let normal_file = dir.path().join("normal_file");
        fs::write(&normal_file, "").unwrap();
        
        let entries: Vec<_> = WalkDir::new(dir.path())
            .into_iter()
            .filter_map(|e| e.ok())
            .collect();
        
        // 验证 is_hidden 函数正确识别隐藏文件
        for entry in &entries {
            let name = entry.file_name().to_string_lossy();
            // 只有深度 > 0 且以 . 开头的才是隐藏文件
            if entry.depth() > 0 && name.starts_with('.') {
                assert!(is_hidden(entry), "{} 应该被识别为隐藏文件", name);
            } else if entry.depth() > 0 && !name.starts_with('.') {
                assert!(!is_hidden(entry), "{} 不应该被识别为隐藏文件", name);
            }
        }
    }

    #[tokio::test]
    async fn test_scan_directory_async() {
        let dir = create_test_dir();
        let result = scan_directory_async(dir.path().to_path_buf(), false).await;
        
        assert!(result.is_ok(), "异步扫描应该成功: {:?}", result.err());
        let scan_result = result.unwrap();
        assert_eq!(scan_result.wallpapers.len(), 3, "应该找到 3 个壁纸");
    }

    #[test]
    fn test_parse_time_constraints_from_path() {
        // 使用实际的文件系统路径进行测试
        // 时间目录格式: HH-HH 或 HHMM-HHMM（用 - 分隔时间）
        let dir = TempDir::new().unwrap();
        let time_dir = dir.path().join("08-18");  // 08:00 - 18:00
        fs::create_dir(&time_dir).unwrap();
        let test_file = time_dir.join("test.jpg");
        fs::write(&test_file, "").unwrap();
        
        let (constraints, time_points) = parse_time_constraints_from_path(&test_file, dir.path());
        
        assert_eq!(constraints.len(), 1, "应该有 1 个时间约束");
        assert!(!time_points.is_empty(), "应该有时间点");
    }

    #[test]
    fn test_parallel_scan_config_default() {
        let config = ParallelScanConfig::default();
        assert!(config.parallelism >= 1);
        assert_eq!(config.batch_size, 100);
        assert_eq!(config.timeout_secs, 30);
    }
}
