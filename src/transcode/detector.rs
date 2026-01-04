use serde::Deserialize;
use std::process::Command;

/// Hyprland 显示器信息结构
#[derive(Deserialize, Debug)]
struct Monitor {
    width: u32,
    height: u32,
}

/// 检测所有显示器分辨率，返回最低分辨率
pub fn detect_screen_resolution() -> (u32, u32) {
    let output = match Command::new("hyprctl").args(&["monitors", "-j"]).output() {
        Ok(output) => output,
        Err(e) => {
            eprintln!("警告: 无法检测显示器分辨率 ({}), 使用默认 1920x1080", e);
            return (1920, 1080);
        }
    };

    if !output.status.success() {
        eprintln!("警告: hyprctl 执行失败, 使用默认分辨率 1920x1080");
        return (1920, 1080);
    }

    match serde_json::from_slice::<Vec<Monitor>>(&output.stdout) {
        Ok(monitors) => {
            if monitors.is_empty() {
                eprintln!("警告: 未检测到显示器, 使用默认分辨率 1920x1080");
                return (1920, 1080);
            }

            // 找到像素总数最小的分辨率
            let (width, height) = monitors
                .iter()
                .map(|m| (m.width, m.height))
                .min_by_key(|(w, h)| w * h)
                .unwrap_or((1920, 1080));

            println!("检测到显示器最低分辨率: {}x{}", width, height);
            (width, height)
        }
        Err(e) => {
            eprintln!("警告: 解析显示器信息失败 ({}), 使用默认分辨率 1920x1080", e);
            (1920, 1080)
        }
    }
}

/// 检测系统可用的硬件编码器
/// 优先级: h264_nvenc > h264_vaapi > libx264
pub fn detect_available_encoder() -> String {
    let output = match Command::new("ffmpeg")
        .args(&["-hide_banner", "-encoders"])
        .output()
    {
        Ok(output) => output,
        Err(_) => {
            eprintln!("错误: 未找到 ffmpeg，转码功能将被禁用");
            eprintln!("请安装 ffmpeg: sudo pacman -S ffmpeg  (Arch Linux)");
            return "none".to_string();
        }
    };

    let encoders_list = String::from_utf8_lossy(&output.stdout);

    // 按优先级检测
    if encoders_list.contains("h264_nvenc") {
        println!("检测到 NVIDIA 硬件编码器: h264_nvenc");
        return "h264_nvenc".to_string();
    }

    if encoders_list.contains("h264_vaapi") {
        println!("检测到 VA-API 硬件编码器: h264_vaapi");
        return "h264_vaapi".to_string();
    }

    if encoders_list.contains("libx264") {
        println!("使用 CPU 编码器: libx264");
        return "libx264".to_string();
    }

    eprintln!("警告: 未检测到任何 H.264 编码器");
    "none".to_string()
}
