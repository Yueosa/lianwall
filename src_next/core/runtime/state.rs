use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 运行时状态
#[derive(Debug, Clone)]
pub struct RuntimeState {
    /// 选择计数器（用于洗牌周期）
    pub selection_count: u32,
    /// 当前壁纸路径
    pub current_wallpaper: Option<PathBuf>,
    /// 当前运行模式
    pub current_mode: RunMode,
    /// 是否正在运行
    pub is_running: bool,
    /// 是否因 VRAM 降级而切换到 Image 模式（用于区分主动配置和被动降级）
    pub was_degraded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunMode {
    Video,
    Image,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            selection_count: 0,
            current_wallpaper: None,
            current_mode: RunMode::Video,
            is_running: false,
            was_degraded: false,
        }
    }
}

impl RuntimeState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 增加选择计数
    pub fn increment_selection_count(&mut self) -> u32 {
        self.selection_count += 1;
        self.selection_count
    }

    /// 重置选择计数
    pub fn reset_selection_count(&mut self) {
        self.selection_count = 0;
    }
}
