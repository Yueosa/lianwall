//! 目录扫描

use std::path::PathBuf;

use super::error::WallpaperError;

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

/// 扫描目录获取壁纸路径列表
///
/// # Arguments
/// * `dir` - 目录路径
/// * `is_video` - true 扫描视频，false 扫描图片
///
/// # Returns
/// 排序后的文件路径列表
pub fn scan_directory(dir: &PathBuf, is_video: bool) -> Result<Vec<PathBuf>, WallpaperError> {
    if !dir.exists() {
        return Err(WallpaperError::DirectoryNotFound(dir.clone()));
    }

    let extensions = if is_video {
        VIDEO_EXTENSIONS
    } else {
        IMAGE_EXTENSIONS
    };

    let entries = std::fs::read_dir(dir).map_err(|e| WallpaperError::Io {
        operation: "read_dir".to_string(),
        path: dir.clone(),
        source: e,
    })?;

    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| extensions.contains(&ext.to_lowercase().as_str()))
                    .unwrap_or(false)
        })
        .collect();

    // 按文件名排序（确保一致性）
    paths.sort();

    if paths.is_empty() {
        return Err(WallpaperError::NoWallpapers(dir.clone()));
    }

    Ok(paths)
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
}
