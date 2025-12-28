#![allow(dead_code)]

use super::PaperEngine;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

/// swww 支持的过渡效果
#[derive(Debug, Clone)]
pub enum TransitionType {
    None,
    Simple,
    Fade,
    Left,
    Right,
    Top,
    Bottom,
    Wipe,
    Wave,
    Grow,
    Center,
    Any,
    Outer,
    Random,
}

impl TransitionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TransitionType::None => "none",
            TransitionType::Simple => "simple",
            TransitionType::Fade => "fade",
            TransitionType::Left => "left",
            TransitionType::Right => "right",
            TransitionType::Top => "top",
            TransitionType::Bottom => "bottom",
            TransitionType::Wipe => "wipe",
            TransitionType::Wave => "wave",
            TransitionType::Grow => "grow",
            TransitionType::Center => "center",
            TransitionType::Any => "any",
            TransitionType::Outer => "outer",
            TransitionType::Random => "random",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "none" => TransitionType::None,
            "simple" => TransitionType::Simple,
            "fade" => TransitionType::Fade,
            "left" => TransitionType::Left,
            "right" => TransitionType::Right,
            "top" => TransitionType::Top,
            "bottom" => TransitionType::Bottom,
            "wipe" => TransitionType::Wipe,
            "wave" => TransitionType::Wave,
            "grow" => TransitionType::Grow,
            "center" => TransitionType::Center,
            "any" => TransitionType::Any,
            "outer" => TransitionType::Outer,
            "random" => TransitionType::Random,
            _ => TransitionType::Fade,
        }
    }
}

/// swww 静态壁纸引擎
/// 支持 jpg, jpeg, png, gif, pnm, tga, tiff, webp, bmp, farbfeld
pub struct Swww {
    /// 过渡效果类型
    pub transition_type: TransitionType,
    /// 过渡时长（秒）
    pub transition_duration: f32,
    /// 过渡帧率
    pub transition_fps: u32,
    /// 过渡步长（越小越平滑）
    pub transition_step: u8,
    /// 填充模式: fit, fill, center, stretch
    pub fill_mode: String,
}

impl Swww {
    pub fn new() -> Self {
        Self {
            transition_type: TransitionType::Fade,
            transition_duration: 2.0,
            transition_fps: 60,
            transition_step: 20,
            fill_mode: "fill".to_string(),
        }
    }

    pub fn with_transition(transition_type: &str, duration: f32) -> Self {
        Self {
            transition_type: TransitionType::from_str(transition_type),
            transition_duration: duration,
            ..Self::new()
        }
    }

    /// 确保 swww-daemon 正在运行
    fn ensure_daemon(&self) -> Result<(), String> {
        // 检查 daemon 是否已运行
        let check = Command::new("pgrep")
            .arg("-x")
            .arg("swww-daemon")
            .output();

        if let Ok(output) = check {
            if output.status.success() {
                return Ok(()); // daemon 已在运行
            }
        }

        // 启动 daemon
        let result = Command::new("swww-daemon")
            .spawn();

        match result {
            Ok(_) => {
                // 等待 daemon 启动
                thread::sleep(Duration::from_millis(500));
                Ok(())
            }
            Err(e) => Err(format!("启动 swww-daemon 失败: {}", e)),
        }
    }

    /// 支持的图片格式
    pub fn supported_extensions() -> &'static [&'static str] {
        &["jpg", "jpeg", "png", "gif", "pnm", "tga", "tiff", "tif", "webp", "bmp", "ff"]
    }
}

impl Default for Swww {
    fn default() -> Self {
        Self::new()
    }
}

impl PaperEngine for Swww {
    fn name(&self) -> &'static str {
        "swww"
    }

    fn set_wallpaper(&self, path: &Path) -> Result<(), String> {
        // 确保 daemon 运行
        self.ensure_daemon()?;

        // 设置壁纸
        let result = Command::new("swww")
            .arg("img")
            .arg(path)
            .args([
                "--transition-type", self.transition_type.as_str(),
                "--transition-duration", &self.transition_duration.to_string(),
                "--transition-fps", &self.transition_fps.to_string(),
                "--transition-step", &self.transition_step.to_string(),
                "--fill", &self.fill_mode,
            ])
            .status();

        match result {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(format!("swww 命令失败，退出码: {:?}", status.code())),
            Err(e) => Err(format!("执行 swww 失败: {}", e)),
        }
    }

    fn stop(&self) -> Result<(), String> {
        let result = Command::new("swww")
            .arg("kill")
            .status();

        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("停止 swww 失败: {}", e)),
        }
    }

    fn is_available(&self) -> bool {
        Command::new("which")
            .arg("swww")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}
