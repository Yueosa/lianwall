//! 引擎模块数据结构

/// 引擎检测结果
#[derive(Debug, Clone)]
pub struct DetectOutput {
    /// mpvpaper 是否可用
    pub mpvpaper_available: bool,
    /// swww 是否可用
    pub swww_available: bool,
}
