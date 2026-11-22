//! Persistent user metadata, independent of disposable thumbnails.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub path: PathBuf,
    pub favorite: bool,
    /// Directory windows from the most recent scan; omitted for unrestricted or absent paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scanned_time_constraints: Vec<String>,
    /// Unix seconds: first discovered by this daemon, not filesystem creation time.
    pub added_at: u64,
    pub last_used: Option<u64>,
    pub play_count: u64,
    /// Successful wallpaper residency, not video duration or user watch time.
    pub display_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibrarySnapshot {
    pub schema_version: u32,
    pub revision: u64,
    pub entries: Vec<LibraryEntry>,
    pub total: usize,
    pub next_offset: Option<usize>,
    pub persistence_error: Option<String>,
}
