use std::path::PathBuf;

use crate::core::algorithm::AlgorithmStatsOutput;
use crate::core::gpu::GpuType;
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

// --- 自检输出结构 ---

#[derive(Debug, Clone)]
pub struct DiagnoseGpuOutput {
    /// GPU 类型
    pub gpu_type: GpuType,
    /// 是否可用于 VRAM 监控
    pub vram_available: bool,
    /// 检测原因/说明
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DiagnoseEnginesOutput {
    /// mpvpaper 是否已安装
    pub mpvpaper_installed: bool,
    /// swww 是否已安装
    pub swww_installed: bool,
}

#[derive(Debug, Clone)]
pub struct DiagnoseDirsOutput {
    /// Video 目录是否存在
    pub video_dir_exists: bool,
    /// Video 目录壁纸数量
    pub video_count: usize,
    /// Image 目录是否存在
    pub image_dir_exists: bool,
    /// Image 目录壁纸数量
    pub image_count: usize,
}

#[derive(Debug, Clone)]
pub struct DiagnoseAllOutput {
    /// 配置文件路径
    pub config_path: PathBuf,
    /// 配置文件是否存在
    pub config_exists: bool,
    /// GPU 检测结果
    pub gpu: DiagnoseGpuOutput,
    /// 引擎安装检测结果
    pub engines: DiagnoseEnginesOutput,
    /// 目录检测结果
    pub dirs: DiagnoseDirsOutput,
    /// 是否全部通过
    pub all_passed: bool,
    /// 错误信息列表
    pub errors: Vec<String>,
}

// --- 壁纸列表输出 ---

#[derive(Debug, Clone)]
pub struct WallpaperInfo {
    /// 壁纸路径
    pub path: PathBuf,
    /// 当前权重
    pub weight: f64,
    /// 是否被锁定
    pub locked: bool,
    /// 跳过次数
    pub skip_streak: u32,
    /// 最后播放时间戳
    pub last_played: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct WallpaperListOutput {
    /// 模式
    pub mode: RunMode,
    /// 活跃壁纸列表（当前时间段匹配且未锁定）
    pub active: Vec<WallpaperInfo>,
    /// 锁定壁纸列表
    pub locked: Vec<WallpaperInfo>,
    /// 非活跃壁纸列表（时间段不匹配）
    pub inactive: Vec<WallpaperInfo>,
}

// --- 锁定操作输出 ---

#[derive(Debug, Clone)]
pub struct LockOutput {
    /// 操作的壁纸路径
    pub path: PathBuf,
    /// 操作后的锁定状态
    pub locked: bool,
}
