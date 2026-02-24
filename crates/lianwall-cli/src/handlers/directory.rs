//! 目录操作命令处理器
//!
//! - `reload` - 重新加载配置文件并重新扫描壁纸目录
//! - `rescan` - 重新扫描壁纸目录

use std::io::{self, Write};
use std::time::{Duration, Instant};

use lianwall_core::config::WallMode;
use lianwall_core::socket::{Event, EventType};

use crate::client::ClientError;
use crate::output::{messages, Formatter};

use super::{connect, Result};

/// 单个模式的空间信息
struct ModeSpaceInfo {
    total: usize,
    available: usize,
    locked: usize,
}

/// 两个模式的空间信息收集器
struct SpaceCollector {
    video: Option<ModeSpaceInfo>,
    image: Option<ModeSpaceInfo>,
}

impl SpaceCollector {
    fn new() -> Self {
        Self { video: None, image: None }
    }

    /// 记录一个 SpaceUpdated 事件，返回是否两个模式都已收集
    fn record(&mut self, mode: &WallMode, total: usize, available: usize, locked: usize) -> bool {
        let info = ModeSpaceInfo { total, available, locked };
        match mode {
            WallMode::Video => self.video = Some(info),
            WallMode::Image => self.image = Some(info),
        }
        self.video.is_some() && self.image.is_some()
    }

    /// 汇总获取 (total, available, locked)
    fn totals(&self) -> (usize, usize, usize) {
        let v = self.video.as_ref().map(|i| (i.total, i.available, i.locked)).unwrap_or_default();
        let i = self.image.as_ref().map(|i| (i.total, i.available, i.locked)).unwrap_or_default();
        (v.0 + i.0, v.1 + i.1, v.2 + i.2)
    }

    /// 格式化详细输出（视频 X / 图片 Y）
    fn detail_string(&self, fmt: &Formatter) -> String {
        let v = self.video.as_ref().map(|i| i.total).unwrap_or(0);
        let i = self.image.as_ref().map(|i| i.total).unwrap_or(0);
        format!("{} {} / {} {}", fmt.icon_video(), v, fmt.icon_image(), i)
    }
}

/// 等待事件的超时时间（秒）
const WAIT_TIMEOUT_SECS: u64 = 30;
/// 显示提示信息的延迟（秒）
const HINT_DELAY_SECS: u64 = 10;

/// 处理 reload 命令
///
/// 重新加载配置文件并重新扫描壁纸目录。
///
/// # 与 rescan 的区别
/// - `reload`: 重新读取 config.toml 文件，更新 daemon 的所有配置状态，
///   如果配置中的壁纸目录路径发生变化，也会自动触发重新扫描。
/// - `rescan`: 只重新扫描壁纸目录发现新增/删除的文件，不读取配置文件。
pub fn handle_reload(fmt: &Formatter) -> Result<()> {
    let mut client = connect()?;

    // 先订阅事件
    client.subscribe(vec![EventType::ConfigChanged, EventType::SpaceUpdated], false)?;

    // 发送 reload 命令
    client.reload_config()?;

    if !fmt.is_json() {
        eprint!("{}...", messages::RELOADING);
        io::stderr().flush().unwrap();
    }

    // 等待 ConfigChanged 和两个 SpaceUpdated 事件（Video + Image）
    let mut config_changed = false;
    let mut collector = SpaceCollector::new();
    let mut space_done = false;

    let result = wait_for_events(&mut client, fmt, |event| {
        match event {
            Event::ConfigChanged { .. } => {
                config_changed = true;
            }
            Event::SpaceUpdated { mode, summary, .. } => {
                space_done = collector.record(mode, summary.total, summary.available, summary.locked);
            }
            _ => {}
        }
        // ConfigChanged + 两个模式的 SpaceUpdated 全部收到才算完成
        config_changed && space_done
    });

    // 清除 "Reloading..."
    if !fmt.is_json() {
        eprint!("\r\x1b[K");
    }

    match result {
        Ok(()) => {
            let (total, available, locked) = collector.totals();
            if fmt.is_json() {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "ok",
                        "total": total,
                        "available": available,
                        "locked": locked,
                        "video": collector.video.as_ref().map(|v| serde_json::json!({
                            "total": v.total,
                            "available": v.available,
                            "locked": v.locked
                        })),
                        "image": collector.image.as_ref().map(|v| serde_json::json!({
                            "total": v.total,
                            "available": v.available,
                            "locked": v.locked
                        }))
                    })
                );
            } else {
                fmt.print_success(&format!(
                    "Reloaded: {} wallpapers ({} available, {} locked) [{}]",
                    total, available, locked, collector.detail_string(fmt)
                ));
            }
        }
        Err(_) => {
            print_timeout_message(fmt);
        }
    }

    Ok(())
}

/// 处理 rescan 命令
///
/// 重新扫描壁纸目录，发现新增/删除的壁纸文件。
///
/// # 使用场景
/// - 在壁纸目录中添加或删除了壁纸文件
/// - 修改了壁纸文件的时间约束目录结构（如 `00-06/`）
///
/// # 与 reload 的区别
/// - `rescan`: 只重新扫描目录，不重新读取配置文件，适合壁纸文件变动的情况
/// - `reload`: 重新读取 config.toml，适合配置文件变动的情况
pub fn handle_rescan(fmt: &Formatter) -> Result<()> {
    let mut client = connect()?;

    // 先订阅事件
    client.subscribe(vec![EventType::SpaceUpdated], false)?;

    // 发送 rescan 命令
    client.rescan()?;

    if !fmt.is_json() {
        eprint!("{}...", messages::RESCANNING);
        io::stderr().flush().unwrap();
    }

    // 等待两个 SpaceUpdated 事件（Video + Image）
    let mut collector = SpaceCollector::new();

    let result = wait_for_events(&mut client, fmt, |event| {
        if let Event::SpaceUpdated { mode, summary, .. } = event {
            return collector.record(mode, summary.total, summary.available, summary.locked);
        }
        false
    });

    // 清除 "Rescanning..."
    if !fmt.is_json() {
        eprint!("\r\x1b[K");
    }

    match result {
        Ok(()) => {
            let (total, available, locked) = collector.totals();
            if fmt.is_json() {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "ok",
                        "total": total,
                        "available": available,
                        "locked": locked,
                        "video": collector.video.as_ref().map(|v| serde_json::json!({
                            "total": v.total,
                            "available": v.available,
                            "locked": v.locked
                        })),
                        "image": collector.image.as_ref().map(|v| serde_json::json!({
                            "total": v.total,
                            "available": v.available,
                            "locked": v.locked
                        }))
                    })
                );
            } else {
                fmt.print_success(&format!(
                    "Rescanned: {} wallpapers ({} available, {} locked) [{}]",
                    total, available, locked, collector.detail_string(fmt)
                ));
            }
        }
        Err(_) => {
            print_timeout_message(fmt);
        }
    }

    Ok(())
}

/// 等待事件，带超时和提示
fn wait_for_events<F>(
    client: &mut crate::client::Client,
    fmt: &Formatter,
    mut is_done: F,
) -> std::result::Result<(), ()>
where
    F: FnMut(&Event) -> bool,
{
    let start = Instant::now();
    let mut hint_shown = false;

    // 设置短超时用于轮询（1秒）
    let _ = client.set_read_timeout(Some(Duration::from_secs(1)));

    loop {
        let elapsed = start.elapsed().as_secs();

        // Timeout check
        if elapsed >= WAIT_TIMEOUT_SECS {
            return Err(());
        }

        // Show hint after 10 seconds
        if !hint_shown && elapsed >= HINT_DELAY_SECS && !fmt.is_json() {
            hint_shown = true;
            eprintln!();
            eprintln!("  {}", crate::output::messages::SCAN_HINT_LINE1);
            eprintln!("  {}", crate::output::messages::SCAN_HINT_LINE2);
            eprint!("  {}...", crate::output::messages::WAITING);
            io::stderr().flush().unwrap();
        }

        // Try to receive event
        match client.receive_event() {
            Ok(event) => {
                if is_done(&event) {
                    return Ok(());
                }
            }
            Err(ClientError::Io(ref e)) if e.kind() == io::ErrorKind::WouldBlock => {
                // Timeout, continue waiting
                continue;
            }
            Err(ClientError::Io(ref e)) if e.kind() == io::ErrorKind::TimedOut => {
                // Timeout, continue waiting
                continue;
            }
            Err(_) => {
                // Other error, exit
                return Err(());
            }
        }
    }
}

/// Print timeout message
fn print_timeout_message(fmt: &Formatter) {
    use crate::output::messages;
    
    if fmt.is_json() {
        println!(
            "{}",
            serde_json::json!({
                "status": "timeout",
                "message": messages::SCAN_BACKGROUND_HINT
            })
        );
    } else {
        fmt.print_warning(messages::TIMEOUT_WARNING);
        fmt.print_info(messages::SCAN_BACKGROUND_HINT);
    }
}
