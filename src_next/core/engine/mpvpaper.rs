use std::process::Command;

use crate::core::engine::error::EngineError;
use crate::core::engine::r#struct::{
    EngineDetectInput, EngineDetectOutput, EngineSetInput, EngineSetOutput, EngineStopInput,
    EngineStopOutput, EngineType,
};
use crate::core::engine::utils::{
    get_active_monitors, is_command_available, pkill, validate_wallpaper,
};

/// 检测 mpvpaper 是否可用
pub fn detect(_input: EngineDetectInput) -> Result<EngineDetectOutput, EngineError> {
    if is_command_available("mpvpaper") {
        Ok(EngineDetectOutput {})
    } else {
        Err(EngineError::Unavailable {
            engine: EngineType::MpvPaper,
            reason: "mpvpaper 未安装或不在 PATH 中".to_string(),
        })
    }
}

/// 设置动态壁纸（mpvpaper）
pub fn set(input: EngineSetInput) -> Result<EngineSetOutput, EngineError> {
    // 1. 验证壁纸文件
    validate_wallpaper(&input.wallpaper_path).map_err(|reason| {
        EngineError::InvalidWallpaper {
            path: input.wallpaper_path.clone(),
            reason,
        }
    })?;

    // 2. 先停止所有相关进程（避免内存泄漏）
    stop(EngineStopInput {
        engine_type: EngineType::MpvPaper,
    })?;
    super::swww::stop(EngineStopInput {
        engine_type: EngineType::Swww,
    })?;

    // 3. 确定显示器输出列表
    let outputs = if input.outputs.is_empty() {
        get_active_monitors()
    } else {
        input.outputs
    };

    // 4. 构建 mpv 参数字符串
    let mpv_options = input.args.join(" ");

    // 5. 为每个显示器启动 mpvpaper
    // 注意：mpvpaper -p 参数是硬编码的（窗口遮挡时暂停）
    for output in &outputs {
        Command::new("mpvpaper")
            .arg("-p") // 硬编码：遮挡暂停
            .arg("-o")
            .arg(&mpv_options)
            .arg(output)
            .arg(&input.wallpaper_path)
            .spawn()
            .map_err(|e| EngineError::StartFailed {
                engine: EngineType::MpvPaper,
                source: e,
            })?;
    }

    Ok(EngineSetOutput { pid: None })
}

/// 停止 mpvpaper
pub fn stop(_input: EngineStopInput) -> Result<EngineStopOutput, EngineError> {
    pkill("mpvpaper");
    Ok(EngineStopOutput {})
}
