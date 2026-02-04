//! 引擎模块数据结构

use crate::config::WallMode;
use std::path::PathBuf;
use std::process::Child;

/// 引擎运行状态
///
/// 由 daemon 持有，管理引擎进程的生命周期
pub struct EngineState {
    /// 当前运行模式
    pub mode: WallMode,
    /// 当前壁纸路径
    pub current: Option<PathBuf>,
    /// mpvpaper 进程句柄（总是由我们管理）
    pub(crate) mpvpaper: Option<Child>,
    /// swww-daemon 进程句柄（仅当我们启动时持有）
    pub(crate) swww_daemon: Option<Child>,
    /// 标记 swww-daemon 是否是外部启动的
    pub(crate) swww_daemon_external: bool,
}

impl EngineState {
    /// 创建新的引擎状态
    pub fn new(mode: WallMode) -> Self {
        Self {
            mode,
            current: None,
            mpvpaper: None,
            swww_daemon: None,
            swww_daemon_external: false,
        }
    }

    /// 检查 mpvpaper 是否正在运行
    pub fn is_mpvpaper_running(&mut self) -> bool {
        if let Some(ref mut child) = self.mpvpaper {
            // try_wait 返回 None 表示进程仍在运行
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }

    /// 检查 swww-daemon 是否正在运行
    pub fn is_swww_daemon_running(&mut self) -> bool {
        if self.swww_daemon_external {
            // 外部启动的，用 pgrep 检测
            std::process::Command::new("pgrep")
                .arg("-x")
                .arg("swww-daemon")
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        } else if let Some(ref mut child) = self.swww_daemon {
            matches!(child.try_wait(), Ok(None))
        } else {
            false
        }
    }
}

/// 引擎检测结果
#[derive(Debug, Clone)]
pub struct DetectOutput {
    /// mpvpaper 是否可用
    pub mpvpaper_available: bool,
    /// swww 是否可用
    pub swww_available: bool,
}
