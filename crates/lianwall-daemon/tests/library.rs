//! Production startup, durable favorites and opt-in events in private XDG directories.
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn request(stream: &mut BufReader<UnixStream>, value: Value) -> Value {
    let id = value["id"].clone();
    writeln!(stream.get_mut(), "{value}").unwrap();
    loop {
        let result = read(stream);
        if result["id"] == id {
            return result;
        }
    }
}
fn read(stream: &mut BufReader<UnixStream>) -> Value {
    let mut line = String::new();
    assert!(stream.read_line(&mut line).unwrap() > 0);
    serde_json::from_str(&line).unwrap()
}
fn start(root: &std::path::Path) -> (Daemon, BufReader<UnixStream>) {
    start_with_log(root, None)
}
fn start_with_log(
    root: &std::path::Path,
    override_filter: Option<&str>,
) -> (Daemon, BufReader<UnixStream>) {
    let mut daemon = Daemon(
        Command::new(env!("CARGO_BIN_EXE_lianwalld"))
            .env("RUST_LOG", override_filter.unwrap_or(""))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_STATE_HOME", root.join("state"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(stream) = UnixStream::connect(root.join("daemon.sock")) {
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            return (daemon, BufReader::new(stream));
        }
        assert!(
            daemon.0.try_wait().unwrap().is_none(),
            "daemon exited during startup"
        );
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn stop(daemon: &mut Daemon, stream: &mut BufReader<UnixStream>) {
    assert_eq!(
        request(stream, json!({"id":99,"cmd":"Shutdown"}))["type"],
        "Ok"
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = daemon.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn favorites_survive_real_daemon_restart_and_push_only_after_save() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut config: lianwall_core::config::Config =
        toml::from_str(lianwall_core::config::DEFAULT_CONFIG_TOML).unwrap();
    config.paths.video_dir = root.join("videos");
    config.paths.image_dir = root.join("images");
    config.vram.enabled = false;
    config.daemon.socket_path = root.join("daemon.sock");
    std::fs::create_dir(&config.paths.video_dir).unwrap();
    std::fs::create_dir(&config.paths.image_dir).unwrap();
    lianwall_core::config::update(lianwall_core::config::ConfigUpdateInput {
        path: Some(root.join("config/lianwall/config.toml")),
        config,
    })
    .unwrap();
    let index = root.join("data/lianwall/library.json");
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    std::fs::write(&index,json!({"schema_version":1,"entries":[{"path":"/retained missing # 图片.jpg","favorite":false,"added_at":100,"last_used":200,"play_count":4,"display_seconds":120}]}).to_string()).unwrap();
    let (mut daemon, mut stream) = start(root);
    let subscription = UnixStream::connect(root.join("daemon.sock")).unwrap();
    subscription
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut subscriber = BufReader::new(subscription);
    assert_eq!(
        request(
            &mut subscriber,
            json!({"id":1,"cmd":"Subscribe","events":["all","library_changed"],"immediate_sync":false})
        )["type"],
        "Subscribed"
    );
    assert_eq!(
        request(
            &mut stream,
            json!({"id":2,"cmd":"SetFavorite","path":"/retained missing # 图片.jpg","favorite":true})
        )["type"],
        "Ok"
    );
    let pushed = read(&mut subscriber);
    assert_eq!(pushed["payload"]["event"], "LibraryChanged");
    assert_eq!(pushed["payload"]["data"]["favorite"], true);
    let saved: Value = serde_json::from_slice(&std::fs::read(&index).unwrap()).unwrap();
    assert_eq!(saved["entries"][0]["favorite"], true);
    stop(&mut daemon, &mut stream);
    let (mut daemon, mut stream) = start(root);
    let snapshot = request(&mut stream, json!({"id":3,"cmd":"GetLibrary"}));
    assert_eq!(snapshot["type"], "Library");
    assert_eq!(snapshot["payload"]["entries"][0]["favorite"], true);
    assert_eq!(snapshot["payload"]["entries"][0]["play_count"], 4);
    assert_eq!(snapshot["payload"]["entries"][0]["display_seconds"], 120);
    std::fs::remove_file(&index).unwrap();
    std::fs::create_dir(&index).unwrap();
    assert_eq!(
        request(
            &mut stream,
            json!({"id":4,"cmd":"SetFavorite","path":"/retained missing # 图片.jpg","favorite":false})
        )["type"],
        "Error"
    );
    let snapshot = request(&mut stream, json!({"id":5,"cmd":"GetLibrary","limit":0}));
    assert_eq!(snapshot["payload"]["entries"][0]["favorite"], true);
    assert!(snapshot["payload"]["persistence_error"].is_string());
    stop(&mut daemon, &mut stream);
}

#[test]
fn diagnostics_are_responsive_read_only_and_log_levels_apply_without_restart() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let mut config: lianwall_core::config::Config =
        toml::from_str(lianwall_core::config::DEFAULT_CONFIG_TOML).unwrap();
    config.paths.video_dir = root.join("videos");
    config.paths.image_dir = root.join("images");
    config.vram.enabled = false;
    config.daemon.socket_path = root.join("daemon.sock");
    std::fs::create_dir(&config.paths.video_dir).unwrap();
    std::fs::create_dir(&config.paths.image_dir).unwrap();
    lianwall_core::config::update(lianwall_core::config::ConfigUpdateInput {
        path: Some(root.join("config/lianwall/config.toml")),
        config,
    })
    .unwrap();
    let (mut daemon, mut stream) = start(root);
    let before = request(&mut stream, json!({"id":1,"cmd":"GetStatus"}));
    writeln!(stream.get_mut(),"{}",json!({"id":2,"cmd":"TestVram","backend":"custom","command":"sleep 0.5; printf 'used_mb=123\ntotal_mb=1024\n'"})).unwrap();
    writeln!(stream.get_mut(), "{}", json!({"id":3,"cmd":"Ping"})).unwrap();
    let fast = read(&mut stream);
    assert_eq!(fast["id"], 3);
    assert_eq!(fast["type"], "Pong");
    let probe = read(&mut stream);
    assert_eq!(probe["id"], 2);
    assert_eq!(probe["payload"]["success"], true);
    assert_eq!(probe["payload"]["used_mb"], 123);
    let after = request(&mut stream, json!({"id":4,"cmd":"GetStatus"}));
    for key in [
        "mode",
        "current",
        "vram_used_mb",
        "vram_total_mb",
        "vram_override",
    ] {
        assert_eq!(before["payload"][key], after["payload"][key]);
    }
    assert_eq!(
        request(
            &mut stream,
            json!({"id":5,"cmd":"SetConfig","key":"daemon.log_level","value":"error"})
        )["type"],
        "Ok"
    );
    let logs = request(&mut stream, json!({"id":6,"cmd":"GetLogs"}));
    assert!(!logs["payload"]["environment_override"].as_bool().unwrap());
    assert!(
        logs["payload"]["active_filter"]
            .as_str()
            .unwrap()
            .contains("lianwall_daemon=error")
    );
    let sequence = logs["payload"]["entries"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["sequence"]
        .as_u64()
        .unwrap();
    request(
        &mut stream,
        json!({"id":7,"cmd":"SetConfig","key":"algorithm.temperature","value":0.6}),
    );
    let quiet = request(&mut stream, json!({"id":8,"cmd":"GetLogs"}));
    assert!(
        quiet["payload"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["sequence"].as_u64().unwrap() <= sequence)
    );
    request(
        &mut stream,
        json!({"id":9,"cmd":"SetConfig","key":"daemon.log_level","value":"info"}),
    );
    request(
        &mut stream,
        json!({"id":10,"cmd":"SetConfig","key":"algorithm.temperature","value":0.7}),
    );
    let visible = request(&mut stream, json!({"id":11,"cmd":"GetLogs"}));
    assert!(
        visible["payload"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["sequence"].as_u64().unwrap() > sequence
                && entry["line"]
                    .as_str()
                    .unwrap()
                    .contains("algorithm.temperature"))
    );
    let log_path = root.join("state/lianwall/daemon.jsonl");
    assert_eq!(visible["payload"]["path"], json!(log_path));
    stop(&mut daemon, &mut stream);
    assert!(
        std::fs::read_to_string(&log_path)
            .unwrap()
            .contains("algorithm.temperature")
    );
    let (mut daemon, mut stream) = start_with_log(
        root,
        Some("lianwalld=error,lianwall_daemon=error,lianwall_core=error"),
    );
    request(
        &mut stream,
        json!({"id":12,"cmd":"SetConfig","key":"daemon.log_level","value":"debug"}),
    );
    let overridden = request(&mut stream, json!({"id":13,"cmd":"GetLogs"}));
    assert_eq!(overridden["payload"]["environment_override"], true);
    assert!(
        overridden["payload"]["active_filter"]
            .as_str()
            .unwrap()
            .contains("lianwall_daemon=error")
    );
    assert!(
        overridden["payload"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["line"]
                .as_str()
                .unwrap()
                .contains("algorithm.temperature"))
    );
    stop(&mut daemon, &mut stream);
}
