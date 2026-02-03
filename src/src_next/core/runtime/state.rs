use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// 状态文件路径
fn state_path() -> PathBuf {
    let cache_dir = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("~/.cache"));
    cache_dir.join("lianwall/state.json")
}

/// 运行时状态（内存中的完整状态）
#[derive(Debug, Clone)]
pub struct RuntimeState {
    /// 选择计数器（用于洗牌周期）
    pub selection_count: u32,
    /// 当前壁纸路径
    pub current_wallpaper: Option<PathBuf>,
    /// 当前运行模式
    pub current_mode: RunMode,
    /// 是否正在运行（不持久化）
    pub is_running: bool,
    /// 是否因 VRAM 降级而切换到 Image 模式（用于区分主动配置和被动降级）
    pub was_degraded: bool,
}

/// 持久化状态（存储到文件）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedState {
    selection_count: u32,
    current_wallpaper: Option<PathBuf>,
    current_mode: RunMode,
    was_degraded: bool,
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

    /// 从文件加载状态（不存在则返回默认值）
    pub fn load() -> Self {
        let path = state_path();
        if !path.exists() {
            return Self::default();
        }

        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<PersistedState>(&content) {
                Ok(persisted) => Self {
                    selection_count: persisted.selection_count,
                    current_wallpaper: persisted.current_wallpaper,
                    current_mode: persisted.current_mode,
                    is_running: false, // 进程重启后默认未运行
                    was_degraded: persisted.was_degraded,
                },
                Err(_) => Self::default(),
            },
            Err(_) => Self::default(),
        }
    }

    /// 保存状态到文件
    pub fn save(&self) {
        let path = state_path();

        // 确保目录存在
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let persisted = PersistedState {
            selection_count: self.selection_count,
            current_wallpaper: self.current_wallpaper.clone(),
            current_mode: self.current_mode.clone(),
            was_degraded: self.was_degraded,
        };

        if let Ok(content) = serde_json::to_string_pretty(&persisted) {
            let _ = fs::write(&path, content);
        }
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
