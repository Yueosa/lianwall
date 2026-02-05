//! 服务端主循环

use std::io;
use std::path::Path;
use std::time::Duration;

use polling::{Event, Events, Poller};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;

use lianwall_core::engine;
use lianwall_core::socket::Server;

use crate::error::DaemonError;
use crate::handler::{handle_request, DaemonState};
use crate::scheduler::Scheduler;

/// 事件 Key
const KEY_SOCKET: usize = 0;

/// 运行守护进程主循环
pub fn run(mut state: DaemonState, socket_path: &Path) -> Result<(), DaemonError> {
    // 绑定 socket
    let server = Server::bind(socket_path, true).map_err(DaemonError::Socket)?;
    server
        .set_nonblocking(true)
        .map_err(DaemonError::Socket)?;

    tracing::info!("监听: {}", socket_path.display());

    // 创建 poller
    let poller = Poller::new().map_err(|e| DaemonError::Io("创建 poller", e))?;

    // 注册 socket
    // Safety: UnixListener 实现了 AsRawFd
    unsafe {
        poller
            .add(server.as_listener(), Event::readable(KEY_SOCKET))
            .map_err(|e| DaemonError::Io("注册 socket", e))?;
    }

    // 注册信号处理
    let mut signals =
        Signals::new([SIGINT, SIGTERM]).map_err(|e| DaemonError::Io("注册信号", e))?;

    // 调度器
    let mut scheduler = Scheduler::new();

    // 首次启动立即切换壁纸
    scheduler.trigger_immediate(&mut state);

    // 事件缓冲区
    let mut events = Events::new();

    // 主循环
    loop {
        // 检查退出条件
        if state.shutdown_requested {
            break;
        }

        // 检查信号
        for sig in signals.pending() {
            match sig {
                SIGINT | SIGTERM => {
                    tracing::info!("收到信号 {}, 准备退出", sig);
                    state.shutdown_requested = true;
                }
                _ => {}
            }
        }

        if state.shutdown_requested {
            break;
        }

        // 计算超时时间
        let deadline = scheduler.next_deadline(&state);
        let now = std::time::Instant::now();
        let timeout = if deadline > now {
            Some(deadline - now)
        } else {
            Some(Duration::ZERO)
        };

        // 等待事件
        events.clear();
        match poller.wait(&mut events, timeout) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(DaemonError::Io("poll", e)),
        }

        // 处理 socket 事件
        for ev in events.iter() {
            if ev.key == KEY_SOCKET && ev.readable {
                handle_connections(&server, &mut state);

                // 重新注册 socket（polling 是 one-shot 的）
                poller
                    .modify(server.as_listener(), Event::readable(KEY_SOCKET))
                    .ok();
            }
        }

        // 执行定时任务
        if let Err(e) = scheduler.tick(&mut state) {
            tracing::error!("调度器错误: {}", e);
        }
    }

    // 优雅关闭
    graceful_shutdown(&mut state)?;

    Ok(())
}

/// 处理所有待处理的连接
fn handle_connections(server: &Server, state: &mut DaemonState) {
    // 非阻塞地接受所有连接
    loop {
        match server.accept() {
            Ok(mut conn) => {
                // 设置超时
                let _ = conn.set_timeout(Some(Duration::from_secs(5)));

                // 处理请求
                let result = conn.serve(|req| {
                    let resp = handle_request(state, req);
                    let should_continue = !state.shutdown_requested;
                    (resp, should_continue)
                });

                if let Err(e) = result {
                    tracing::debug!("连接处理错误: {}", e);
                }
            }
            Err(lianwall_core::socket::SocketError::RecvFailed(ref e))
                if e.kind() == io::ErrorKind::WouldBlock =>
            {
                // 没有更多连接
                break;
            }
            Err(e) => {
                tracing::debug!("接受连接失败: {}", e);
                break;
            }
        }
    }
}

/// 优雅关闭
fn graceful_shutdown(state: &mut DaemonState) -> Result<(), DaemonError> {
    tracing::info!("正在关闭...");

    // 保存状态
    if let Err(e) = state.save_weights() {
        tracing::warn!("保存状态失败: {}", e);
    }

    // 停止引擎
    if let Err(e) = engine::shutdown(&mut state.engine) {
        tracing::warn!("停止引擎失败: {}", e);
    }

    tracing::info!("守护进程已关闭");
    Ok(())
}
