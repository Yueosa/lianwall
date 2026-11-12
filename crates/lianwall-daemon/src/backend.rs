//! External-engine boundary: prepare, verify, then replace owned processes.
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use lianwall_core::config::{VideoTransition, WallMode};
use lianwall_core::engine::detect_image_bin;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use crate::state::{EngineProcess, SharedState};

const DIAGNOSTIC_BYTES: usize = 4096;
type Diagnostics = Arc<Mutex<Vec<u8>>>;

fn drain(
    pipe: Option<impl AsyncRead + Unpin + Send + 'static>,
    diagnostics: Diagnostics,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Some(mut pipe) = pipe else { return };
        let mut chunk = [0u8; 1024];
        while let Ok(count) = pipe.read(&mut chunk).await {
            if count == 0 {
                break;
            }
            let mut tail = diagnostics
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            tail.extend_from_slice(&chunk[..count]);
            let excess = tail.len().saturating_sub(DIAGNOSTIC_BYTES);
            tail.drain(..excess);
        }
    })
}

fn diagnostic(tail: &Diagnostics) -> String {
    String::from_utf8_lossy(&tail.lock().unwrap_or_else(|poison| poison.into_inner()))
        .trim()
        .to_string()
}

async fn short(command: &mut Command, budget: Duration) -> anyhow::Result<(bool, String)> {
    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut process = EngineProcess::spawn(command)?;
    let tail = Diagnostics::default();
    let output_task = drain(process.child.stderr.take(), tail.clone());
    let result = tokio::time::timeout(budget, process.child.wait()).await;
    match result {
        Ok(Ok(status)) => {
            process.stop().await;
            let _ = tokio::time::timeout(Duration::from_millis(200), output_task).await;
            if !status.success() {
                tracing::debug!(%status, stderr = %diagnostic(&tail), "Engine command rejected");
            }
            Ok((status.success(), diagnostic(&tail)))
        }
        Ok(Err(error)) => Err(error.into()),
        Err(_) => {
            process.stop().await;
            bail!(
                "Engine command timed out after {budget:?}: {}",
                diagnostic(&tail)
            );
        }
    }
}

fn namespace(bin: &str) -> Option<String> {
    (bin == "awww").then(|| format!("lianwall-{}", std::process::id()))
}

fn image_command(bin: &str, action: &str) -> Command {
    let mut command = Command::new(bin);
    command.arg(action);
    if let Some(namespace) = namespace(bin) {
        command.arg("--namespace").arg(namespace);
    }
    command
}

/// Each mpv_args entry is one option, and values are quoted for mpvpaper's config parser.
fn mpv_options(arguments: &[String]) -> anyhow::Result<String> {
    arguments
        .iter()
        .map(|argument| {
            if argument.contains(['\n', '\r', '\0']) {
                bail!("mpv_args entries must not contain newlines or NUL");
            }
            if let Some((name, value)) = argument.split_once('=') {
                if name.chars().any(char::is_whitespace) {
                    bail!("Invalid mpv option name: {name}");
                }
                Ok(format!(
                    "{name}=\"{}\"",
                    value.replace('\\', "\\\\").replace('"', "\\\"")
                ))
            } else {
                if argument.chars().any(char::is_whitespace) {
                    bail!("Use option=value for each mpv_args entry: {argument}");
                }
                Ok(argument.clone())
            }
        })
        .collect::<anyhow::Result<Vec<_>>>()
        .map(|options| options.join(" "))
}

fn has_option(arguments: &[String], name: &str) -> bool {
    arguments
        .iter()
        .any(|argument| argument.trim_start_matches('-').split('=').next() == Some(name))
}

async fn video(state: &SharedState, path: &Path) -> anyhow::Result<()> {
    let config = state.get_config().await.video_engine;
    if config.mpvpaper_args.iter().any(|arg| {
        arg == "--fork"
            || (!arg.starts_with("--") && arg.starts_with('-') && arg[1..].contains('f'))
    }) {
        bail!("mpvpaper --fork/-f cannot be used: lianwalld must own the foreground process");
    }
    let mut options = config.mpv_args.clone();
    if !config.ipc_socket.is_empty() {
        let expected = format!("input-ipc-server={}", config.ipc_socket);
        if let Some(actual) = options.iter().find(|option| {
            option.trim_start_matches('-').split('=').next() == Some("input-ipc-server")
        }) {
            if actual.trim_start_matches('-') != expected {
                bail!("video_engine.ipc_socket conflicts with mpv_args input-ipc-server");
            }
        } else {
            options.push(format!("--{expected}"));
        }
    }
    if config.transition == VideoTransition::Fade && !has_option(&options, "vf") {
        options.push("--vf=lavfi=[fade=t=in:st=0:d=0.4]".into());
    }
    let manual_socket = options.iter().find_map(|argument| {
        argument
            .trim_start_matches('-')
            .strip_prefix("input-ipc-server=")
            .map(std::path::PathBuf::from)
    });
    let persistent_socket = if !config.ipc_socket.is_empty() {
        Some(std::path::PathBuf::from(&config.ipc_socket))
    } else {
        manual_socket
    };
    let private_directory = if persistent_socket.is_none() {
        Some(tempfile::Builder::new().prefix("lianwall-mpv-").tempdir()?)
    } else {
        None
    };
    let control_socket = persistent_socket.clone().unwrap_or_else(|| {
        private_directory
            .as_ref()
            .unwrap()
            .path()
            .join("control.sock")
    });
    if persistent_socket.is_none() {
        options.push(format!("--input-ipc-server={}", control_socket.display()));
    }
    let options = mpv_options(&options)?;
    if persistent_socket.is_some() {
        let owns_socket = state
            .engine
            .owned_ipc
            .lock()
            .await
            .as_ref()
            .is_some_and(|(path, _, _)| path == &control_socket);
        if !config.ipc_socket.is_empty()
            && owns_socket
            && state.engine.mpvpaper.is_running().await
            && crate::ipc::loadfile(&config.ipc_socket, path).await.is_ok()
        {
            state.engine.swww_daemon.kill().await;
            return Ok(());
        }
        // A shared, user-configured address must be released before rebinding.
        state.engine.mpvpaper.kill().await;
        cleanup_ipc(state).await;
        if std::fs::symlink_metadata(&control_socket).is_ok() {
            bail!(
                "IPC path already exists and is not owned by this daemon: {}",
                control_socket.display()
            );
        }
    }
    let mut command = Command::new("mpvpaper");
    command.args(&config.mpvpaper_args);
    if !options.is_empty() {
        command.arg("-o").arg(options);
    }
    command
        .arg("--")
        .arg(&config.display)
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut replacement = EngineProcess::spawn(&mut command).context("Failed to start mpvpaper")?;
    let tail = Diagnostics::default();
    drain(replacement.child.stdout.take(), tail.clone());
    drain(replacement.child.stderr.take(), tail.clone());
    replacement.ipc_directory = private_directory;
    let started = Instant::now();
    loop {
        if persistent_socket.is_some() {
            record_ipc(state, &control_socket).await;
        }
        if let Some(status) = replacement.child.try_wait()? {
            if persistent_socket.is_some() {
                cleanup_ipc(state).await;
            }
            bail!(
                "mpvpaper exited during startup ({status}): {}",
                diagnostic(&tail)
            );
        }
        if crate::ipc::video_ready(&control_socket).await {
            break;
        }
        if started.elapsed() >= Duration::from_secs(5) {
            if persistent_socket.is_some() {
                cleanup_ipc(state).await;
            }
            bail!(
                "mpvpaper did not report decoded video parameters: {}",
                diagnostic(&tail)
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    state.engine.mpvpaper.set(replacement).await;
    if persistent_socket.is_none() {
        cleanup_ipc(state).await;
    }
    state.engine.swww_daemon.kill().await;
    Ok(())
}

async fn image(state: &SharedState, path: &Path) -> anyhow::Result<()> {
    let config = state.get_config().await.image_engine;
    if config.awww_args.iter().any(|arg| {
        matches!(arg.as_str(), "--all" | "-a" | "--namespace" | "-n")
            || arg.starts_with("--namespace=")
    }) {
        bail!("awww_args must not override the namespace owned by lianwalld");
    }
    let running = state.engine.swww_daemon.is_running().await;
    let bin = if running {
        state.engine.image_bin.read().await.clone()
    } else {
        None
    };
    let bin = match bin {
        Some(bin) => bin,
        None => detect_image_bin()
            .await
            .context("Install awww and awww-daemon (or the matching legacy swww pair) in PATH")?,
    };
    let mut pending = None;
    if !running {
        // Modern awww is isolated. Legacy swww has no namespace: do not kill another user's engine.
        if namespace(&bin).is_none()
            && short(&mut image_command(&bin, "query"), Duration::from_secs(1))
                .await
                .map(|(success, _)| success)
                .unwrap_or(false)
        {
            bail!(
                "An external swww-daemon is already running; stop it before using the legacy backend"
            );
        }
        let mut command = Command::new(format!("{bin}-daemon"));
        if let Some(namespace) = namespace(&bin) {
            command.arg("--namespace").arg(namespace);
        }
        command.stdout(Stdio::null()).stderr(Stdio::piped());
        let mut process = EngineProcess::spawn(&mut command)?;
        let tail = Diagnostics::default();
        drain(process.child.stderr.take(), tail.clone());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = process.child.try_wait()? {
                bail!(
                    "{bin}-daemon exited during startup ({status}): {}",
                    diagnostic(&tail)
                );
            }
            if short(
                &mut image_command(&bin, "query"),
                Duration::from_millis(500),
            )
            .await
            .map(|(success, _)| success)
            .unwrap_or(false)
            {
                break;
            }
            if Instant::now() >= deadline {
                bail!("{bin}-daemon did not become ready: {}", diagnostic(&tail));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        pending = Some(process);
    }
    let mut command = image_command(&bin, "img");
    if !config.outputs.is_empty() {
        command.arg("--outputs").arg(config.outputs);
    }
    command.args(&config.awww_args).arg("--").arg(path);
    let (success, stderr) = short(&mut command, Duration::from_secs(15)).await?;
    if !success {
        bail!("{bin} img failed: {stderr}");
    }
    if let Some(process) = pending {
        state.engine.swww_daemon.set(process).await;
        *state.engine.image_bin.write().await = Some(bin);
    }
    state.engine.mpvpaper.kill().await;
    cleanup_ipc(state).await;
    Ok(())
}

pub async fn apply(state: &SharedState, path: &Path, mode: WallMode) -> anyhow::Result<()> {
    let path = path
        .canonicalize()
        .with_context(|| format!("Wallpaper file is unavailable: {}", path.display()))?;
    if !path.is_file() {
        bail!("Wallpaper path is not a regular file: {}", path.display());
    }
    match mode {
        WallMode::Video => video(state, &path).await,
        WallMode::Image => image(state, &path).await,
    }
}

async fn record_ipc(state: &SharedState, path: &Path) {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    if let Ok(metadata) = std::fs::symlink_metadata(path)
        && metadata.file_type().is_socket()
    {
        *state.engine.owned_ipc.lock().await =
            Some((path.to_path_buf(), metadata.dev(), metadata.ino()));
    }
}

/// Never delete arbitrary files, symlinks or another process's replacement IPC socket.
pub async fn cleanup_ipc(state: &SharedState) {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    if let Some((path, device, inode)) = state.engine.owned_ipc.lock().await.take()
        && let Ok(metadata) = std::fs::symlink_metadata(&path)
        && metadata.file_type().is_socket()
        && metadata.dev() == device
        && metadata.ino() == inode
    {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn ipc_cleanup_preserves_user_files_and_replacement_sockets() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mpv.sock");
        let config = toml::from_str(lianwall_core::config::DEFAULT_CONFIG_TOML).unwrap();
        let state = SharedState::init(config).await.unwrap();
        std::fs::write(&path, "user data").unwrap();
        record_ipc(&state, &path).await;
        cleanup_ipc(&state).await;
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "user data");
        std::fs::remove_file(&path).unwrap();
        let owned = std::os::unix::net::UnixListener::bind(&path).unwrap();
        record_ipc(&state, &path).await;
        std::fs::remove_file(&path).unwrap();
        let replacement = std::os::unix::net::UnixListener::bind(&path).unwrap();
        cleanup_ipc(&state).await;
        assert!(path.exists());
        record_ipc(&state, &path).await;
        cleanup_ipc(&state).await;
        assert!(!path.exists());
        drop(owned);
        drop(replacement);
    }

    #[test]
    fn mpv_options_preserve_spaces_quotes_and_exact_option_names() {
        assert_eq!(
            mpv_options(&["--script=/tmp/a b.lua".into()]).unwrap(),
            "--script=\"/tmp/a b.lua\""
        );
        assert!(mpv_options(&["--title=a\nb".into()]).is_err());
        assert!(!has_option(&["--script=/tmp/vf.lua".into()], "vf"));
        assert!(has_option(&["--vf=lavfi=[]".into()], "vf"));
    }
}
