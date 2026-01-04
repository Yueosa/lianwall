pub mod cache;
pub mod config;
pub mod detector;
pub mod encoder;
pub mod preloader;

use std::path::{Path, PathBuf};

pub use cache::{cleanup_cache, get_cache_path, is_cache_valid};
pub use config::TranscodeConfig;
pub use encoder::transcode_video;
pub use preloader::PreloadQueue;

/// 获取或转码视频文件
///
/// 逻辑：
/// 1. 计算源文件的缓存路径
/// 2. 如果缓存存在且有效，直接返回缓存路径
/// 3. 如果缓存不存在或无效，执行同步转码
/// 4. 清理缓存（如果超出大小限制）
pub fn get_or_transcode_video(
    source_path: &Path,
    config: &TranscodeConfig,
) -> Result<PathBuf, String> {
    let cache_path = get_cache_path(source_path, config)?;

    // 检查缓存是否有效
    if is_cache_valid(&cache_path, source_path)? {
        return Ok(cache_path);
    }

    // 缓存无效，执行转码
    transcode_video(source_path, &cache_path, config)?;

    // 清理缓存（如果需要）
    cleanup_cache(&config.cache_dir, config.max_cache_size_mb)?;

    Ok(cache_path)
}
