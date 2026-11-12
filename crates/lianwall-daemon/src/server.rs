//! Socket Server - Unix Socket 服务器
//!
//! 负责：
//! - 监听 Unix Socket
//! - 接受新连接
//! - 为每个连接 spawn Connection Task

use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixListener;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::command::CommandMsg;
use crate::connection;
use crate::event::EventBus;
use crate::state::SharedState;

/// 运行 Socket 服务器
pub async fn run(
    state: Arc<SharedState>,
    event_bus: EventBus,
    cmd_tx: mpsc::Sender<CommandMsg>,
) -> anyhow::Result<()> {
    let config = state.get_config().await;
    run_bound(bind(&config.daemon.socket_path)?, state, event_bus, cmd_tx).await
}

/// Bind before initializing hooks, engines or background tasks.
pub struct BoundSocket {
    listener: UnixListener,
    _binding: Arc<SocketBinding>,
}

impl BoundSocket {
    /// Keep the listening path and lock owned until all daemon cleanup has finished.
    pub fn keep_alive(&self) -> Arc<SocketBinding> {
        self._binding.clone()
    }
}

pub fn bind(path: &std::path::Path) -> std::io::Result<BoundSocket> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let (listener, binding) = SocketBinding::bind(path)?;
    Ok(BoundSocket {
        listener,
        _binding: Arc::new(binding),
    })
}

pub async fn run_bound(
    socket: BoundSocket,
    state: Arc<SharedState>,
    event_bus: EventBus,
    cmd_tx: mpsc::Sender<CommandMsg>,
) -> anyhow::Result<()> {
    if state.is_shutting_down() {
        return Ok(());
    }
    let mut connections = JoinSet::new();
    let listener = &socket.listener;
    tracing::info!("Server listening on {:?}", socket._binding.path);

    // 连接 ID 计数器
    let mut conn_id: u64 = 0;

    // 获取 shutdown 信号
    let mut shutdown_rx = state.shutdown_receiver();

    while !state.is_shutting_down() {
        tokio::select! {
            result = connections.join_next(), if !connections.is_empty() => {
                if let Some(Err(error)) = result { tracing::warn!("Connection task failed: {}", error); }
            }
            // 接受新连接
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        conn_id += 1;
                        let id = conn_id;

                        tracing::debug!("New connection: #{}", id);

                        // Spawn connection task
                        let state = Arc::clone(&state);
                        let event_bus = event_bus.clone();
                        let cmd_tx = cmd_tx.clone();

                        connections.spawn(async move {
                            if let Err(e) = connection::handle(id, stream, state, event_bus, cmd_tx).await {
                                tracing::warn!("Connection #{} error: {}", id, e);
                            }
                            tracing::debug!("Connection #{} closed", id);
                        });
                    }
                    Err(e) => {
                        tracing::error!("Accept error: {}", e);
                    }
                }
            }

            // 收到关闭信号
            _ = shutdown_rx.recv() => {
                tracing::info!("Server shutting down");
                break;
            }
        }
    }

    // Connections stop reading on shutdown but flush their pending acknowledgments.
    // A stalled peer or command must not hold the daemon open indefinitely.
    let _ = tokio::time::timeout(Duration::from_secs(1), async {
        while connections.join_next().await.is_some() {}
    })
    .await;

    Ok(())
}

/// A persistent lock inode prevents concurrent startups; never unlink the lock file.
pub struct SocketBinding {
    path: std::path::PathBuf,
    identity: (u64, u64),
    _lock: std::fs::File,
}

impl SocketBinding {
    fn bind(path: &std::path::Path) -> std::io::Result<(UnixListener, Self)> {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
        let mut lock_path = path.as_os_str().to_os_string();
        lock_path.push(".lock");
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(lock_path)?;
        if !lock.metadata()?.is_file() {
            return Err(std::io::Error::other("Socket lock is not a regular file"));
        }
        // SAFETY: lock owns a valid descriptor and flock does not retain Rust pointers.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "Another daemon holds the socket lock",
            ));
        }
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                if !metadata.file_type().is_socket() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        "Socket path is not a socket; refusing to delete it",
                    ));
                }
                let probe =
                    socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None)?;
                probe.set_nonblocking(true)?;
                match probe.connect(&socket2::SockAddr::unix(path)?) {
                    Ok(_) => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::AddrInUse,
                            "Socket already has a listener",
                        ));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                        std::fs::remove_file(path)?;
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
        let listener = UnixListener::bind(path)?;
        let metadata = std::fs::symlink_metadata(path)?;
        Ok((
            listener,
            Self {
                path: path.to_path_buf(),
                identity: (metadata.dev(), metadata.ino()),
                _lock: lock,
            },
        ))
    }
}

impl Drop for SocketBinding {
    fn drop(&mut self) {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        if let Ok(metadata) = std::fs::symlink_metadata(&self.path)
            && metadata.file_type().is_socket()
            && (metadata.dev(), metadata.ino()) == self.identity
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[tokio::test]
    async fn duplicate_start_preserves_live_socket_and_releases_lock_after_exit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.sock");
        let (listener, binding) = SocketBinding::bind(&path).unwrap();
        assert!(SocketBinding::bind(&path).is_err());
        let _client = tokio::net::UnixStream::connect(&path).await.unwrap();
        assert!(listener.accept().await.is_ok());
        drop(listener);
        drop(binding);
        assert!(!path.exists());
        assert!(path.with_file_name("daemon.sock.lock").exists());
        assert!(SocketBinding::bind(&path).is_ok());
    }

    #[tokio::test]
    async fn recovers_stale_socket_but_preserves_external_listener_files_and_links() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.sock");
        drop(UnixListener::bind(&path).unwrap());
        let (listener, binding) = SocketBinding::bind(&path).unwrap();
        drop(listener);
        drop(binding);
        let external = UnixListener::bind(&path).unwrap();
        assert!(SocketBinding::bind(&path).is_err());
        assert!(path.exists());
        drop(external);
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, "user data").unwrap();
        assert!(SocketBinding::bind(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "user data");
        std::fs::remove_file(&path).unwrap();
        symlink(dir.path().join("missing"), &path).unwrap();
        assert!(SocketBinding::bind(&path).is_err());
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[tokio::test]
    async fn cleanup_does_not_remove_a_replacement_socket() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.sock");
        let (listener, binding) = SocketBinding::bind(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let replacement = UnixListener::bind(&path).unwrap();
        drop(binding);
        assert!(path.exists());
        drop(listener);
        drop(replacement);
    }
}
