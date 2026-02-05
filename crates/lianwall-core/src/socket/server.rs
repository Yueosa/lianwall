//! Socket 服务端
//!
//! 提供守护进程监听客户端连接的功能

use std::fs;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use super::codec::{recv_json, send_json};
use super::error::SocketError;
use super::protocol::{Request, Response};

/// Socket 服务端
pub struct Server {
    listener: UnixListener,
    socket_path: PathBuf,
}

impl Server {
    /// 绑定 Unix Socket
    ///
    /// # Arguments
    /// * `socket_path` - Socket 文件路径
    /// * `force` - 如果 socket 文件已存在，是否强制删除
    ///
    /// # Example
    /// ```ignore
    /// let server = Server::bind("/tmp/lianwall.sock", true)?;
    /// ```
    pub fn bind(socket_path: impl AsRef<Path>, force: bool) -> Result<Self, SocketError> {
        let path = socket_path.as_ref();

        // 如果 socket 文件已存在
        if path.exists() {
            if force {
                // 强制模式：删除旧文件
                fs::remove_file(path).map_err(|e| SocketError::BindFailed {
                    path: path.to_path_buf(),
                    source: e,
                })?;
            } else {
                // 非强制模式：检查是否有进程在监听
                if UnixStream::connect(path).is_ok() {
                    return Err(SocketError::SocketExists {
                        path: path.to_path_buf(),
                    });
                }
                // 没有进程监听，删除残留文件
                let _ = fs::remove_file(path);
            }
        }

        // 确保父目录存在
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| SocketError::BindFailed {
                path: path.to_path_buf(),
                source: e,
            })?;
        }

        // 绑定
        let listener = UnixListener::bind(path).map_err(|e| SocketError::BindFailed {
            path: path.to_path_buf(),
            source: e,
        })?;

        Ok(Self {
            listener,
            socket_path: path.to_path_buf(),
        })
    }

    /// 获取 socket 路径
    pub fn path(&self) -> &Path {
        &self.socket_path
    }

    /// 接受一个连接
    ///
    /// 这是阻塞调用，直到有客户端连接
    pub fn accept(&self) -> Result<Connection, SocketError> {
        let (stream, _addr) = self
            .listener
            .accept()
            .map_err(|e| SocketError::RecvFailed(e))?;

        Ok(Connection { stream })
    }

    /// 设置非阻塞模式
    pub fn set_nonblocking(&self, nonblocking: bool) -> Result<(), SocketError> {
        self.listener
            .set_nonblocking(nonblocking)
            .map_err(SocketError::SendFailed)
    }

    /// 获取底层 listener（用于 poll/epoll）
    pub fn as_listener(&self) -> &UnixListener {
        &self.listener
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // 清理 socket 文件
        let _ = fs::remove_file(&self.socket_path);
    }
}

/// 单个客户端连接
pub struct Connection {
    stream: UnixStream,
}

impl Connection {
    /// 接收请求
    pub fn recv_request(&mut self) -> Result<Request, SocketError> {
        recv_json(&mut self.stream)
    }

    /// 发送响应
    pub fn send_response(&mut self, response: &Response) -> Result<(), SocketError> {
        send_json(&mut self.stream, response)
    }

    /// 处理单个请求-响应周期
    ///
    /// # Arguments
    /// * `handler` - 处理函数，接收 Request 返回 Response
    pub fn handle<F>(&mut self, handler: F) -> Result<(), SocketError>
    where
        F: FnOnce(Request) -> Response,
    {
        let request = self.recv_request()?;
        let response = handler(request);
        self.send_response(&response)
    }

    /// 循环处理请求直到连接关闭
    ///
    /// # Arguments
    /// * `handler` - 处理函数，返回 (Response, 是否继续)
    pub fn serve<F>(&mut self, mut handler: F) -> Result<(), SocketError>
    where
        F: FnMut(Request) -> (Response, bool),
    {
        loop {
            let request = match self.recv_request() {
                Ok(req) => req,
                Err(SocketError::ConnectionClosed) => break,
                Err(e) => return Err(e),
            };

            let (response, should_continue) = handler(request);
            self.send_response(&response)?;

            if !should_continue {
                break;
            }
        }

        Ok(())
    }

    /// 设置读写超时
    pub fn set_timeout(&self, timeout: Option<std::time::Duration>) -> Result<(), SocketError> {
        self.stream
            .set_read_timeout(timeout)
            .map_err(SocketError::SendFailed)?;
        self.stream
            .set_write_timeout(timeout)
            .map_err(SocketError::SendFailed)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_bind_and_cleanup() {
        let socket_path = "/tmp/lianwall_test_server.sock";

        // 清理可能存在的旧文件
        let _ = fs::remove_file(socket_path);

        {
            let server = Server::bind(socket_path, false).unwrap();
            assert!(Path::new(socket_path).exists());
            assert_eq!(server.path(), Path::new(socket_path));
        }

        // Drop 后文件应该被清理
        assert!(!Path::new(socket_path).exists());
    }

    #[test]
    fn test_client_server_roundtrip() {
        use super::super::protocol::{Response, ResponseData};

        let socket_path = "/tmp/lianwall_test_roundtrip.sock";
        let _ = fs::remove_file(socket_path);

        // 启动服务端线程
        let server_handle = thread::spawn(move || {
            let server = Server::bind(socket_path, false).unwrap();
            let mut conn = server.accept().unwrap();

            conn.handle(|req| {
                assert!(matches!(req, Request::Ping));
                Response::with_data(ResponseData::Pong)
            })
            .unwrap();
        });

        // 等待服务端启动
        thread::sleep(std::time::Duration::from_millis(100));

        // 客户端连接
        use super::super::client::Client;
        let mut client = Client::connect(socket_path).unwrap();
        assert!(client.ping().unwrap());

        server_handle.join().unwrap();
    }
}
