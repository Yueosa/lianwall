//! Additive engine introspection. Availability is discovery, not playback proof.
use crate::config::{ImageBackend, VideoBackend, VideoDecode, WallMode};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInfo {
    pub schema_version: u32,
    pub configured_image: ImageBackend,
    pub configured_video: VideoBackend,
    pub configured_decode: VideoDecode,
    pub active_backend: String,
    pub backends: Vec<EngineCapability>,
    pub native: Option<NativeRuntime>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineCapability {
    pub id: String,
    pub available: bool,
    /// compiled / not_compiled / executable / missing_executable
    pub availability_basis: String,
    pub media: Vec<WallMode>,
    pub decode_policies: Vec<VideoDecode>,
    pub image_formats: Vec<String>,
    pub crossfade: bool,
    pub explicit_single_output: bool,
    pub note: String,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativePhase {
    #[default]
    Idle,
    Preparing,
    Running,
    Stopped,
    Failed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecoderKind {
    Software,
    Vulkan,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineOperation {
    #[serde(default)]
    pub output: String,
    pub path: PathBuf,
    pub media: WallMode,
    pub requested_decode: Option<VideoDecode>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NativeRuntime {
    pub revision: u64,
    pub phase: NativePhase,
    pub current: Option<PathBuf>,
    #[serde(default)]
    pub output: Option<String>,
    pub media: Option<WallMode>,
    pub pending: Option<EngineOperation>,
    pub requested_decode: Option<VideoDecode>,
    pub decoder: Option<DecoderKind>,
    pub fallback_reason: Option<String>,
    pub last_error: Option<String>,
    /// Sampled on state changes and approximately once per second while idle/running.
    pub statistics: EngineStatistics,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct EngineStatistics {
    pub decodes: u64,
    pub uploads: u64,
    pub cache_hits: u64,
    pub frames: u64,
    pub video_frames: u64,
    pub video_dropped: u64,
    pub video_loops: u64,
    pub video_hardware_frames: u64,
    pub video_software_frames: u64,
    pub video_fallbacks: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutputProtocols {
    pub compositor: bool,
    pub layer_shell: bool,
    pub viewporter: bool,
    pub fractional_scale: bool,
    pub presentation_time: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputInfo {
    pub registry_id: u32,
    pub name: Option<String>,
    pub description: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub refresh_millihz: Option<i32>,
    pub integer_scale: i32,
    pub transform: i32,
    pub native_selectable: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputSnapshot {
    pub schema_version: u32,
    pub available: bool,
    pub truncated: bool,
    pub protocols: OutputProtocols,
    pub outputs: Vec<OutputInfo>,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn engine_query_and_runtime_have_explicit_nulls_and_decoder_policy() {
        let request: super::super::Request =
            serde_json::from_str(r#"{"cmd":"GetEngineInfo"}"#).unwrap();
        assert_eq!(request.name(), "GetEngineInfo");
        let runtime = NativeRuntime {
            phase: NativePhase::Running,
            requested_decode: Some(VideoDecode::Auto),
            decoder: Some(DecoderKind::Software),
            fallback_reason: Some("unsupported profile".into()),
            ..Default::default()
        };
        let value = serde_json::to_value(&runtime).unwrap();
        assert_eq!(value["requested_decode"], "auto");
        assert_eq!(value["decoder"], "software");
        assert!(value["pending"].is_null());
        let parsed: NativeRuntime = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.phase, NativePhase::Running);
    }
}
