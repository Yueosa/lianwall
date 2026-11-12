//! Exercise the actual daemon binary without wallpaper engines: both media dirs are empty.
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_case(signal: Option<i32>, duplicate: bool) {
    let directory = tempfile::tempdir().unwrap();
    let mut config: lianwall_core::config::Config =
        toml::from_str(lianwall_core::config::DEFAULT_CONFIG_TOML).unwrap();
    config.paths.video_dir = directory.path().join("videos");
    config.paths.image_dir = directory.path().join("images");
    std::fs::create_dir(&config.paths.video_dir).unwrap();
    std::fs::create_dir(&config.paths.image_dir).unwrap();
    config.vram.enabled = false;
    config.daemon.socket_path = directory.path().join("daemon.sock");
    let socket = config.daemon.socket_path.clone();
    let config_path = directory.path().join("lianwall/config.toml");
    lianwall_core::config::update(lianwall_core::config::ConfigUpdateInput {
        path: Some(config_path),
        config,
    })
    .unwrap();
    let marker = directory.path().join("cleanup");
    let mut command = format!(
        "printf done > '{}'",
        marker.to_string_lossy().replace('\'', "'\\''")
    );
    if duplicate {
        command = format!("sleep 0.3; {command}");
    }
    let mut hooks = lianwall_core::hook::HookConfig::default();
    hooks.hook.push(lianwall_core::hook::HookEntry {
        name: None,
        on: lianwall_core::hook::HookEvent::DaemonShutdown,
        command,
        mode: None,
        trigger: None,
        timeout: 1,
        enabled: true,
    });
    std::fs::write(
        directory.path().join("lianwall/hooks.toml"),
        toml::to_string(&hooks).unwrap(),
    )
    .unwrap();
    let mut daemon = Daemon(
        Command::new(env!("CARGO_BIN_EXE_lianwalld"))
            .env("XDG_CONFIG_HOME", directory.path())
            .env("XDG_CACHE_HOME", directory.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let start = Instant::now();
    let mut stream = loop {
        if let Ok(stream) = UnixStream::connect(&socket) {
            break stream;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "daemon did not start"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    if duplicate {
        let mut second = Daemon(
            Command::new(env!("CARGO_BIN_EXE_lianwalld"))
                .env("XDG_CONFIG_HOME", directory.path())
                .env("XDG_CACHE_HOME", directory.path())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let start = Instant::now();
        loop {
            if let Some(status) = second.0.try_wait().unwrap() {
                assert!(
                    !status.success(),
                    "duplicate daemon should fail before initializing tasks"
                );
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut probe =
            UnixStream::connect(&socket).expect("duplicate startup removed the original socket");
        probe
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        writeln!(probe, "{{\"id\":99,\"cmd\":\"Ping\"}}").unwrap();
        let mut reply = String::new();
        BufReader::new(probe).read_line(&mut reply).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&reply).unwrap()["type"],
            "Pong"
        );
    }
    if let Some(signal) = signal {
        // SAFETY: this pid belongs to the daemon child owned by this fixture.
        assert_eq!(unsafe { libc::kill(daemon.0.id() as i32, signal) }, 0);
    } else {
        writeln!(stream, "{{\"id\":1,\"cmd\":\"Shutdown\"}}").unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut response = String::new();
        BufReader::new(&stream).read_line(&mut response).unwrap();
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(response["type"], "Ok");
        assert_eq!(response["id"], 1);
        if duplicate {
            std::thread::sleep(Duration::from_millis(50));
            assert!(
                socket.exists(),
                "socket ownership must last through shutdown hooks"
            );
            let mut during_cleanup = Daemon(
                Command::new(env!("CARGO_BIN_EXE_lianwalld"))
                    .env("XDG_CONFIG_HOME", directory.path())
                    .env("XDG_CACHE_HOME", directory.path())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            let started = Instant::now();
            loop {
                if let Some(status) = during_cleanup.0.try_wait().unwrap() {
                    assert!(
                        !status.success(),
                        "new instance started before old cleanup finished"
                    );
                    break;
                }
                assert!(started.elapsed() < Duration::from_secs(2));
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
    let start = Instant::now();
    loop {
        if let Some(status) = daemon.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(7),
            "daemon did not shut down"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "done");
}

#[test]
fn socket_shutdown_finishes_cleanup_hook() {
    run_case(None, false);
}

#[test]
fn sigterm_finishes_cleanup_hook() {
    run_case(Some(libc::SIGTERM), false);
}

#[test]
fn sigint_finishes_cleanup_hook() {
    run_case(Some(libc::SIGINT), false);
}

#[test]
fn duplicate_start_keeps_the_original_daemon_reachable() {
    run_case(None, true);
}
