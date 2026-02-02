//! CLI 层
//!
//! ## 职责
//! - 解析命令行参数（clap）
//! - 调用 API 层函数
//! - 格式化输出（彩色、emoji、表格）
//!
//! ## 命令列表
//! - `start`: 启动守护进程
//! - `next`: 切换壁纸/模式
//! - `stop`: 停止守护进程
//! - `reload`: 热重载壁纸目录
//! - `status`: 查询状态
//! - `diagnose`: 诊断系统
//! - `config`: 配置管理
//! - `uninstall`: 卸载程序

mod commands;
mod error;
mod handlers;
mod output;

use commands::Cli;
use error::CliError;

/// CLI 入口函数
pub fn run() {
    let cli = Cli::parse_args();

    if let Err(e) = handlers::handle_command(cli.command) {
        // 用户取消操作时不打印错误
        if !matches!(e, CliError::UserCancelled) {
            output::print_error_chain(&e);
            std::process::exit(1);
        }
    }
}
