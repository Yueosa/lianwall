//! Engine discovery must not start GPU devices, players or worker commands.
use crate::state::SharedState;
use lianwall_core::{
    config::{VideoDecode, WallMode},
    engine::executable_in_path,
    socket::{EngineCapability, EngineInfo, Response},
};

pub async fn get_engine_info(state: &SharedState) -> Response {
    let config = state.get_config().await;
    let active = state.get_engine_state().await;
    let active_backend = if active.native_running {
        "native".into()
    } else if active.mpvpaper_running {
        "mpvpaper".into()
    } else if active.swww_daemon_running {
        active.image_bin.unwrap_or_else(|| "awww".into())
    } else {
        "none".into()
    };
    let native_compiled = cfg!(feature = "native-image");
    let mut backends = vec![EngineCapability {
        id: "native".into(), available: native_compiled,
        availability_basis: if native_compiled { "compiled" } else { "not_compiled" }.into(),
        media: vec![WallMode::Image, WallMode::Video],
        decode_policies: vec![VideoDecode::Software, VideoDecode::Vulkan, VideoDecode::Auto],
        image_formats: native_formats(), crossfade: true, explicit_single_output: true,
        note: "Experimental SDR single-output renderer. Compiled support is not a device/codec/Wayland capability probe; Vulkan support is checked when opening each video.".into(),
    }];
    for (id, installed, media) in [
        (
            "awww",
            executable_in_path("awww") || executable_in_path("swww"),
            WallMode::Image,
        ),
        ("mpvpaper", executable_in_path("mpvpaper"), WallMode::Video),
    ] {
        backends.push(EngineCapability { id: id.into(), available: installed,
            availability_basis: if installed { "executable" } else { "missing_executable" }.into(),
            media: vec![media], decode_policies: vec![], image_formats: vec![], crossfade: false,
            explicit_single_output: false,
            note: "Executable discovery only; actual playback and driver support are not probed. External arguments retain their existing meaning.".into(),
        });
    }
    #[cfg(feature = "native-image")]
    let native = state
        .engine
        .native
        .lock()
        .await
        .as_ref()
        .map(|engine| engine.runtime());
    #[cfg(not(feature = "native-image"))]
    let native = None;
    Response::EngineInfo(EngineInfo {
        schema_version: 1,
        configured_image: config.image_engine.backend,
        configured_video: config.video_engine.backend,
        configured_decode: config.video_engine.decode,
        active_backend,
        backends,
        native,
    })
}
fn native_formats() -> Vec<String> {
    #[cfg(feature = "native-image")]
    {
        crate::native::FORMATS
            .iter()
            .map(|s| s.to_string())
            .collect()
    }
    #[cfg(not(feature = "native-image"))]
    {
        Vec::new()
    }
}

pub async fn get_outputs() -> Response {
    #[cfg(feature = "native-image")]
    {
        // The blocking query retains the permit even if its client disconnects.
        static LIMIT: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
            std::sync::OnceLock::new();
        let permit = match LIMIT
            .get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(1)))
            .clone()
            .try_acquire_owned()
        {
            Ok(permit) => permit,
            Err(_) => {
                return Response::error(
                    lianwall_core::socket::ErrorCode::EngineError,
                    "Output discovery already in progress; retry shortly",
                );
            }
        };
        match tokio::task::spawn_blocking(move || {
            let _permit = permit;
            crate::outputs::discover()
        })
        .await
        {
            Ok(snapshot) => Response::Outputs(snapshot),
            Err(error) => Response::error(
                lianwall_core::socket::ErrorCode::InternalError,
                format!("Output discovery failed: {error}"),
            ),
        }
    }
    #[cfg(not(feature = "native-image"))]
    {
        Response::Outputs(lianwall_core::socket::OutputSnapshot {
            schema_version: 1,
            available: false,
            truncated: false,
            protocols: Default::default(),
            outputs: vec![],
            error: Some(
                "Wayland output discovery is not compiled in; build with native-image".into(),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn discovery_is_read_only_and_does_not_create_a_native_worker() {
        let config = toml::from_str(lianwall_core::config::DEFAULT_CONFIG_TOML).unwrap();
        let state = SharedState::init(config).await.unwrap();
        let Response::EngineInfo(info) = get_engine_info(&state).await else {
            panic!("wrong response")
        };
        assert_eq!(info.active_backend, "none");
        assert!(info.native.is_none());
        assert_eq!(info.schema_version, 1);
        assert_eq!(info.backends.len(), 3);
        let native = &info.backends[0];
        assert_eq!(native.available, cfg!(feature = "native-image"));
        assert!(native.explicit_single_output);
        #[cfg(feature = "native-image")]
        assert!(state.engine.native.lock().await.is_none());
        let value = serde_json::to_value(Response::EngineInfo(info)).unwrap();
        assert_eq!(value["type"], "EngineInfo");
        assert!(value["payload"]["native"].is_null());
    }
}
