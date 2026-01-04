use super::config::TranscodeConfig;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};

/// 执行转码任务（阻塞直到完成）
pub fn transcode_video(
    input: &Path,
    output: &Path,
    config: &TranscodeConfig,
) -> Result<(), String> {
    // 检查输入文件是否存在
    if !input.exists() {
        return Err(format!("输入文件不存在: {}", input.display()));
    }

    // 创建输出目录
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("无法创建输出目录: {}", e))?;
    }

    // 检测原始视频分辨率
    let (orig_width, orig_height) = detect_video_resolution(input)?;

    // 判断是否需要转码
    let needs_transcode = orig_width > config.target_width || orig_height > config.target_height;

    if !needs_transcode && config.target_fps == 0 {
        println!(
            "  原始分辨率 {}x{} 已满足要求，跳过转码",
            orig_width, orig_height
        );
        // 直接复制文件
        std::fs::copy(input, output).map_err(|e| format!("复制文件失败: {}", e))?;
        return Ok(());
    }

    println!(
        "开始转码: {} ({:?})",
        input.file_name().unwrap().to_string_lossy(),
        config.encoder
    );
    println!(
        "  原始: {}x{} → 目标: {}x{}@{}fps",
        orig_width, orig_height, config.target_width, config.target_height, config.target_fps
    );

    // 构建 FFmpeg 命令
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-i").arg(input);

    // 视频过滤器: 缩放 + 帧率
    let mut vf_filters = Vec::new();

    if orig_width > config.target_width || orig_height > config.target_height {
        vf_filters.push(format!(
            "scale={}:{}:flags=lanczos",
            config.target_width, config.target_height
        ));
    }

    if config.target_fps > 0 {
        vf_filters.push(format!("fps={}", config.target_fps));
    }

    if !vf_filters.is_empty() {
        cmd.arg("-vf").arg(vf_filters.join(","));
    }

    // 视频编码器
    cmd.arg("-c:v").arg(&config.encoder);

    // CRF 质量控制
    cmd.arg("-crf").arg(config.crf.to_string());

    // 编码预设
    cmd.arg("-preset").arg(&config.preset);

    // 丢弃音频流（加速转码）
    cmd.arg("-an");

    // 优化流式播放
    cmd.arg("-movflags").arg("+faststart");

    // 覆盖已存在文件
    cmd.arg("-y");

    // 输出文件
    cmd.arg(output);

    // 隐藏 ffmpeg banner
    cmd.arg("-hide_banner");
    cmd.arg("-loglevel").arg("error");

    // 执行转码
    let status = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| format!("FFmpeg 执行失败: {}", e))?;

    if !status.success() {
        // 删除不完整的输出文件
        let _ = std::fs::remove_file(output);
        return Err(format!("转码失败，退出码: {}", status.code().unwrap_or(-1)));
    }

    println!(
        "✅ 转码完成: {}",
        output.file_name().unwrap().to_string_lossy()
    );
    Ok(())
}

/// 后台异步转码，返回任务句柄
#[allow(dead_code)]
pub fn transcode_async(
    input: PathBuf,
    output: PathBuf,
    config: TranscodeConfig,
) -> JoinHandle<Result<(), String>> {
    thread::spawn(move || transcode_video(&input, &output, &config))
}

/// 检测视频原始分辨率
fn detect_video_resolution(input: &Path) -> Result<(u32, u32), String> {
    let output = Command::new("ffprobe")
        .args(&[
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
            input.to_str().unwrap(),
        ])
        .output()
        .map_err(|e| format!("ffprobe 执行失败: {}", e))?;

    if !output.status.success() {
        return Err("无法检测视频分辨率".to_string());
    }

    let resolution_str = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = resolution_str.trim().split(',').collect();

    if parts.len() != 2 {
        return Err(format!("无效的分辨率输出: {}", resolution_str));
    }

    let width: u32 = parts[0].parse().map_err(|_| "解析宽度失败")?;
    let height: u32 = parts[1].parse().map_err(|_| "解析高度失败")?;

    Ok((width, height))
}
