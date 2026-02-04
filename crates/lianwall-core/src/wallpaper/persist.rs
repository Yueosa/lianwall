//! 持久化 CRUD 操作

use std::fs;
use std::path::PathBuf;

use super::error::WallpaperError;
use super::r#struct::WeightsFile;

/// 获取权重文件路径
pub fn weights_path() -> PathBuf {
    let cache_dir = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("~/.cache"));
    cache_dir.join("lianwall/weights.json")
}

/// 加载权重文件
///
/// 文件不存在时返回默认值
pub fn load_weights() -> Result<WeightsFile, WallpaperError> {
    let path = weights_path();

    if !path.exists() {
        return Ok(WeightsFile::default());
    }

    let content = fs::read_to_string(&path).map_err(|e| WallpaperError::Io {
        operation: "read".to_string(),
        path: path.clone(),
        source: e,
    })?;

    let file: WeightsFile =
        serde_json::from_str(&content).map_err(|e| WallpaperError::Parse {
            path: path.clone(),
            source: e,
        })?;

    Ok(file)
}

/// 保存权重文件
pub fn save_weights(file: &WeightsFile) -> Result<(), WallpaperError> {
    let path = weights_path();

    // 确保目录存在
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| WallpaperError::Io {
            operation: "create_dir".to_string(),
            path: parent.to_path_buf(),
            source: e,
        })?;
    }

    let content =
        serde_json::to_string_pretty(file).map_err(|e| WallpaperError::Serialize {
            path: path.clone(),
            source: e,
        })?;

    fs::write(&path, content).map_err(|e| WallpaperError::Io {
        operation: "write".to_string(),
        path: path.clone(),
        source: e,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weights_path() {
        let path = weights_path();
        assert!(path.to_string_lossy().contains("lianwall"));
        assert!(path.to_string_lossy().ends_with("weights.json"));
    }

    #[test]
    fn test_default_weights_file() {
        let file = WeightsFile::default();
        assert_eq!(file.version, 1);
        assert!(file.video.items.is_empty());
        assert!(file.image.items.is_empty());
    }
}
