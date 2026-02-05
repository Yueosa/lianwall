//! 目录扫描（支持时间段目录）

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::error::WallpaperError;
use super::time_range::{parse_time_dir, TimePoint, TimeRange};

/// mpv 支持的视频文件扩展名
///
/// 参考: https://github.com/mpv-player/mpv/blob/master/etc/mplayer-input.conf
/// mpv 基于 FFmpeg，支持几乎所有常见视频格式
const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "webm", "avi", "mov", "flv", "wmv", "m4v", "3gp", "ogv", "ts", "m2ts",
];

/// swww 支持的图片文件扩展名
///
/// 参考: https://github.com/LGFae/swww
/// swww 使用 image crate，支持以下格式
const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff", "tif", "pnm", "tga", "farbfeld",
];

/// 扫描到的壁纸（带时间约束）
#[derive(Debug, Clone)]
pub struct ScannedWallpaper {
    /// 文件路径
    pub path: PathBuf,
    /// 时间约束（从嵌套目录继承，所有约束必须同时满足）
    pub time_constraints: Vec<TimeRange>,
}

impl ScannedWallpaper {
    /// 判断在给定时间是否活跃
    pub fn is_active(&self, time: &TimePoint) -> bool {
        // 无约束 = 全天可用
        if self.time_constraints.is_empty() {
            return true;
        }
        // 所有约束都必须满足
        self.time_constraints.iter().all(|r| r.is_active(time))
    }
}

/// 扫描结果
#[derive(Debug, Clone)]
pub struct ScanResult {
    /// 所有壁纸（带时间约束）
    pub wallpapers: Vec<ScannedWallpaper>,
    /// 关键时间点（用于调度）
    pub time_points: BTreeSet<TimePoint>,
}

/// 递归扫描目录获取壁纸
///
/// # Arguments
/// * `dir` - 根目录路径
/// * `is_video` - true 扫描视频，false 扫描图片
///
/// # Returns
/// 扫描结果（壁纸列表 + 关键时间点）
pub fn scan_directory(dir: &PathBuf, is_video: bool) -> Result<ScanResult, WallpaperError> {
    if !dir.exists() {
        return Err(WallpaperError::DirectoryNotFound(dir.clone()));
    }

    let extensions = if is_video {
        VIDEO_EXTENSIONS
    } else {
        IMAGE_EXTENSIONS
    };

    let mut wallpapers = Vec::new();
    let mut time_points = BTreeSet::new();

    // 递归扫描
    scan_recursive(dir, extensions, &[], &mut wallpapers, &mut time_points)?;

    // 按路径排序（确保一致性）
    wallpapers.sort_by(|a, b| a.path.cmp(&b.path));

    if wallpapers.is_empty() {
        return Err(WallpaperError::NoWallpapers(dir.clone()));
    }

    Ok(ScanResult {
        wallpapers,
        time_points,
    })
}

/// 递归扫描目录
fn scan_recursive(
    dir: &PathBuf,
    extensions: &[&str],
    inherited_constraints: &[TimeRange],
    wallpapers: &mut Vec<ScannedWallpaper>,
    time_points: &mut BTreeSet<TimePoint>,
) -> Result<(), WallpaperError> {
    let entries = std::fs::read_dir(dir).map_err(|e| WallpaperError::Io {
        operation: "read_dir".to_string(),
        path: dir.clone(),
        source: e,
    })?;

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();

        if path.is_file() {
            // 检查文件扩展名
            let is_valid = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| extensions.contains(&ext.to_lowercase().as_str()))
                .unwrap_or(false);

            if is_valid {
                wallpapers.push(ScannedWallpaper {
                    path,
                    time_constraints: inherited_constraints.to_vec(),
                });
            }
        } else if path.is_dir() {
            // 尝试解析目录名为时间范围
            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            let mut constraints = inherited_constraints.to_vec();

            if let Some(range) = parse_time_dir(dir_name) {
                // 记录关键时间点
                for point in range.key_points() {
                    time_points.insert(point);
                }
                // 添加时间约束
                constraints.push(range);
            }

            // 递归扫描子目录
            scan_recursive(&path, extensions, &constraints, wallpapers, time_points)?;
        }
    }

    Ok(())
}

/// 过滤活跃壁纸
pub fn filter_active(wallpapers: &[ScannedWallpaper], time: &TimePoint) -> Vec<PathBuf> {
    wallpapers
        .iter()
        .filter(|w| w.is_active(time))
        .map(|w| w.path.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extensions() {
        assert!(VIDEO_EXTENSIONS.contains(&"mp4"));
        assert!(IMAGE_EXTENSIONS.contains(&"jpg"));
        assert!(!VIDEO_EXTENSIONS.contains(&"jpg"));
    }

    #[test]
    fn test_scanned_wallpaper_is_active() {
        // 无约束
        let w = ScannedWallpaper {
            path: PathBuf::from("/test.mp4"),
            time_constraints: vec![],
        };
        assert!(w.is_active(&TimePoint::new(12, 0)));

        // 单个约束
        let w = ScannedWallpaper {
            path: PathBuf::from("/test.mp4"),
            time_constraints: vec![TimeRange {
                start: TimePoint::new(8, 0),
                end: TimePoint::new(18, 0),
            }],
        };
        assert!(w.is_active(&TimePoint::new(12, 0)));
        assert!(!w.is_active(&TimePoint::new(20, 0)));

        // 嵌套约束（必须全部满足）
        let w = ScannedWallpaper {
            path: PathBuf::from("/test.mp4"),
            time_constraints: vec![
                TimeRange {
                    start: TimePoint::new(8, 0),
                    end: TimePoint::new(18, 0),
                },
                TimeRange {
                    start: TimePoint::new(12, 0),
                    end: TimePoint::new(13, 30),
                },
            ],
        };
        assert!(w.is_active(&TimePoint::new(12, 30)));
        assert!(!w.is_active(&TimePoint::new(10, 0))); // 不在第二个约束内
        assert!(!w.is_active(&TimePoint::new(20, 0))); // 不在第一个约束内
    }
}
