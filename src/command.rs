use clap::{Parser, Subcommand};

/// LianWall - 智能动态壁纸管理器
/// 基于负反馈闭环调节的壁纸轮换系统
#[derive(Parser, Debug)]
#[command(name = "lianwall")]
#[command(author = "Sakurine")]
#[command(version = "0.1.0")]
#[command(about = "智能动态壁纸管理器", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// 启动后台守护进程（动态壁纸模式）
    Daemon,
    
    /// 立即切换到下一张壁纸（根据当前模式）
    Next,
    
    /// 切换到动态壁纸模式（视频）
    Video,
    
    /// 切换到静态壁纸模式（图片）
    Picture,
    
    /// 热重载：重新扫描壁纸目录并更新权重文件
    Reset {
        /// 指定模式: video 或 picture，默认 video
        #[arg(short, long, default_value = "video")]
        mode: String,
    },
    
    /// 显示当前状态和壁纸列表
    Status {
        /// 指定模式: video 或 picture，默认 video
        #[arg(short, long, default_value = "video")]
        mode: String,
    },
}

impl Cli {
    pub fn parse_args() -> Self {
        Cli::parse()
    }
}
