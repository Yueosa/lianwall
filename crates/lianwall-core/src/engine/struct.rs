//! 引擎模块数据结构

/// 引擎检测结果
#[derive(Debug, Clone)]
pub struct DetectOutput {
    /// mpvpaper 是否可用
    pub mpvpaper_available: bool,
    /// 图片引擎是否可用（awww 或 swww 任一可用即为 true）
    pub swww_available: bool,
}
