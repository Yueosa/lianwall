use std::path::PathBuf;

use crate::core::algorithm::AlgorithmStatsOutput;
use crate::core::runtime::RunMode;

// --- 输出结构 ---

#[derive(Debug, Clone)]
pub struct ManagerNextOutput {
    /// 选中的壁纸路径
    pub selected_path: PathBuf,
    /// 当前模式
    pub mode: RunMode,
    /// 是否触发了归一化
    pub normalized: bool,
    /// 是否触发了洗牌
    pub shuffled: bool,
}

#[derive(Debug, Clone)]
pub struct ManagerReloadOutput {
    /// 总记录数（包括封锁的）
    pub total_count: usize,
    /// 活跃记录数（当前时间段）
    pub active_count: usize,
    /// 新增文件数
    pub new_count: usize,
    /// 删除文件数
    pub removed_count: usize,
}

#[derive(Debug, Clone)]
pub struct ManagerStatusOutput {
    /// 当前模式
    pub current_mode: RunMode,
    /// 当前壁纸
    pub current_wallpaper: Option<PathBuf>,
    /// 是否正在运行
    pub is_running: bool,
    /// 选择计数
    pub selection_count: u32,
    /// Video 模式统计
    pub video_stats: Option<ModeStats>,
    /// Image 模式统计
    pub image_stats: Option<ModeStats>,
}

#[derive(Debug, Clone)]
pub struct ModeStats {
    /// 总记录数（all_records）
    pub total_count: usize,
    /// 活跃记录数（active_records）
    pub active_count: usize,
    /// 封锁记录数（total - active）
    pub locked_count: usize,
    /// 算法统计信息
    pub algorithm_stats: AlgorithmStatsOutput,
}
