#![allow(dead_code)]

use super::PaperEngine;
use std::path::Path;
use std::process::Command;
use serde_json::Value;

/// mpvpaper 动态壁纸引擎
pub struct MpvPaper {
    pub fit_mode: String,
    pub panscan: f64,
}

impl MpvPaper {
    /// 获取主显示器分辨率 (width, height)
    fn get_monitor_resolution() -> (u32, u32) {
        if let Ok(output) = Command::new("hyprctl").args(["monitors", "-j"]).output() {
            if let Ok(json_str) = String::from_utf8(output.stdout) {
                if let Ok(monitors) = serde_json::from_str::<Value>(&json_str) {
                    if let Some(monitors_array) = monitors.as_array() {
                        for monitor in monitors_array {
                            if monitor["focused"].as_bool().unwrap_or(false) 
                                || monitors_array.len() == 1 {
                                if let (Some(w), Some(h)) = 
                                    (monitor["width"].as_u64(), monitor["height"].as_u64()) {
                                    return (w as u32, h as u32);
                                }
                            }
                        }
                        if let Some(first) = monitors_array.first() {
                            if let (Some(w), Some(h)) = 
                                (first["width"].as_u64(), first["height"].as_u64()) {
                                return (w as u32, h as u32);
                            }
                        }
                    }
                }
            }
        }
        // 默认 1920x1080
        (1920, 1080)
    }

    /// 用 ffprobe 获取视频分辨率
    fn get_video_resolution(path: &Path) -> Option<(u32, u32)> {
        let output = Command::new("ffprobe")
            .args([
                "-v", "error",
                "-select_streams", "v:0",
                "-show_entries", "stream=width,height",
                "-of", "json",
            ])
            .arg(path)
            .output()
            .ok()?;
        
        let json_str = String::from_utf8(output.stdout).ok()?;
        let json: Value = serde_json::from_str(&json_str).ok()?;
        
        let streams = json["streams"].as_array()?;
        let stream = streams.first()?;
        
        let width = stream["width"].as_u64()? as u32;
        let height = stream["height"].as_u64()? as u32;
        
        Some((width, height))
    }

    /// 根据视频分辨率和显示器宽高比，计算目标分辨率
    /// 视频太宽就按高度算宽度，视频太高就按宽度算高度
    fn calculate_target_resolution(video_width: u32, video_height: u32, monitor_width: u32, monitor_height: u32) -> (u32, u32) {
        let video_ratio = video_width as f64 / video_height as f64;
        let monitor_ratio = monitor_width as f64 / monitor_height as f64;
        
        if (video_ratio - monitor_ratio).abs() < 0.01 {
            // 宽高比已经接近目标
            return (video_width, video_height);
        }
        
        if video_ratio > monitor_ratio {
            // 视频太宽，以高度为基准计算新宽度
            let new_width = (video_height as f64 * monitor_ratio).round() as u32;
            let new_width = new_width / 2 * 2; // 偶数
            (new_width, video_height)
        } else {
            // 视频太高，以宽度为基准计算新高度
            let new_height = (video_width as f64 / monitor_ratio).round() as u32;
            let new_height = new_height / 2 * 2; // 偶数
            (video_width, new_height)
        }
    }

    pub fn new() -> Self {
        Self {
            fit_mode: "auto".to_string(),
            panscan: 1.0,
        }
    }

    pub fn with_fit_mode(fit_mode: &str, panscan: f64) -> Self {
        Self {
            fit_mode: fit_mode.to_string(),
            panscan,
        }
    }

    /// 根据视频路径动态生成 mpv 选项
    fn build_options(&self, path: &Path) -> String {
        match self.fit_mode.as_str() {
            "crop" => {
                let (mw, mh) = Self::get_monitor_resolution();
                
                if let Some((vw, vh)) = Self::get_video_resolution(path) {
                    let (tw, th) = Self::calculate_target_resolution(vw, vh, mw, mh);
                    // -o 参数格式：不需要 -- 前缀
                    format!("loop no-audio hwdec=no vf=scale={}:{}:force_original_aspect_ratio=increase,crop={}:{}", 
                        tw, th, tw, th)
                } else {
                    format!("loop no-audio hwdec=auto keepaspect=yes panscan={}", self.panscan)
                }
            }
            "contain" => "loop no-audio hwdec=auto keepaspect=yes panscan=0.0".to_string(),
            "cover" => format!("loop no-audio hwdec=auto keepaspect=yes panscan={}", self.panscan),
            "fill" => "loop no-audio hwdec=auto keepaspect=no".to_string(),
            _ => "loop no-audio hwdec=auto".to_string(),
        }
    }

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
        self.stop()?;

        let options = self.build_options(path);
        
        // 打印调试信息
        eprintln!("DEBUG: mpvpaper options = {}", options);

        // 使用 -o 参数，直接传递选项字符串
        let result = Command::new("mpvpaper")
            .arg("-o")
            .arg(&options)
            .arg("*")
            .arg(path)
            .spawn();

        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("启动 mpvpaper 失败: {}", e)),
        }
    }

    fn stop(&self) -> Result<(), String> {
        let result = Command::new("pkill").arg("mpvpaper").status();

        match result {
            Ok(_) => Ok(()),
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
