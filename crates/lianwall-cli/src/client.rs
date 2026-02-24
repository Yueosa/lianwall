//! Socket 客户端封装
//!
//! 提供与 lianwalld 守护进程通信的同步 API。
//! 内部使用 Unix Socket + 长度前缀帧协议。
//!
//! # API 说明
//!
//! 本模块提供了完整的客户端 API，部分方法是为 GUI 和脚本预留的接口：
//!
//! | 方法 | CLI 使用 | 预留给 GUI/脚本 |
//! |------|----------|-----------------|
//! | `ping()` | ✓ | |
//! | `status()` | ✓ | |
//! | `next()` / `prev()` | ✓ | |
//! | `set_wallpaper()` | ✓ | |
//! | `set_mode()` | ✓ | |
//! | `lock()` / `unlock()` | ✓ | |
//! | `config()` | ✓ | |
//! | `set_config()` | ✓ | |
//! | `rescan()` | ✓ | |
//! | `reload_config()` | ✓ | |
//! | `shutdown()` | ✓ | |
//! | `space()` | | ✓ (GUI 壁纸列表) |
//! | `time_info()` | | ✓ (GUI 时间调度) |
//! | `toggle_lock()` | | ✓ (GUI 一键切换) |
//! | `subscribe()` | 调试用 | ✓ (GUI 实时同步) |
//! | `receive_event()` | 调试用 | ✓ (GUI 事件循环) |
//! | `unsubscribe()` | | ✓ |
//!
//! 标记为 "预留给 GUI/脚本" 的方法在 CLI 中可能未使用，
//! 但它们是 Socket 协议的完整实现，供外部集成使用。
//!
//! # 示例
//!
//! ```ignore
//! use lianwall_cli::client::Client;
//!
//! let mut client = Client::connect("/tmp/lianwall.sock".as_ref())?;
//! client.next()?;  // 切换下一张壁纸
//! ```

#![allow(dead_code)] // 预留 API，供 GUI/脚本使用

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use lianwall_core::config::WallMode;
use lianwall_core::socket::{
    ConfigSnapshot, ErrorCode, Event, EventType, Request, Response, SpaceSnapshot,
    StatusInfo, TimeScheduleInfo, HookInfo,
};

/// 客户端错误
#[derive(Debug)]
pub enum ClientError {
    /// Daemon 未运行（socket 文件不存在或连接被拒绝）
    DaemonNotRunning,
    /// 连接超时
    ConnectionTimeout,
    /// IO 错误
    Io(std::io::Error),
    /// 编解码错误
    Codec(String),
    /// Daemon 返回错误
    DaemonError { code: ErrorCode, message: String },
    /// 意外的响应类型
    UnexpectedResponse(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DaemonNotRunning => {
                write!(f, "Daemon is not running. Start it with: lianwall start")
            }
            Self::ConnectionTimeout => write!(f, "Connection timeout"),
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Codec(s) => write!(f, "Codec error: {}", s),
            Self::DaemonError { code, message } => {
                write!(f, "Daemon error [{:?}]: {}", code, message)
            }
            Self::UnexpectedResponse(s) => write!(f, "Unexpected response: {}", s),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<std::io::Error> for ClientError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound
            || e.kind() == std::io::ErrorKind::ConnectionRefused
        {
            Self::DaemonNotRunning
        } else if e.kind() == std::io::ErrorKind::TimedOut {
            Self::ConnectionTimeout
        } else {
            Self::Io(e)
        }
    }
}

/// Socket 客户端
///
/// 使用行分隔 JSON 协议与 daemon 通信
pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Client {
    /// 连接到 daemon
    ///
    /// # Arguments
    /// * `socket_path` - Unix socket 路径（通常是 `/tmp/lianwall.sock`）
    ///
    /// # Errors
    /// * `ClientError::DaemonNotRunning` - socket 不存在或连接被拒绝
    /// * `ClientError::Io` - 其他 IO 错误
    pub fn connect(socket_path: &Path) -> Result<Self, ClientError> {
        let stream = UnixStream::connect(socket_path)?;

        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;

        let writer = stream.try_clone()?;
        let reader = BufReader::new(stream);

        Ok(Self { reader, writer })
    }

    /// 发送请求并接收响应（行分隔 JSON 协议）
    fn request(&mut self, req: Request) -> Result<Response, ClientError> {
        // 序列化并发送（行分隔）
        let json = serde_json::to_string(&req)
            .map_err(|e| ClientError::Codec(format!("Serialize error: {}", e)))?;
        writeln!(self.writer, "{}", json)?;
        self.writer.flush()?;

        // 读取响应（行分隔）
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        
        let resp: Response = serde_json::from_str(line.trim())
            .map_err(|e| ClientError::Codec(format!("Deserialize error: {}", e)))?;

        // 检查错误响应
        if let Response::Error { code, message } = &resp {
            return Err(ClientError::DaemonError {
                code: code.clone(),
                message: message.clone(),
            });
        }

        Ok(resp)
    }

    // ========================================================================
    // Query API - 无状态查询
    // ========================================================================

    /// 心跳检测，返回 daemon 运行时间（秒）
    pub fn ping(&mut self) -> Result<u64, ClientError> {
        match self.request(Request::Ping)? {
            Response::Pong { uptime_secs, .. } => Ok(uptime_secs),
            other => Err(unexpected_response(&other)),
        }
    }

    /// 获取 daemon 状态信息
    ///
    /// 包含：当前模式、当前壁纸、引擎状态、VRAM 状态、壁纸统计等
    pub fn status(&mut self) -> Result<StatusInfo, ClientError> {
        match self.request(Request::GetStatus)? {
            Response::Status(info) => Ok(info),
            other => Err(unexpected_response(&other)),
        }
    }

    /// 获取向量空间快照
    ///
    /// # Arguments
    /// * `mode` - `None` 表示当前模式，`Some(mode)` 表示指定模式
    pub fn space(&mut self, mode: Option<WallMode>) -> Result<SpaceSnapshot, ClientError> {
        match self.request(Request::GetSpace { mode })? {
            Response::Space(snap) => Ok(snap),
            other => Err(unexpected_response(&other)),
        }
    }

    /// 获取时间调度信息
    ///
    /// 包含：下次切换时间、时间点列表、各时间段壁纸数量等
    pub fn time_info(&mut self) -> Result<TimeScheduleInfo, ClientError> {
        match self.request(Request::GetTimeInfo)? {
            Response::TimeInfo(info) => Ok(info),
            other => Err(unexpected_response(&other)),
        }
    }

    /// 获取配置
    ///
    /// # Arguments
    /// * `key` - `None` 获取全部配置，`Some(key)` 获取指定字段
    ///
    /// # Key 格式
    /// - `"paths.mode"` - 获取 paths 节的 mode 字段
    /// - `"vram.threshold_percent"` - 获取 vram 节的 threshold_percent 字段
    /// - `"vram"` - 获取整个 vram 节
    pub fn config(&mut self, key: Option<String>) -> Result<ConfigSnapshot, ClientError> {
        match self.request(Request::GetConfig { key })? {
            Response::Config(snap) => Ok(snap),
            other => Err(unexpected_response(&other)),
        }
    }

    // ========================================================================
    // Command API - 状态修改（串行执行）
    // ========================================================================

    /// 切换到下一张壁纸
    ///
    /// 使用黄金角算法选择下一张壁纸
    pub fn next(&mut self) -> Result<(), ClientError> {
        self.request(Request::Next { trigger_hint: None })?;
        Ok(())
    }

    /// 切换到上一张壁纸
    ///
    /// 从历史栈弹出上一张壁纸
    pub fn prev(&mut self) -> Result<(), ClientError> {
        self.request(Request::Prev { trigger_hint: None })?;
        Ok(())
    }

    /// 设置指定壁纸
    ///
    /// # Arguments
    /// * `path` - 壁纸文件的绝对路径
    pub fn set_wallpaper(&mut self, path: PathBuf) -> Result<(), ClientError> {
        self.request(Request::SetWallpaper { path })?;
        Ok(())
    }

    /// 设置壁纸模式
    ///
    /// # Arguments
    /// * `mode` - `WallMode::Video` 或 `WallMode::Image`
    pub fn set_mode(&mut self, mode: WallMode) -> Result<(), ClientError> {
        self.request(Request::SetMode { mode })?;
        Ok(())
    }

    /// 锁定壁纸（从轮换中排除）
    ///
    /// 锁定的壁纸不会被自动选中，但可以手动 `set` 设置
    pub fn lock(&mut self, path: PathBuf) -> Result<(), ClientError> {
        self.request(Request::Lock { path })?;
        Ok(())
    }

    /// 解锁壁纸
    pub fn unlock(&mut self, path: PathBuf) -> Result<(), ClientError> {
        self.request(Request::Unlock { path })?;
        Ok(())
    }

    /// 切换壁纸锁定状态
    pub fn toggle_lock(&mut self, path: PathBuf) -> Result<(), ClientError> {
        self.request(Request::ToggleLock { path })?;
        Ok(())
    }

    /// 设置配置项
    ///
    /// # Arguments
    /// * `key` - 配置键（如 `"vram.threshold_percent"`）
    /// * `value` - 新值（JSON 格式）
    ///
    /// # 注意
    /// 修改会立即生效并持久化到配置文件
    pub fn set_config(
        &mut self,
        key: String,
        value: serde_json::Value,
    ) -> Result<(), ClientError> {
        self.request(Request::SetConfig { key, value })?;
        Ok(())
    }

    /// 重新扫描壁纸目录
    ///
    /// # 使用场景
    /// - 添加/删除了壁纸文件
    /// - 修改了壁纸文件的时间约束目录结构
    ///
    /// # 与 reload_config 的区别
    /// - `rescan`: 只重新扫描目录，不重新读取配置文件
    /// - `reload_config`: 重新读取配置文件，如果目录配置变了也会触发扫描
    pub fn rescan(&mut self) -> Result<(), ClientError> {
        self.request(Request::Rescan)?;
        Ok(())
    }

    /// 重新加载配置文件
    ///
    /// # 使用场景
    /// - 手动编辑了 config.toml 文件
    /// - 需要应用新的配置（如切换间隔、VRAM 阈值等）
    ///
    /// # 与 rescan 的区别
    /// - `reload_config`: 重新读取 config.toml，更新 daemon 的所有配置状态
    /// - `rescan`: 只重新扫描目录发现新壁纸，不读取配置文件
    ///
    /// # 注意
    /// 如果配置中的壁纸目录路径发生变化，reload_config 也会自动触发 rescan
    pub fn reload_config(&mut self) -> Result<(), ClientError> {
        self.request(Request::ReloadConfig)?;
        Ok(())
    }

    /// 重新加载 hooks.toml
    pub fn reload_hooks(&mut self) -> Result<(), ClientError> {
        self.request(Request::ReloadHooks)?;
        Ok(())
    }

    /// 列出当前 hook 配置
    pub fn list_hooks(&mut self) -> Result<Vec<HookInfo>, ClientError> {
        match self.request(Request::ListHooks)? {
            Response::HookList(hooks) => Ok(hooks),
            resp => Err(ClientError::UnexpectedResponse(format!("{:?}", resp))),
        }
    }

    /// 关闭守护进程
    pub fn shutdown(&mut self) -> Result<(), ClientError> {
        self.request(Request::Shutdown)?;
        Ok(())
    }

    // ========================================================================
    // Subscribe API - 订阅模式
    // ========================================================================

    /// 订阅事件
    ///
    /// # Arguments
    /// * `events` - 要订阅的事件类型列表
    /// * `immediate_sync` - 是否在订阅成功后立即推送当前状态
    ///
    /// # Returns
    /// 订阅成功返回会话 ID
    ///
    /// # 注意
    /// 订阅后连接会保持，使用 `receive_event` 接收事件
    pub fn subscribe(
        &mut self,
        events: Vec<EventType>,
        immediate_sync: bool,
    ) -> Result<String, ClientError> {
        match self.request(Request::Subscribe {
            events,
            immediate_sync,
        })? {
            Response::Subscribed { session_id, .. } => Ok(session_id),
            other => Err(unexpected_response(&other)),
        }
    }

    /// 接收事件（阻塞）
    ///
    /// # 注意
    /// 必须先调用 `subscribe` 建立订阅。
    /// 如果收到同步响应（如 immediate_sync 的 Status），会自动跳过并继续等待。
    pub fn receive_event(&mut self) -> Result<Event, ClientError> {
        loop {
            let mut line = String::new();
            self.reader.read_line(&mut line)?;
            
            let resp: Response = serde_json::from_str(line.trim())
                .map_err(|e| ClientError::Codec(format!("Deserialize error: {}", e)))?;

            match resp {
                Response::Event(event) => return Ok(event),
                Response::Error { code, message } => return Err(ClientError::DaemonError { code, message }),
                // immediate_sync 同步响应：跳过，继续等待真正的事件
                Response::Status(_) | Response::Space(_) | Response::Config(_) | Response::TimeInfo(_) => {
                    continue;
                }
                other => return Err(unexpected_response(&other)),
            }
        }
    }

    /// 接收原始响应（阻塞）
    ///
    /// 与 `receive_event` 不同，返回完整的 Response，不会跳过同步响应。
    /// 用于需要处理 immediate_sync 数据的场景。
    pub fn receive_response(&mut self) -> Result<Response, ClientError> {
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        
        let resp: Response = serde_json::from_str(line.trim())
            .map_err(|e| ClientError::Codec(format!("Deserialize error: {}", e)))?;
        
        Ok(resp)
    }

    /// 设置读取超时
    ///
    /// # Arguments
    /// * `timeout` - 超时时间，None 表示无限等待
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> Result<(), ClientError> {
        self.reader
            .get_ref()
            .set_read_timeout(timeout)
            .map_err(ClientError::Io)
    }

    /// 取消订阅
    pub fn unsubscribe(&mut self) -> Result<(), ClientError> {
        self.request(Request::Unsubscribe)?;
        Ok(())
    }

    /// 获取底层读取器（用于高级用途）
    pub fn into_reader(self) -> BufReader<UnixStream> {
        self.reader
    }
}

/// 检查 daemon 是否在运行（快速 ping）
///
/// # Arguments
/// * `socket_path` - Unix socket 路径
pub fn is_running(socket_path: &Path) -> bool {
    Client::connect(socket_path)
        .and_then(|mut c| c.ping())
        .is_ok()
}

fn unexpected_response(resp: &Response) -> ClientError {
    ClientError::UnexpectedResponse(format!("{:?}", resp))
}
