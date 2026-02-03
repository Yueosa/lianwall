use std::path::Path;
use std::process::Command;

/// 检查命令是否可用
pub fn is_command_available(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 获取活动的显示器列表
/// 优先使用 hyprctl，失败则回退到通配符 "*"
pub fn get_active_monitors() -> Vec<String> {
    // 检测 hyprctl 是否可用
    if !is_command_available("hyprctl") {
        return vec!["*".to_string()];
    }

    // 尝试解析 hyprctl monitors -j
    match parse_hyprctl_monitors() {
        Some(monitors) if !monitors.is_empty() => monitors,
        _ => vec!["*".to_string()],
    }
}

/// 解析 hyprctl monitors -j 输出
fn parse_hyprctl_monitors() -> Option<Vec<String>> {
    let output = Command::new("hyprctl")
        .args(["monitors", "-j"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let monitors: serde_json::Value = serde_json::from_str(&stdout).ok()?;

    let names: Vec<String> = monitors
        .as_array()?
        .iter()
        .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
        .collect();

    if names.is_empty() {
        None
    } else {
        Some(names)
    }
}

/// 停止进程（通过 pkill）
pub fn pkill(process_name: &str) -> bool {
    Command::new("pkill")
        .arg(process_name)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 检查进程是否在运行（通过 pgrep）
pub fn is_process_running(process_name: &str) -> bool {
    Command::new("pgrep")
        .arg("-x")
        .arg(process_name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 验证壁纸文件是否存在且为文件
pub fn validate_wallpaper(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("壁纸文件不存在: {}", path.display()));
    }
    if !path.is_file() {
        return Err(format!("壁纸路径不是文件: {}", path.display()));
    }
    Ok(())
}
