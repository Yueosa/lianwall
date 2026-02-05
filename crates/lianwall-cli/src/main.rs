//! LianWall CLI (lianwall)
//!
//! 命令行客户端，通过 Unix Socket 与 daemon 通信
//! 
//! 使用 `--daemon` 参数启动守护进程模式

mod commands;
mod daemon;
mod handlers;
mod output;

use clap::Parser;
use commands::{Cli, Command};
use handlers::HandlerError;
use output::{Formatter, TerminalCaps};

fn main() {
    let cli = Cli::parse();

    // Daemon 模式：直接运行守护进程
    if cli.daemon {
        if let Err(e) = daemon::run(None) {
            eprintln!("守护进程错误: {}", e);
            std::process::exit(1);
        }
        return;
    }

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
        // 配置
        Command::Config { action } => handlers::handle_config(&fmt, action),
    };

    // 处理错误
    if let Err(e) = result {
        if cli.json {
            eprintln!("{}", serde_json::json!({
                "success": false,
                "error": e.to_string()
            }));
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
