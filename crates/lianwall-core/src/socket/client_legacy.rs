//! Socket 客户端
//!
//! 提供连接守护进程并发送命令的高层 API
//! CLI 和 GUI 都使用这个客户端

use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::codec::{recv_json, send_json};
use super::error::SocketError;
use super::protocol::{Request, Response, ResponseData, SpaceSnapshot, StatusInfo};
use crate::config::WallMode;

/// 默认连接超时时间
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Socket 客户端
pub struct Client {
    stream: UnixStream,
}

impl Client {
    /// 连接到守护进程
    ///
    /// # Arguments
    /// * `socket_path` - Unix Socket 路径
    ///
    /// # Example
    /// ```ignore
    /// let client = Client::connect("/tmp/lianwall.sock")?;
    /// ```
    pub fn connect(socket_path: impl AsRef<Path>) -> Result<Self, SocketError> {
        let path = socket_path.as_ref();

        // 检查 socket 文件是否存在
        if !path.exists() {
            return Err(SocketError::DaemonNotRunning);
        }

        let stream = UnixStream::connect(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::ConnectionRefused {
                SocketError::DaemonNotRunning
            } else {
                SocketError::ConnectFailed {
                    path: path.to_path_buf(),
                    source: e,
                }
            }
        })?;

        // 设置读写超时
        stream
            .set_read_timeout(Some(DEFAULT_TIMEOUT))
            .map_err(SocketError::SendFailed)?;
        stream
            .set_write_timeout(Some(DEFAULT_TIMEOUT))
            .map_err(SocketError::SendFailed)?;

        Ok(Self { stream })
    }

    /// 发送请求并接收响应
    fn request(&mut self, req: Request) -> Result<Response, SocketError> {
        send_json(&mut self.stream, &req)?;
        recv_json(&mut self.stream)
    }

    /// 发送请求并检查是否成功
    fn request_ok(&mut self, req: Request) -> Result<(), SocketError> {
        let resp = self.request(req)?;
        if resp.success {
            Ok(())
        } else {
            Err(SocketError::DeserializeFailed {
                context: resp.error.unwrap_or_else(|| "未知错误".to_string()),
                source: serde_json::from_str::<()>("").unwrap_err(),
            })
        }
    }

    // ========================================================================
    // 高层 API
    // ========================================================================

    /// 心跳检测
    pub fn ping(&mut self) -> Result<bool, SocketError> {
        let resp = self.request(Request::Ping)?;
        Ok(resp.success && matches!(resp.data, Some(ResponseData::Pong)))
    }

    /// 获取状态
    pub fn status(&mut self) -> Result<StatusInfo, SocketError> {
        let resp = self.request(Request::Status)?;
        match resp.data {
            Some(ResponseData::Status(info)) => Ok(info),
            _ => Err(SocketError::DeserializeFailed {
                context: "期望 StatusInfo 响应".to_string(),
                source: serde_json::from_str::<()>("").unwrap_err(),
            }),
        }
    }

    /// 获取向量空间快照（GUI 绘图用）
    pub fn get_space(&mut self) -> Result<SpaceSnapshot, SocketError> {
        let resp = self.request(Request::GetSpace)?;
        match resp.data {
            Some(ResponseData::Space(snapshot)) => Ok(snapshot),
            _ => Err(SocketError::DeserializeFailed {
                context: "期望 SpaceSnapshot 响应".to_string(),
                source: serde_json::from_str::<()>("").unwrap_err(),
            }),
        }
    }

    /// 切换到下一张壁纸
    pub fn next(&mut self) -> Result<(), SocketError> {
        self.request_ok(Request::Next { trigger_hint: None })
    }

    /// 切换到上一张壁纸
    pub fn previous(&mut self) -> Result<(), SocketError> {
        self.request_ok(Request::Previous { trigger_hint: None })
    }

    /// 指定壁纸
    pub fn set_wallpaper(&mut self, path: impl Into<PathBuf>) -> Result<(), SocketError> {
        self.request_ok(Request::SetWallpaper { path: path.into() })
    }

    /// 切换模式
    pub fn set_mode(&mut self, mode: WallMode) -> Result<(), SocketError> {
        self.request_ok(Request::SetMode { mode })
    }

    /// 锁定壁纸
    pub fn lock(&mut self, path: impl Into<PathBuf>) -> Result<(), SocketError> {
        self.request_ok(Request::Lock { path: path.into() })
    }

    /// 解锁壁纸
    pub fn unlock(&mut self, path: impl Into<PathBuf>) -> Result<(), SocketError> {
        self.request_ok(Request::Unlock { path: path.into() })
    }

    /// 重载配置和壁纸目录
    pub fn reload(&mut self) -> Result<(), SocketError> {
        self.request_ok(Request::Reload)
    }

    /// 关闭守护进程
    pub fn shutdown(&mut self) -> Result<(), SocketError> {
        self.request_ok(Request::Shutdown)
    }
}

/// 快捷函数：连接并执行单个命令
pub mod quick {
    use super::*;

    /// 检查守护进程是否运行
    pub fn is_running(socket_path: impl AsRef<Path>) -> bool {
        Client::connect(socket_path)
            .and_then(|mut c| c.ping())
            .unwrap_or(false)
    }

    /// 获取状态
    pub fn status(socket_path: impl AsRef<Path>) -> Result<StatusInfo, SocketError> {
        Client::connect(socket_path)?.status()
    }

    /// 下一张壁纸
    pub fn next(socket_path: impl AsRef<Path>) -> Result<(), SocketError> {
        Client::connect(socket_path)?.next()
    }

    /// 上一张壁纸
    pub fn previous(socket_path: impl AsRef<Path>) -> Result<(), SocketError> {
        Client::connect(socket_path)?.previous()
    }

    /// 关闭守护进程
    pub fn shutdown(socket_path: impl AsRef<Path>) -> Result<(), SocketError> {
        Client::connect(socket_path)?.shutdown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_nonexistent() {
        let result = Client::connect("/nonexistent/path.sock");
        assert!(matches!(result, Err(SocketError::DaemonNotRunning)));
    }

    #[test]
    fn test_is_running_false() {
        assert!(!quick::is_running("/nonexistent/path.sock"));
    }
}
