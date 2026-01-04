#![allow(dead_code)]

pub mod mpvpaper;
pub mod swww;

use std::path::Path;

/// 壁纸引擎 trait，定义统一接口
pub trait PaperEngine {
    /// 引擎名称
    fn name(&self) -> &'static str;

    /// 设置壁纸
    fn set_wallpaper(&self, path: &Path) -> Result<(), String>;

    /// 停止当前壁纸
    fn stop(&self) -> Result<(), String>;

    /// 检查引擎是否可用
    fn is_available(&self) -> bool;
}

/// 根据引擎类型创建对应的引擎实例
pub fn create_engine(engine_type: &str) -> Box<dyn PaperEngine> {
    match engine_type {
        "mpvpaper" => Box::new(mpvpaper::MpvPaper::new()),
        "swww" => Box::new(swww::Swww::new()),
        _ => {
            eprintln!("未知引擎类型: {}, 使用默认 mpvpaper", engine_type);
            Box::new(mpvpaper::MpvPaper::new())
        }
    }
}

/// 获取引擎支持的文件扩展名
pub fn supported_extensions(engine_type: &str) -> Vec<&'static str> {
    match engine_type {
        "mpvpaper" => mpvpaper::MpvPaper::supported_extensions().to_vec(),
        "swww" => swww::Swww::supported_extensions().to_vec(),
        _ => mpvpaper::MpvPaper::supported_extensions().to_vec(),
    }
}
