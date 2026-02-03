use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "lianwall")]
#[command(version = "3.0.0")]
#[command(about = "🌌 智能动态壁纸管理器", long_about = None)]
pub struct Cli {
    /// 启用 debug 追踪
    #[arg(long, global = true)]
    pub debug: bool,

    /// JSON 格式输出
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 🚀 启动守护进程
    Start,

    /// 🛑 停止守护进程
    Stop,

    /// ➡️  切换下一张壁纸
    Next,

    /// 🔀 切换模式（Video ↔ Image）
    Switch,

    /// 🔃 热重载壁纸目录
    Reload,

    /// 📊 查询当前状态
    Status,

    /// 📋 列出壁纸
    List {
        /// 过滤类型 (all/active/locked)
        #[arg(long, default_value = "all")]
        filter: String,
    },

    /// 🔒 锁定壁纸（不再参与轮换）
    Lock {
        /// 壁纸路径
        path: PathBuf,
    },

    /// 🔓 解锁壁纸（重新参与轮换）
    Unlock {
        /// 壁纸路径
        path: PathBuf,
    },

    /// 📈 统计信息
    Stats,

    /// 🔍 诊断系统
    Diagnose,

    /// ⚙️  配置管理
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// 📖 获取配置项
    Get {
        /// 配置键（如 weight.base）
        key: String,
    },

    /// ✏️  设置配置项
    Set {
        /// 配置键
        key: String,
        /// 配置值
        value: String,
    },

    /// 📄 显示完整配置
    Show,

    /// 🔄 重置为默认值
    Reset {
        /// 跳过确认提示
        #[arg(long, short)]
        yes: bool,
    },
}

impl Cli {
    pub fn parse_args() -> Self {
        Cli::parse()
    }
}
