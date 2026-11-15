#![cfg(feature = "native-image")]
use lianwall_daemon::native::{NativeEngine, Options};
use std::path::Path;

/// Run explicitly inside an isolated Wayland session, never on the user's outputs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires layer-shell Wayland, Vulkan, and LIANWALL_NATIVE_TEST_OUTPUT"]
async fn native_cache_transitions_rollback_and_shutdown() {
    let output = std::env::var("LIANWALL_NATIVE_TEST_OUTPUT").expect("set a test output name");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("native/tests/fixtures");
    let engine = std::sync::Arc::new(NativeEngine::new().unwrap());
    let cut = Options {
        fit: 0,
        duration_ms: 0,
        fps: 30,
    };
    let fade = Options {
        fit: 1,
        duration_ms: 400,
        fps: 30,
    };
    let first = engine
        .show(&fixtures.join("red.png"), &output, cut)
        .await
        .unwrap();
    assert!(engine.is_active());
    let repeated = engine
        .show(&fixtures.join("red.png"), &output, cut)
        .await
        .unwrap();
    assert_eq!(repeated.decodes, first.decodes);
    assert_eq!(repeated.uploads, first.uploads);
    assert_eq!(
        repeated.frames, first.frames,
        "unchanged wallpaper must not redraw"
    );
    let second = engine
        .show(&fixtures.join("blue.png"), &output, fade)
        .await
        .unwrap();
    assert_eq!(second.decodes, 2);
    assert_eq!(second.uploads, 2);
    assert!(
        second.frames > first.frames + 1,
        "transition must produce intermediate frames"
    );
    assert!(
        second.frames <= first.frames + 16,
        "30 fps must bound the transition"
    );
    let previous = engine
        .show(&fixtures.join("red.png"), &output, cut)
        .await
        .unwrap();
    assert_eq!(
        previous.decodes, 2,
        "previous wallpaper must reuse its source texture"
    );
    assert_eq!(previous.uploads, 2);
    let temp = tempfile::tempdir().unwrap();
    let invalid = temp.path().join("bad.png");
    std::fs::write(&invalid, b"broken image").unwrap();
    assert!(engine.show(&invalid, &output, cut).await.is_err());
    assert!(
        engine.is_active(),
        "decode failure must retain the current image"
    );
    assert!(
        engine
            .show(&fixtures.join("blue.png"), "missing-output", cut)
            .await
            .is_err()
    );
    assert!(
        engine.is_active(),
        "bad output must retain the current surface"
    );
    let modified = temp.path().join("modified.png");
    std::fs::copy(fixtures.join("red.png"), &modified).unwrap();
    let before = engine.show(&modified, &output, cut).await.unwrap();
    std::fs::copy(fixtures.join("blue.png"), &modified).unwrap();
    let after = engine.show(&modified, &output, cut).await.unwrap();
    assert_eq!(
        after.decodes,
        before.decodes + 1,
        "replaced files invalidate the cache"
    );
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let idle = engine.show(&modified, &output, cut).await.unwrap();
    assert_eq!(
        idle.frames, after.frames,
        "idle renderer must not submit frames"
    );
    engine.hide().await.unwrap();
    assert!(!engine.is_active());
    engine
        .show(&fixtures.join("red.png"), &output, cut)
        .await
        .unwrap();
    let changing = engine.clone();
    let blue = fixtures.join("blue.png");
    let selected_output = output.clone();
    let transition = tokio::spawn(async move {
        changing
            .show(
                &blue,
                &selected_output,
                Options {
                    duration_ms: 2000,
                    ..fade
                },
            )
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    tokio::time::timeout(std::time::Duration::from_secs(2), engine.hide())
        .await
        .unwrap()
        .unwrap();
    assert!(
        transition.await.unwrap().is_err(),
        "hide cancels an in-flight transition"
    );
    assert!(!engine.is_active());
    engine.shutdown().await;
    assert!(!engine.is_active());
    assert!(
        engine
            .show(&fixtures.join("blue.png"), &output, cut)
            .await
            .is_err()
    );
    eprintln!("native cache/transition stats: first={first:?}, fade={second:?}, final={idle:?}");
}
