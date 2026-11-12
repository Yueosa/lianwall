//! 壁纸数据结构定义

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;

use super::time_range::TimeRange;

/// 单个壁纸记录（运行时）
#[derive(Debug, Clone)]
pub struct WallpaperRecord {
    /// 文件路径
    pub path: PathBuf,
    /// 在圆周上的角度 [0, 2π)
    pub angle: f64,
    /// 是否锁定（锁定后不会被选中）
    pub locked: bool,
    /// 上次播放的 Unix 时间戳
    pub last_played: Option<u64>,
    /// 时间约束列表（空表示无时间限制，全天可用）
    pub time_constraints: Vec<TimeRange>,
}

/// 向量空间（运行时）
#[derive(Debug, Clone)]
pub struct WallpaperSpace {
    /// 壁纸列表
    pub items: Vec<WallpaperRecord>,
    /// 当前指针角度 [0, 2π)
    pub pointer: f64,
    /// 选择器步进计数器，用于生成可复现的采样随机流
    pub selector_nonce: u64,
    /// 冷却队列（存储最近选中的壁纸索引）
    pub cooldown_queue: VecDeque<usize>,
    /// 当前壁纸索引
    pub current_index: Option<usize>,
}

impl WallpaperSpace {
    /// 获取壁纸数量
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Unlocked entries outside cooldown, before the selector's cooldown fallback.
    pub fn ready_count(&self) -> usize {
        let cooldown: std::collections::HashSet<_> = self.cooldown_queue.iter().copied().collect();
        self.items
            .iter()
            .enumerate()
            .filter(|(index, item)| !item.locked && !cooldown.contains(index))
            .count()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 获取可选壁纸数量（排除锁定的）
    pub fn available_count(&self) -> usize {
        self.items.iter().filter(|w| !w.locked).count()
    }
}

// === 持久化结构 ===

/// 持久化文件根结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightsFile {
    /// 文件版本
    pub version: u32,
    /// 视频模式数据
    pub video: ModeData,
    /// 图片模式数据
    pub image: ModeData,
}

impl Default for WeightsFile {
    fn default() -> Self {
        Self {
            version: 1,
            video: ModeData::default(),
            image: ModeData::default(),
        }
    }
}

/// 单个模式的持久化数据
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModeData {
    /// 指针角度
    pub pointer: f64,
    /// 选择器步进计数器
    #[serde(default)]
    pub selector_nonce: u64,
    /// 当前壁纸路径（用于启动时恢复）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_path: Option<PathBuf>,
    /// 壁纸记录列表
    pub items: Vec<PersistedRecord>,
}

/// 持久化的壁纸记录（不包含 angle，重建时重新分配）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedRecord {
    /// 文件路径
    pub path: PathBuf,
    /// 是否锁定
    #[serde(default)]
    pub locked: bool,
    /// 上次播放时间
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_played: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ready_count_does_not_double_subtract_locked_cooldown() {
        let items = (0..3)
            .map(|index| WallpaperRecord {
                path: format!("/{index}.jpg").into(),
                angle: 0.0,
                locked: index == 0,
                last_played: None,
                time_constraints: vec![],
            })
            .collect();
        let space = WallpaperSpace {
            items,
            pointer: 0.0,
            selector_nonce: 0,
            cooldown_queue: VecDeque::from([0, 1, 1, 999]),
            current_index: None,
        };
        assert_eq!(space.available_count(), 2);
        assert_eq!(space.ready_count(), 1);
    }
}
