use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub sequence: u64,
    pub timestamp: String,
    pub level: String,
    pub line: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSnapshot {
    pub schema_version: u32,
    pub entries: Vec<LogEntry>,
    pub path: Option<PathBuf>,
    pub persistence_error: Option<String>,
    pub dropped_disk_entries: u64,
    pub active_filter: String,
    pub environment_override: bool,
}
