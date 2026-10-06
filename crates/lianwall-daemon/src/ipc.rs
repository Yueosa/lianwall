//! mpv IPC 客户端
//!
//! 通过 mpv 的 input-ipc-server Unix socket 发送换片命令,
//! 实现 mpvpaper 常驻进程热切换,避免每次切换的冷启动开销。

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, Context as _};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// IPC 操作超时
const IPC_TIMEOUT: Duration = Duration::from_secs(2);

/// loadfile 请求使用的 request_id（用于跳过 mpv 主动推送的 event 行）
const LOADFILE_REQUEST_ID: u64 = 1;

/// 向 mpv IPC socket 发送 loadfile 换片
///
/// 返回 Ok(()) 表示 mpv 接受换片,Err 表示调用方应回退到冷启动路径
pub async fn loadfile(socket_path: &str, video_path: &Path) -> anyhow::Result<()> {
    tokio::time::timeout(IPC_TIMEOUT, loadfile_inner(socket_path, video_path))
        .await
        .context("mpv IPC timed out")?
}

async fn loadfile_inner(socket_path: &str, video_path: &Path) -> anyhow::Result<()> {
    let stream = UnixStream::connect(socket_path).await?;
    let (reader, mut writer) = stream.into_split();

    let request = serde_json::json!({
        "command": ["loadfile", video_path.to_string_lossy(), "replace"],
        "request_id": LOADFILE_REQUEST_ID,
    });
    let mut payload = serde_json::to_vec(&request)?;
    payload.push(b'\n');
    writer.write_all(&payload).await?;

    // 逐行读响应,跳过 mpv 主动推送的 event 行,直到匹配 request_id
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        let Ok(resp) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if resp.get("request_id").and_then(|v| v.as_u64()) != Some(LOADFILE_REQUEST_ID) {
            continue;
        }
        return match resp.get("error").and_then(|v| v.as_str()) {
            Some("success") => Ok(()),
            other => Err(anyhow!("mpv loadfile failed: {:?}", other)),
        };
    }

    Err(anyhow!("mpv IPC connection closed before response"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_sock(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "lianwall-ipc-test-{}-{}.sock",
            name,
            std::process::id()
        ))
    }

    /// 启动一个假 mpv IPC 服务端:读掉请求行,然后返回指定响应(可选先推一条 event)
    async fn spawn_fake_mpv(path: &Path, event_line: Option<&str>, response: Option<String>) {
        let listener = tokio::net::UnixListener::bind(path).unwrap();
        let event_line = event_line.map(|s| s.to_string());
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            if let Some(ev) = event_line {
                let _ = writer.write_all(ev.as_bytes()).await;
                let _ = writer.write_all(b"\n").await;
            }
            // 读掉客户端的请求行
            let _ = lines.next_line().await;
            match response {
                Some(resp) => {
                    let _ = writer.write_all(resp.as_bytes()).await;
                    let _ = writer.write_all(b"\n").await;
                }
                None => {
                    // 不响应,保持连接让客户端超时
                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
            }
        });
    }

    #[tokio::test]
    async fn test_loadfile_success() {
        let sock = temp_sock("success");
        let _ = std::fs::remove_file(&sock);
        spawn_fake_mpv(
            &sock,
            Some(r#"{"event":"property-change"}"#),
            Some(r#"{"data":null,"request_id":1,"error":"success"}"#.to_string()),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let result = loadfile(sock.to_str().unwrap(), Path::new("/tmp/a.mp4")).await;
        assert!(result.is_ok());
        let _ = std::fs::remove_file(&sock);
    }

    #[tokio::test]
    async fn test_loadfile_error_response() {
        let sock = temp_sock("error");
        let _ = std::fs::remove_file(&sock);
        spawn_fake_mpv(
            &sock,
            None,
            Some(r#"{"data":null,"request_id":1,"error":"error running command"}"#.to_string()),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let result = loadfile(sock.to_str().unwrap(), Path::new("/tmp/a.mp4")).await;
        assert!(result.is_err());
        let _ = std::fs::remove_file(&sock);
    }

    #[tokio::test]
    async fn test_loadfile_timeout() {
        let sock = temp_sock("timeout");
        let _ = std::fs::remove_file(&sock);
        spawn_fake_mpv(&sock, None, None).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let result = loadfile(sock.to_str().unwrap(), Path::new("/tmp/a.mp4")).await;
        assert!(result.is_err());
        let _ = std::fs::remove_file(&sock);
    }

    #[tokio::test]
    async fn test_loadfile_socket_missing() {
        let result = loadfile("/tmp/lianwall-ipc-test-nonexistent.sock", Path::new("/tmp/a.mp4")).await;
        assert!(result.is_err());
    }
}
