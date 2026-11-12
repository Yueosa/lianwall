//! Real daemon, private PATH, fake engines: never contacts the user's wallpaper session.
use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::{fs::PermissionsExt, net::UnixStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const ENGINE: &str = r#"#!/usr/bin/python3
import json, os, pathlib, signal, subprocess, sys, time, socket, shlex, threading
root = pathlib.Path(os.environ['ENGINE_FIXTURE'])
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
with (root / 'calls.jsonl').open('a') as output:
    output.write(json.dumps([name, args], ensure_ascii=False) + '\n')
if name.endswith('-daemon'):
    stop = False
    def terminate(*_):
        global stop
        stop = True
    signal.signal(signal.SIGTERM, terminate)
    time.sleep(0.35)
    (root / 'ready').write_text('ready')
    while not stop: time.sleep(0.02)
    (root / 'ready').unlink(missing_ok=True)
elif name in ('awww', 'swww'):
    if args[0] == 'query': sys.exit(0 if (root / 'ready').exists() else 1)
    if (root / 'fail-image').exists():
        print('图片测试拒绝', file=sys.stderr)
        sys.exit(7)
    assert pathlib.Path(args[-1]).is_file()
    assert args[-2] == '--'
elif name == 'mpvpaper':
    if (root / 'fail-video').exists():
        print('video startup rejected', file=sys.stderr)
        sys.exit(8)
    options = shlex.split(args[args.index('-o') + 1])
    control_path = next(option.split('=', 1)[1] for option in options if option.lstrip('-').startswith('input-ipc-server='))
    control = socket.socket(socket.AF_UNIX)
    control.bind(control_path)
    control.listen()
    def answer():
        while True:
            connection, _ = control.accept()
            with connection:
                request = json.loads(connection.makefile('rb').readline())
                connection.sendall((json.dumps({'request_id':request['request_id'], 'error':'success', 'data':{'w':320,'h':180}})+'\n').encode())
    threading.Thread(target=answer, daemon=True).start()
    subprocess.Popen(['/usr/bin/python3', '-c', 'import time,pathlib; time.sleep(2); pathlib.Path(' + repr(str(root / 'leaked-descendant')) + ').write_text("leaked")'])
    sys.stdout.write('\rVO: fake-renderer\r')
    sys.stdout.flush()
    while True: time.sleep(1)
"#;

struct Fixture {
    dir: tempfile::TempDir,
    daemon: Child,
    socket: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}
impl Fixture {
    fn new(legacy: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let image = if legacy { "swww" } else { "awww" };
        for name in [
            image.to_owned(),
            format!("{image}-daemon"),
            "mpvpaper".into(),
        ] {
            let path = bin.join(name);
            fs::write(&path, ENGINE).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut config: lianwall_core::config::Config =
            toml::from_str(lianwall_core::config::DEFAULT_CONFIG_TOML).unwrap();
        config.paths.video_dir = dir.path().join("videos");
        config.paths.image_dir = dir.path().join("images");
        fs::create_dir(&config.paths.video_dir).unwrap();
        fs::create_dir(&config.paths.image_dir).unwrap();
        config.video_engine.interval = 0;
        config.image_engine.interval = 0;
        config.vram.enabled = false;
        config.video_engine.mpv_args = vec!["--no-audio".into(), "--script=/tmp/a b.lua".into()];
        config.daemon.socket_path = dir.path().join("daemon.sock");
        let socket = config.daemon.socket_path.clone();
        lianwall_core::config::update(lianwall_core::config::ConfigUpdateInput {
            path: Some(dir.path().join("config/lianwall/config.toml")),
            config,
        })
        .unwrap();
        let daemon = Command::new(env!("CARGO_BIN_EXE_lianwalld"))
            .env("PATH", &bin)
            .env("ENGINE_FIXTURE", dir.path())
            .env("HOME", dir.path())
            .env("XDG_CONFIG_HOME", dir.path().join("config"))
            .env("XDG_CACHE_HOME", dir.path().join("cache"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let fixture = Self {
            dir,
            daemon,
            socket,
        };
        let start = Instant::now();
        while UnixStream::connect(&fixture.socket).is_err() {
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(fixture.request(json!({"cmd":"Ping"}))["type"], "Pong");
        fixture
    }
    fn request(&self, mut request: Value) -> Value {
        request["id"] = json!(42);
        let mut stream = UnixStream::connect(&self.socket).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        writeln!(stream, "{request}").unwrap();
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }
    fn file(&self, name: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        fs::write(&path, "fixture").unwrap();
        path
    }
    fn set(&self, path: &PathBuf) -> Value {
        self.request(json!({"cmd":"SetWallpaper", "path":path}))
    }
    fn status(&self) -> Value {
        self.request(json!({"cmd":"GetStatus"}))["payload"].clone()
    }
    fn calls(&self) -> Vec<Value> {
        fs::read_to_string(self.dir.path().join("calls.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
    fn shutdown(&mut self) {
        assert_eq!(self.request(json!({"cmd":"Shutdown"}))["type"], "Ok");
        let start = Instant::now();
        loop {
            if let Some(status) = self.daemon.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!self.dir.path().join("ready").exists());
    }
}

#[test]
fn images_start_without_which_wait_until_ready_and_keep_video_on_image_failure() {
    let mut fixture = Fixture::new(false);
    let video = fixture.file("视频 # first.mp4");
    assert_eq!(fixture.set(&video)["type"], "Ok");
    let image = fixture.file("图片 # with spaces.png");
    fixture.file("fail-image");
    let failure = fixture.set(&image);
    assert_eq!(failure["type"], "Error");
    assert!(failure.to_string().contains("图片测试拒绝"));
    assert_eq!(fixture.status()["current"], json!(video));
    assert_eq!(fixture.status()["engine"], "mpvpaper");
    fs::remove_file(fixture.dir.path().join("fail-image")).unwrap();
    assert_eq!(fixture.set(&image)["type"], "Ok");
    assert_eq!(fixture.status()["engine"], "awww");
    let calls = fixture.calls();
    let daemon = calls.iter().find(|call| call[0] == "awww-daemon").unwrap();
    assert_eq!(daemon[1][0], "--namespace");
    assert!(calls.iter().any(|call| call[0] == "awww"
        && call[1][0] == "img"
        && call[1].as_array().unwrap().last() == Some(&json!(image))));
    fixture.shutdown();
    std::thread::sleep(Duration::from_millis(2100));
    assert!(!fixture.dir.path().join("leaked-descendant").exists());
}

#[test]
fn failed_video_preserves_engine_history_and_selection_then_shuts_down_owned_group() {
    let mut fixture = Fixture::new(false);
    let image = fixture.file("first.png");
    let video = fixture.file("second.mp4");
    assert_eq!(fixture.set(&image)["type"], "Ok");
    assert_eq!(fixture.set(&video)["type"], "Ok");
    fixture.file("fail-image");
    assert_eq!(fixture.request(json!({"cmd":"Prev"}))["type"], "Error");
    assert_eq!(fixture.status()["current"], json!(video));
    fs::remove_file(fixture.dir.path().join("fail-image")).unwrap();
    assert_eq!(fixture.request(json!({"cmd":"Prev"}))["type"], "Ok");
    assert_eq!(fixture.status()["current"], json!(image));
    fixture.file("fail-video");
    assert_eq!(fixture.request(json!({"cmd":"Next"}))["type"], "Error");
    assert_eq!(fixture.status()["current"], json!(image));
    assert_eq!(fixture.status()["engine"], "awww");
    fs::remove_file(fixture.dir.path().join("fail-video")).unwrap();
    assert_eq!(fixture.request(json!({"cmd":"Next"}))["type"], "Ok");
    assert_eq!(fixture.status()["current"], json!(video));
    // A failed selection must not change the space's current index/timestamps/cooldown.
    fixture.file("videos/select.mp4");
    assert_eq!(fixture.request(json!({"cmd":"Rescan"}))["type"], "Ok");
    let start = Instant::now();
    let before = loop {
        let space = fixture.request(json!({"cmd":"GetSpace", "mode":"Video"}));
        if space["payload"]["items"].as_array().unwrap().len() == 1 {
            break space;
        }
        assert!(start.elapsed() < Duration::from_secs(3));
        std::thread::sleep(Duration::from_millis(10));
    };
    fixture.file("fail-video");
    assert_eq!(fixture.request(json!({"cmd":"Next"}))["type"], "Error");
    assert_eq!(
        fixture.request(json!({"cmd":"GetSpace", "mode":"Video"})),
        before
    );
    fixture.shutdown();
}

#[test]
fn legacy_pair_works_and_reports_its_actual_engine_name() {
    let mut fixture = Fixture::new(true);
    let image = fixture.file("legacy.png");
    assert_eq!(fixture.set(&image)["type"], "Ok");
    assert_eq!(fixture.status()["engine"], "swww");
    fixture.shutdown();
}
