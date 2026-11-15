use crate::state::SharedState;
use lianwall_core::{
    config::VramBackend,
    gpu::GpuBackend,
    socket::{ErrorCode, Response},
};
pub async fn test_vram(
    state: &SharedState,
    backend: Option<VramBackend>,
    command: Option<String>,
) -> Response {
    let _permit = match state.vram_probe_permit.try_acquire() {
        Ok(permit) => permit,
        Err(_) => {
            return Response::error(ErrorCode::InternalError, "A GPU test is already running");
        }
    };
    let config = state.get_config().await;
    let backend = backend.unwrap_or(config.vram.backend);
    if command.as_ref().is_some_and(|value| value.len() > 16384) {
        return Response::error(ErrorCode::InvalidRequest, "GPU test command exceeds 16 KiB");
    }
    let backend = match backend {
        VramBackend::Auto => {
            if command.as_ref().is_some_and(|value| !value.is_empty()) {
                return Response::error(
                    ErrorCode::InvalidRequest,
                    "A command requires backend=custom",
                );
            }
            lianwall_core::gpu::detect_backend().await
        }
        VramBackend::Custom => {
            let command = command.unwrap_or(config.vram.custom_command);
            if command.len() > 16384 {
                return Response::error(
                    ErrorCode::InvalidRequest,
                    "GPU test command exceeds 16 KiB",
                );
            }
            GpuBackend::Custom { command }
        }
    };
    // Deliberately independent of monitoring enabled/override, snapshots and rotation.
    Response::VramTest(lianwall_core::gpu::probe_vram(backend).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_unsaved_probe_does_not_mutate_monitor_or_rotation() {
        let config: lianwall_core::config::Config =
            toml::from_str(lianwall_core::config::DEFAULT_CONFIG_TOML).unwrap();
        let state = SharedState::init(config).await.unwrap();
        *state.vram_override.write().await = Some(true);
        let before = serde_json::to_value(state.get_config().await).unwrap();
        let deadline = *state.next_switch.read().await;
        let result = test_vram(
            &state,
            Some(VramBackend::Custom),
            Some("printf 'used_mb=123\ntotal_mb=1024\n'".into()),
        )
        .await;
        assert!(
            matches!(result,Response::VramTest(report) if report.success&&report.used_mb==Some(123))
        );
        assert_eq!(
            before,
            serde_json::to_value(state.get_config().await).unwrap()
        );
        assert_eq!(deadline, *state.next_switch.read().await);
        assert_eq!(*state.vram_override.read().await, Some(true));
        assert!(state.gpu_state.read().await.is_none());
        assert!(state.gpu_snapshot.read().await.vram_info.is_none());
        let _permit = state.vram_probe_permit.acquire().await.unwrap();
        assert!(matches!(
            test_vram(&state, None, None).await,
            Response::Error { .. }
        ));
    }
}
