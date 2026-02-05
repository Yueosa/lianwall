//! LianWall CLI (lianwall)
//!
//! 命令行客户端，通过 Unix Socket 与 lianwalld 守护进程通信

mod client;
mod commands;
mod handlers;
mod output;
mod subscribe;

use clap::Parser;
use commands::{Cli, Command};
use handlers::HandlerError;
use output::{Formatter, TerminalCaps};

fn main() {
    let cli = Cli::parse();

    // 无参数时显示帮助
    if cli.command.is_none() {
        use clap::CommandFactory;
        Cli::command().print_help().unwrap();
        println!();
        return;
    }

    // 初始化格式化器
    let caps = if cli.no_color {
        TerminalCaps::detect().without_color()
    } else {
        TerminalCaps::detect()
    };
    let fmt = Formatter::new(caps, cli.json);

    // 执行命令
    let result = match cli.command.unwrap() {
        // 生命周期
        Command::Start { foreground } => handlers::handle_start(&fmt, foreground),
        Command::Stop => handlers::handle_stop(&fmt),
        Command::Restart => handlers::handle_restart(&fmt),
        // 状态查询
        Command::Status => handlers::handle_status(&fmt),
        // 壁纸控制
        Command::Next => handlers::handle_next(&fmt),
        Command::Prev => handlers::handle_prev(&fmt),
        Command::Switch => handlers::handle_switch(&fmt),
        Command::Set { path } => handlers::handle_set(&fmt, path),
        Command::Mode { mode } => handlers::handle_mode(&fmt, mode),
        Command::Lock { path } => handlers::handle_lock(&fmt, path),
        Command::Unlock { path } => handlers::handle_unlock(&fmt, path),
        Command::Reload => handlers::handle_reload(&fmt),
        Command::Rescan => handlers::handle_rescan(&fmt),
        // 配置
        Command::Config { action } => handlers::handle_config(&fmt, action),
        // 订阅（调试）
        Command::Subscribe { events } => {
            let socket_path = get_socket_path();
            subscribe::run_subscribe(&fmt, &socket_path, events).map_err(|e| e.into())
        }
    };

    // 处理错误
    if let Err(e) = result {
        if cli.json {
            eprintln!(
                "{}",
                serde_json::json!({
                    "success": false,
                    "error": e.to_string()
                })
            );
        } else {
            fmt.print_error(&e.to_string());
        }

        // 特殊退出码
        let exit_code = match e {
            HandlerError::DaemonNotRunning => 2,
            _ => 1,
        };
        std::process::exit(exit_code);
    }
}

/// 获取 socket 路径
fn get_socket_path() -> std::path::PathBuf {
    match lianwall_core::config::read(lianwall_core::config::ConfigReadInput { path: None }) {
        Ok(output) => output.config.daemon.socket_path,
        Err(_) => std::path::PathBuf::from("/tmp/lianwall.sock"),
    }
}
