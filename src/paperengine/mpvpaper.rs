#![allow(dead_code)]

use super::PaperEngine;
use std::path::Path;
use std::process::Command;

/// mpvpaper 动态壁纸引擎
pub struct MpvPaper {
    /// mpvpaper 的额外参数
    pub options: String,
}

impl MpvPaper {
    pub fn new() -> Self {
        Self {
            options: "--loop --no-audio --hwdec=auto".to_string(),
        }
    }

    pub fn with_options(options: &str) -> Self {
        Self {
            options: options.to_string(),
        }
    }

    /// 支持的视频格式
    pub fn supported_extensions() -> &'static [&'static str] {
        &["mp4", "mkv", "webm", "avi", "mov", "flv", "wmv", "m4v", "gif"]
    }
}

impl Default for MpvPaper {
    fn default() -> Self {
        Self::new()
    }
}

impl PaperEngine for MpvPaper {
    fn name(&self) -> &'static str {
        "mpvpaper"
    }

    fn set_wallpaper(&self, path: &Path) -> Result<(), String> {
        // 先停止现有的 mpvpaper
        self.stop()?;

        // 启动新的 mpvpaper
        let result = Command::new("mpvpaper")
            .args(["-o", &self.options, "*"])
            .arg(path)
            .spawn();

        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("启动 mpvpaper 失败: {}", e)),
        }
    }

    fn stop(&self) -> Result<(), String> {
        let result = Command::new("pkill")
            .arg("mpvpaper")
            .status();

        match result {
            Ok(_) => Ok(()), // pkill 返回非零也没关系，可能没有进程在运行
            Err(e) => Err(format!("停止 mpvpaper 失败: {}", e)),
        }
    }

    fn is_available(&self) -> bool {
        Command::new("which")
            .arg("mpvpaper")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}
