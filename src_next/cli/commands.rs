use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lianwall")]
#[command(version = "2.0.0")]
#[command(about = "🌌 智能动态壁纸管理器", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 🚀 启动守护进程
    Start {
        /// 启用 debug 追踪
        #[arg(long)]
        debug: bool,
    },

    /// 🔄 切换壁纸/模式
    Next {
        /// 切换模式（Video ↔ Image）
        #[arg(long)]
        mode: bool,

        /// 启用 debug 追踪
        #[arg(long)]
        debug: bool,
    },

    /// 🛑 停止守护进程
    Stop {
        /// 启用 debug 追踪
        #[arg(long)]
        debug: bool,
    },

    /// 🔃 热重载壁纸目录
    Reload {
        /// 指定模式 (video/image)
        #[arg(long, value_name = "MODE")]
        mode: Option<String>,

        /// 启用 debug 追踪
        #[arg(long)]
        debug: bool,
    },

    /// 📊 查询当前状态
    Status {
        /// JSON 格式输出
        #[arg(long)]
        json: bool,

        /// 启用 debug 追踪
        #[arg(long)]
        debug: bool,
    },

    /// 🔍 诊断系统
    Diagnose {
        /// JSON 格式输出
        #[arg(long)]
        json: bool,

        /// 启用 debug 追踪
        #[arg(long)]
        debug: bool,
    },

    /// ⚙️  配置管理
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// 🗑️  卸载程序
    Uninstall {
        /// 删除用户数据（缓存、配置）
        #[arg(long)]
        purge: bool,

        /// 跳过确认提示
        #[arg(long, short)]
        yes: bool,

        /// 启用 debug 追踪
        #[arg(long)]
        debug: bool,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// 📖 获取配置项
    Get {
        /// 配置键（如 weight.base）
        key: String,

        /// 启用 debug 追踪
        #[arg(long)]
        debug: bool,
    },

    /// ✏️  设置配置项
    Set {
        /// 配置键
        key: String,

        /// 配置值
        value: String,

        /// 启用 debug 追踪
        #[arg(long)]
        debug: bool,
    },

    /// 📄 显示完整配置
    Show {
        /// 启用 debug 追踪
        #[arg(long)]
        debug: bool,
    },

    /// 🔄 重置为默认值
    Reset {
        /// 启用 debug 追踪
        #[arg(long)]
        debug: bool,
    },
}

impl Cli {
    pub fn parse_args() -> Self {
        Cli::parse()
    }
}
