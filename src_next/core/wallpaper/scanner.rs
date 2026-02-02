use std::fs;
use std::path::{Path, PathBuf};

use crate::core::wallpaper::error::WallpaperError;
use crate::core::wallpaper::r#struct::{WallpaperScanInput, WallpaperScanOutput};
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
}

/// 递归扫描目录
fn recursive_scan(dir: &Path, ctx: &mut ScanContext) -> Result<Vec<PathBuf>, WallpaperError> {
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
                result.push(path.clone());
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
                        result.extend(recursive_scan(&path, ctx)?);
                    }
                    Some(_) => {
                        // 是时间段但不匹配，跳过整个目录
                    }
                    None => {
                        // 普通目录，正常递归
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
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // UTC 时间转本地时间（简化处理，实际可能需要 chrono）
    let total_minutes = (now / 60) % 1440; // 一天的分钟数
    let hour = (total_minutes / 60) as u8;
    let minute = (total_minutes % 60) as u8;

    (hour, minute)
}
