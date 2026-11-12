//! Actual CLI subprocesses, isolated configuration and Unix-socket fixtures.
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

struct Fixture {
    directory: tempfile::TempDir,
    socket: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("daemon.sock");
        let mut config: lianwall_core::config::Config =
            toml::from_str(lianwall_core::config::DEFAULT_CONFIG_TOML).unwrap();
        config.daemon.socket_path = socket.clone();
        lianwall_core::config::update(lianwall_core::config::ConfigUpdateInput {
            path: Some(directory.path().join("lianwall/config.toml")),
            config,
        })
        .unwrap();
        Self { directory, socket }
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_lianwall"));
        command
            .args(args)
            .env("XDG_CONFIG_HOME", self.directory.path());
        command
    }
    fn output(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    fn json(&self, args: &[&str]) -> Value {
        let output = self.output(args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
    fn listener(&self) -> UnixListener {
        let listener = UnixListener::bind(&self.socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        listener
    }
}
fn accept(listener: &UnixListener) -> (BufReader<UnixStream>, UnixStream) {
    let start = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                return (BufReader::new(stream.try_clone().unwrap()), stream);
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    && start.elapsed() < Duration::from_secs(3) =>
            {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(error) => panic!("fixture accept failed: {error}"),
        }
    }
}
fn request(reader: &mut BufReader<UnixStream>) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}
fn send(writer: &mut UnixStream, value: Value) {
    writeln!(writer, "{value}").unwrap();
}
fn status() -> Value {
    let mut value = json!({"mode":"Video", "current":null, "current_filename":null,
        "engine":"none", "total_wallpapers":0, "locked_count":0, "available_count":0,
        "scanned_count":0, "vram_used_mb":0, "vram_total_mb":0, "vram_degraded":false,
        "uptime_secs":0, "protocol_version":3, "next_time_point":null, "time_points_count":0,
        "next_switch_secs":null});
    value["vram_override"] = Value::Null;
    value
}

#[test]
fn offline_config_preserves_types_and_rejects_invalid_edits() {
    let fixture = Fixture::new();
    let output = fixture.json(&["--json", "config", "set", "video_engine.interval", "123"]);
    assert_eq!(output["new_value"], 123);
    assert!(output["old_value"].is_number());
    assert_eq!(
        fixture.json(&["--json", "config", "get", "video_engine.interval"])["value"],
        123
    );
    fixture.json(&["--json", "config", "set", "video_engine.display", "123"]);
    assert_eq!(
        fixture.json(&["--json", "config", "get", "video_engine.display"])["value"],
        "123"
    );
    fixture.json(&[
        "--json",
        "config",
        "set",
        "video_engine.mpv_args",
        "[\"a b\",\"中文\"]",
    ]);
    assert_eq!(
        fixture.json(&["--json", "config", "get", "video_engine.mpv_args"])["value"],
        json!(["a b", "中文"])
    );
    let invalid = fixture.output(&["--json", "config", "set", "video_engine.interval", "1"]);
    assert!(!invalid.status.success());
    assert!(invalid.stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&invalid.stdout).unwrap()["success"],
        false
    );
    assert_eq!(
        fixture.json(&["--json", "config", "get", "video_engine.interval"])["value"],
        123
    );
    fixture.json(&["--json", "config", "set", "vram.backend", "custom"]);
    assert_eq!(
        fixture.json(&["--json", "config", "get", "vram.backend"])["value"],
        "custom"
    );
}

#[test]
fn json_errors_and_invalid_arguments_use_stdout() {
    let fixture = Fixture::new();
    for args in [
        vec!["--json", "next"],
        vec!["--json", "subscribe", "invalid"],
        vec!["--json", "--bad-argument"],
        vec!["--json"],
        vec!["--json", "start", "--foreground"],
    ] {
        let output = fixture.output(&args);
        assert!(!output.status.success(), "{args:?}");
        assert!(
            output.stderr.is_empty(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap()["success"],
            false
        );
    }
}

#[test]
fn subscription_is_ndjson_and_survives_idle_then_reports_eof() {
    let fixture = Fixture::new();
    let listener = fixture.listener();
    let server = std::thread::spawn(move || {
        let (mut reader, mut writer) = accept(&listener);
        let subscribe = request(&mut reader);
        send(
            &mut writer,
            json!({"id":subscribe["id"],"type":"Subscribed","payload":{"session_id":"test","subscribed_events":["error"]}}),
        );
        send(
            &mut writer,
            json!({"id":0,"type":"Status","payload":status()}),
        );
        std::thread::sleep(Duration::from_secs(11));
        send(
            &mut writer,
            json!({"id":0,"type":"Event","payload":{"event":"Error","data":{"code":"internal_error","message":"中文 \"quoted\"","recoverable":true}}}),
        );
    });
    let start = Instant::now();
    let output = fixture.output(&["--json", "subscribe", "error"]);
    server.join().unwrap();
    assert!(start.elapsed() >= Duration::from_secs(11));
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let frames: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(frames.len(), 4);
    assert_eq!(frames[0]["type"], "Subscribed");
    assert_eq!(frames[1]["type"], "Status");
    assert_eq!(frames[2]["payload"]["data"]["message"], "中文 \"quoted\"");
    assert_eq!(frames[3]["success"], false);
}

#[test]
fn rescan_keeps_events_received_before_acknowledgment() {
    let fixture = Fixture::new();
    let listener = fixture.listener();
    let server = std::thread::spawn(move || {
        let (mut reader, mut writer) = accept(&listener);
        let subscribe = request(&mut reader);
        send(
            &mut writer,
            json!({"id":subscribe["id"],"type":"Subscribed","payload":{"session_id":"test","subscribed_events":["space_updated"]}}),
        );
        let rescan = request(&mut reader);
        for mode in ["Video", "Image"] {
            send(
                &mut writer,
                json!({"id":0,"type":"Event","payload":{"event":"SpaceUpdated","data":{"mode":mode,"reason":"rescanned","summary":{"total":1,"available":1,"locked":0,"in_cooldown":0}}}}),
            );
        }
        send(&mut writer, json!({"id":rescan["id"],"type":"Ok"}));
    });
    let output = fixture.json(&["--json", "rescan"]);
    server.join().unwrap();
    assert_eq!(output["status"], "ok");
    assert_eq!(output["total"], 2);
}

#[test]
fn online_config_uses_the_same_json_types_as_offline() {
    let fixture = Fixture::new();
    let listener = fixture.listener();
    let server = std::thread::spawn(move || {
        let (mut reader, mut writer) = accept(&listener);
        let ping = request(&mut reader);
        send(
            &mut writer,
            json!({"id":ping["id"],"type":"Pong","payload":{"uptime_secs":1,"protocol_version":3}}),
        );
        drop(reader);
        drop(writer);
        let (mut reader, mut writer) = accept(&listener);
        let mut config: lianwall_core::config::Config =
            toml::from_str(lianwall_core::config::DEFAULT_CONFIG_TOML).unwrap();
        for _ in 0..3 {
            let frame = request(&mut reader);
            match frame["cmd"].as_str().unwrap() {
                "GetConfig" => {
                    let value = if let Some(key) = frame["key"].as_str() {
                        lianwall_core::config::get_value(&config, key).unwrap()
                    } else {
                        serde_json::to_value(&config).unwrap()
                    };
                    send(
                        &mut writer,
                        json!({"id":frame["id"],"type":"Config","payload":{"value":value}}),
                    );
                }
                "SetConfig" => {
                    lianwall_core::config::set_value(
                        &mut config,
                        frame["key"].as_str().unwrap(),
                        frame["value"].clone(),
                    )
                    .unwrap();
                    send(&mut writer, json!({"id":frame["id"],"type":"Ok"}));
                }
                other => panic!("unexpected command {other}"),
            }
        }
    });
    let output = fixture.json(&["--json", "config", "set", "video_engine.interval", "123"]);
    server.join().unwrap();
    assert!(output["old_value"].is_number());
    assert_eq!(output["new_value"], 123);
}

#[test]
fn unicode_hook_commands_do_not_panic_in_human_output() {
    let fixture = Fixture::new();
    let listener = fixture.listener();
    let command = format!("a{}", "中".repeat(30));
    let expected = command.clone();
    let server = std::thread::spawn(move || {
        let (mut reader, mut writer) = accept(&listener);
        let frame = request(&mut reader);
        send(
            &mut writer,
            json!({"id":frame["id"],"type":"HookList","payload":[{
                "name":"unicode", "on":"wallpaper_changed", "command":command,
                "mode":null, "trigger":null, "timeout":10, "enabled":true
            }]}),
        );
    });
    let output = fixture.output(&["hook", "list"]);
    server.join().unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains(&expected)
    );
}

#[test]
fn legacy_image_config_migrates_and_cli_only_accepts_new_key() {
    let fixture = Fixture::new();
    let path = fixture.directory.path().join("lianwall/config.toml");
    let original = std::fs::read_to_string(&path)
        .unwrap()
        .replace("awww_args", "swww_args");
    std::fs::write(&path, &original).unwrap();
    assert!(
        fixture.json(&["--json", "config", "get", "image_engine.awww_args"])["value"].is_array()
    );
    assert_eq!(
        std::fs::read_to_string(path.with_file_name("config.toml.pre-awww.bak")).unwrap(),
        original
    );
    let output = fixture.output(&["--json", "config", "get", "image_engine.swww_args"]);
    assert!(!output.status.success());
    let _: Value = serde_json::from_slice(&output.stdout).unwrap();
    fixture.json(&[
        "--json",
        "config",
        "set",
        "image_engine.awww_args",
        r#"["--transition-type","fade"]"#,
    ]);
    assert_eq!(
        fixture.json(&["--json", "config", "get", "image_engine.awww_args"])["value"],
        json!(["--transition-type", "fade"])
    );
    assert!(
        !std::fs::read_to_string(&path)
            .unwrap()
            .contains("swww_args")
    );
    // Conflicts must surface as JSON errors, rather than silently picking a value.
    std::fs::write(
        &path,
        original.replace("swww_args =", "awww_args = []\nswww_args ="),
    )
    .unwrap();
    let output = fixture.output(&["--json", "config", "show"]);
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(error.to_string().contains("Both"));
}
