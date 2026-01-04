use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::config::TranscodeConfig;

/// 计算文件的 SHA256 hash（仅前 1MB，快速检测）
pub fn calculate_file_hash(path: &Path) -> Result<String, std::io::Error> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();

    // 仅读取前 1MB 用于快速 hash
    let mut buffer = vec![0; 1024 * 1024]; // 1MB
    let bytes_read = reader.read(&mut buffer)?;

    hasher.update(&buffer[..bytes_read]);
    let result = hasher.finalize();

    // 返回前 16 位 hex 编码
    Ok(format!("{:x}", result)[..16].to_string())
}

/// 根据原始文件和转码配置生成缓存路径
/// 命名格式: {原文件名}_{宽度}x{高度}@{fps}fps_{hash前8位}.mp4
pub fn get_cache_path(original: &Path, config: &TranscodeConfig) -> Result<PathBuf, String> {
    let hash = calculate_file_hash(original).map_err(|e| e.to_string())?;
    let target_spec = format!(
        "{}x{}@{}fps",
        config.target_width, config.target_height, config.target_fps
    );

    let original_stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let filename = format!(
        "{}_{}_{}_.mp4",
        original_stem,
        target_spec,
        &hash[..8.min(hash.len())]
    );

    Ok(config.cache_dir.join(filename))
}

/// 检查缓存文件是否存在且有效
pub fn is_cache_valid(cached: &Path, original: &Path) -> Result<bool, String> {
    // 检查缓存文件是否存在
    if !cached.exists() {
        return Ok(false);
    }

    // 提取缓存文件名中的 hash（格式：{name}_{spec}_{hash8}.mp4）
    let cache_filename = cached
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("无效的缓存文件名")?;

    let parts: Vec<&str> = cache_filename.rsplitn(2, '_').collect();
    if parts.len() != 2 {
        return Ok(false); // 文件名格式不正确
    }

    let expected_hash_prefix = parts[0]; // hash 的前8位

    // 计算原始文件的当前 hash
    let current_hash = calculate_file_hash(original).map_err(|e| e.to_string())?;

    // 比较 hash 前缀
    Ok(current_hash.starts_with(expected_hash_prefix))
}

/// 获取缓存目录总大小（MB）
pub fn get_cache_size(cache_dir: &Path) -> u64 {
    if !cache_dir.exists() {
        return 0;
    }

    let mut total_size = 0u64;

    if let Ok(entries) = fs::read_dir(cache_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    total_size += metadata.len();
                }
            }
        }
    }

    // 转换为 MB
    total_size / (1024 * 1024)
}

/// 清理缓存文件，直到总大小 < max_size
/// **策略**: 删除最近使用（访问时间最新）的文件
/// 原因: 基于零和博弈算法，刚播放过的壁纸权重低，短期不会再出现
pub fn cleanup_cache(cache_dir: &Path, max_size_mb: u64) -> Result<(), String> {
    if max_size_mb == 0 {
        return Ok(()); // 无限制
    }

    let current_size = get_cache_size(cache_dir);
    if current_size <= max_size_mb {
        return Ok(()); // 未超限
    }

    println!(
        "缓存大小 {} MB 超过限制 {} MB，开始清理...",
        current_size, max_size_mb
    );

    // 收集所有缓存文件及其访问时间
    let mut files: Vec<(PathBuf, SystemTime)> = Vec::new();

    if let Ok(entries) = fs::read_dir(cache_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("mp4") {
                if let Ok(metadata) = fs::metadata(&path) {
                    if let Ok(accessed) = metadata.accessed() {
                        files.push((path, accessed));
                    }
                }
            }
        }
    }

    // 按访问时间降序排序（最新的在前）
    files.sort_by(|a, b| b.1.cmp(&a.1));

    // 删除最近访问的文件，直到满足大小限制
    let mut removed_size = 0u64;
    let target_remove = (current_size - max_size_mb) * 1024 * 1024; // 转换回字节

    for (path, _) in files {
        if removed_size >= target_remove {
            break;
        }

        if let Ok(metadata) = fs::metadata(&path) {
            let file_size = metadata.len();

            if fs::remove_file(&path).is_ok() {
                println!("  删除缓存: {}", path.display());
                removed_size += file_size;
            }
        }
    }

    println!(
        "缓存清理完成，释放 {} MB 空间",
        removed_size / (1024 * 1024)
    );

    Ok(())
}
