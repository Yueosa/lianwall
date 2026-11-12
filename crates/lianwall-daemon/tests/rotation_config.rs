//! Configuration writes run in a child process with an isolated XDG directory.
//! This avoids mutating process-wide environment variables in parallel async tests.
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lianwall_core::config::{Config, DEFAULT_CONFIG_TOML};
use lianwall_core::socket::{Request, Response};
use lianwall_daemon::{EventBus, SharedState, handler};

#[test]
fn rotation_config_updates_preserve_deadlines_and_pause_semantics() {
    if std::env::var("LIANWALL_ROTATION_TEST_CHILD").as_deref() == Ok("1") {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let mut config: Config = toml::from_str(DEFAULT_CONFIG_TOML).unwrap();
            config.video_engine.interval = 60;
            let state = SharedState::init(config).await.unwrap();
            let events = EventBus::new(16);
            state.set_next_switch(Some(Instant::now() + Duration::from_secs(30))).await;
            let original_deadline = *state.next_switch.read().await;
            let response = handler::handle_command(&state, &events, Request::SetConfig {
                key: "video_engine.interval".into(), value: serde_json::json!(60),
            }).await;
            assert!(matches!(response, Response::Ok));
            assert_eq!(*state.next_switch.read().await, original_deadline);

            let response = handler::handle_command(&state, &events, Request::PauseRotation).await;
            assert!(matches!(response, Response::Ok));
            let status = handler::handle_query(&state, Request::GetStatus).await;
            assert!(matches!(status, Response::Status(info) if info.next_switch_secs.is_none()));
            let status = handler::handle_query(&state, Request::GetRotationStatus).await;
            assert!(matches!(status, Response::RotationStatus(info) if info.paused && info.saved_interval == Some(60)));

            // An unrelated persisted edit must not make the temporary pause permanent.
            let response = handler::handle_command(&state, &events, Request::SetConfig {
                key: "video_engine.display".into(), value: serde_json::json!("123"),
            }).await;
            assert!(matches!(response, Response::Ok));
            let disk = lianwall_core::config::read(lianwall_core::config::ConfigReadInput { path: None }).unwrap().config;
            assert_eq!(disk.video_engine.interval, 60);
            assert_eq!(state.get_config().await.video_engine.interval, 0);

            let response = handler::handle_command(&state, &events, Request::ResumeRotation).await;
            assert!(matches!(response, Response::Ok));
            assert!(state.next_switch_remaining_secs().await.unwrap() >= 58);
            let status = handler::handle_query(&state, Request::GetRotationStatus).await;
            assert!(matches!(status, Response::RotationStatus(info) if !info.paused));
            // paths.mode is the startup preference; live mode remains controlled by SetMode.
            let response = handler::handle_command(&state, &events, Request::SetConfig {
                key: "paths.mode".into(), value: serde_json::json!("Image"),
            }).await;
            assert!(matches!(response, Response::Ok));
            assert_eq!(*state.engine.mode.read().await, lianwall_core::config::WallMode::Video);
            assert_eq!(state.get_config().await.paths.mode, lianwall_core::config::WallMode::Image);

            // A failing file write must not commit the edited value in memory or publish it.
            let path = lianwall_core::config::config_path(None);
            std::fs::remove_file(&path).unwrap();
            std::fs::create_dir(&path).unwrap();
            let before = state.get_config().await.algorithm.temperature;
            let mut receiver = events.subscribe();
            let response = handler::handle_command(&state, &events, Request::SetConfig {
                key: "algorithm.temperature".into(), value: serde_json::json!(0.5),
            }).await;
            assert!(matches!(response, Response::Error { .. }));
            assert_eq!(state.get_config().await.algorithm.temperature, before);
            assert!(receiver.try_recv().is_err());

        });
        return;
    }

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("lianwall-rotation-{}-{unique}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "rotation_config_updates_preserve_deadlines_and_pause_semantics",
            "--nocapture",
        ])
        .env("LIANWALL_ROTATION_TEST_CHILD", "1")
        .env("XDG_CONFIG_HOME", &root)
        .output()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(
        output.status.success(),
        "child test failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
