use std::fs;
use std::path::{Path, PathBuf};

use chrono::Timelike;

use crate::core::wallpaper::error::WallpaperError;
use crate::core::wallpaper::r#struct::{ScannedWallpaper, WallpaperScanInput, WallpaperScanOutput};
use crate::core::wallpaper::time_range::{is_in_range, parse_time_range};

/// 扫描壁纸目录
pub fn scan(input: WallpaperScanInput) -> Result<WallpaperScanOutput, WallpaperError> {
    // 检查基础目录是否存在
    if !input.base_dir.exists() {
        return Err(WallpaperError::DirectoryNotFound {
            path: input.base_dir.clone(),
        });
    }

    if !input.base_dir.is_dir() {
        return Err(WallpaperError::Io {
            operation: "scan".to_string(),
            path: input.base_dir.clone(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "路径不是目录",
            ),
        });
    }

    // 获取当前时间
    let now = get_current_time();

    // 递归扫描
    let mut context = ScanContext {
        extensions: &input.extensions,
        use_time_ranges: input.use_time_ranges,
        current_time: now,
        matched_ranges: Vec::new(),
        total_scanned: 0,
        from_root: 0,
        is_root: true,
        current_time_range: None,
    };

    let wallpapers = recursive_scan(&input.base_dir, &mut context)?;

    if wallpapers.is_empty() {
        return Err(WallpaperError::NoWallpapersFound {
            path: input.base_dir,
        });
    }

    Ok(WallpaperScanOutput {
        wallpapers,
        matched_ranges: context.matched_ranges,
        total_scanned: context.total_scanned,
        from_root: context.from_root,
    })
}

/// 扫描上下文
struct ScanContext<'a> {
    extensions: &'a [String],
    use_time_ranges: bool,
    current_time: (u8, u8),
    matched_ranges: Vec<String>,
    total_scanned: usize,
    from_root: usize,
    is_root: bool,
    /// 当前所在的时间段目录名（用于标记壁纸归属）
    current_time_range: Option<String>,
}

/// 递归扫描目录
fn recursive_scan(dir: &Path, ctx: &mut ScanContext) -> Result<Vec<ScannedWallpaper>, WallpaperError> {
    let mut result = Vec::new();
    let is_root = ctx.is_root;
    ctx.is_root = false;

    let entries = fs::read_dir(dir).map_err(|e| WallpaperError::ScanFailed {
        path: dir.to_path_buf(),
        source: e,
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| WallpaperError::ScanFailed {
            path: dir.to_path_buf(),
            source: e,
        })?;

        let path = entry.path();
        ctx.total_scanned += 1;

        if path.is_file() {
            // 检查文件扩展名
            if has_valid_extension(&path, ctx.extensions) {
                result.push(ScannedWallpaper {
                    path: path.clone(),
                    time_range: ctx.current_time_range.clone(),
                });
                if is_root {
                    ctx.from_root += 1;
                }
            }
        } else if path.is_dir() {
            let dir_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // 尝试解析为时间段
            if ctx.use_time_ranges {
                match parse_time_range(dir_name) {
                    Some(range) if is_in_range(&range, ctx.current_time) => {
                        // 匹配时间段，递归进入
                        ctx.matched_ranges.push(dir_name.to_string());
                        let prev_range = ctx.current_time_range.clone();
                        ctx.current_time_range = Some(dir_name.to_string());
                        result.extend(recursive_scan(&path, ctx)?);
                        ctx.current_time_range = prev_range;
                    }
                    Some(_) => {
                        // 是时间段但不匹配，跳过整个目录
                    }
                    None => {
                        // 普通目录，正常递归（保持当前时间段标记）
                        result.extend(recursive_scan(&path, ctx)?);
                    }
                }
            } else {
                // 不启用时间段匹配，所有目录都正常递归
                result.extend(recursive_scan(&path, ctx)?);
            }
        }
    }

    Ok(result)
}

/// 检查文件是否有有效扩展名
fn has_valid_extension(path: &Path, extensions: &[String]) -> bool {
    if let Some(ext) = path.extension() {
        let ext_lower = ext.to_string_lossy().to_lowercase();
        extensions.iter().any(|e| e == &ext_lower)
    } else {
        false
    }
}

/// 获取当前时间（小时, 分钟）
fn get_current_time() -> (u8, u8) {
    let now = chrono::Local::now();
    (now.hour() as u8, now.minute() as u8)
}

/// 扫描目录下所有时间段子目录（不管当前时间是否匹配）
///
/// 用于 GUI 展示时间树结构
pub fn scan_all_time_ranges(input: WallpaperScanInput) -> Result<Vec<crate::core::wallpaper::r#struct::TimeRangeInfo>, WallpaperError> {
    use crate::core::wallpaper::time_range::format_time_range;

    // 检查基础目录是否存在
    if !input.base_dir.exists() {
        return Err(WallpaperError::DirectoryNotFound {
            path: input.base_dir.clone(),
        });
    }

    let current_time = get_current_time();
    let mut result = Vec::new();

    // 只扫描第一层目录
    let entries = fs::read_dir(&input.base_dir).map_err(|e| WallpaperError::ScanFailed {
        path: input.base_dir.clone(),
        source: e,
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| WallpaperError::ScanFailed {
            path: input.base_dir.clone(),
            source: e,
        })?;

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };

        // 尝试解析为时间段
        if let Some(range) = parse_time_range(dir_name) {
            // 递归计数该目录下的壁纸数量
            let count = count_wallpapers_in_dir(&path, &input.extensions);
            let (start_time, end_time) = format_time_range(&range);
            let is_active = is_in_range(&range, current_time);

            result.push(crate::core::wallpaper::r#struct::TimeRangeInfo {
                name: dir_name.to_string(),
                start_time,
                end_time,
                wallpaper_count: count,
                is_active,
            });
        }
    }

    // 按开始时间排序
    result.sort_by_key(|r| {
        let parts: Vec<&str> = r.start_time.split(':').collect();
        if parts.len() == 2 {
            parts[0].parse::<u16>().unwrap_or(0) * 60 + parts[1].parse::<u16>().unwrap_or(0)
        } else {
            0
        }
    });

    Ok(result)
}

/// 递归计算目录下符合扩展名的文件数量
fn count_wallpapers_in_dir(dir: &Path, extensions: &[String]) -> usize {
    let mut count = 0;

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && has_valid_extension(&path, extensions) {
                count += 1;
            } else if path.is_dir() {
                count += count_wallpapers_in_dir(&path, extensions);
            }
        }
    }

    count
}
