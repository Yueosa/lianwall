//! CLI 层
//!
//! ## 职责
//! - 解析命令行参数（clap）
//! - 调用 API 层函数
//! - 格式化输出（彩色、emoji）
//!
//! ## 全局参数
//! - `--debug`: 启用 debug 追踪
//! - `--json`: JSON 格式输出
//!
//! ## 命令列表
//! - `start`: 启动守护进程
//! - `stop`: 停止守护进程
//! - `next`: 切换壁纸
//! - `switch`: 切换模式
//! - `reload`: 热重载壁纸目录
//! - `status`: 查询状态
//! - `list`: 列出壁纸
//! - `lock`: 锁定壁纸
//! - `unlock`: 解锁壁纸
//! - `stats`: 统计信息
//! - `diagnose`: 诊断系统
//! - `config`: 配置管理

mod commands;
mod error;
mod handlers;
mod output;

use commands::Cli;
use error::CliError;

/// CLI 入口函数
pub fn run() -> ! {
    let result = run_cli();

    let exit_code = match &result {
        Ok(_) => 0,
        Err(CliError::UserCancelled) => 130,   // 用户取消
        Err(CliError::InvalidFilter(_)) => 2,  // 参数错误
        Err(CliError::InvalidPath(_)) => 2,    // 参数错误
        Err(CliError::Api(_)) => 1,            // API 错误
        Err(CliError::Io(_)) => 3,             // IO 错误
    };

    // 只在非用户取消的情况下打印错误
    if let Err(e) = &result {
        if !matches!(e, CliError::UserCancelled) {
            output::print_error_chain(e);
        }
    }

    std::process::exit(exit_code);
}

fn run_cli() -> Result<(), CliError> {
    let cli = Cli::parse_args();
    handlers::handle_command(cli)
}
