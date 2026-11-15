//! Bounded in-memory diagnostics and a best-effort rotating file worker.
use lianwall_core::socket::{LogEntry, LogSnapshot};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::{BufRead, Write},
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
};
use tracing_subscriber::{
    EnvFilter, Registry,
    layer::{Context, SubscriberExt},
    reload,
    util::SubscriberInitExt,
};
const MAX_LINES: usize = 1024;
const MAX_LINE_BYTES: usize = 2048;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
type FilterHandle = reload::Handle<EnvFilter, Registry>;
#[derive(Clone)]
pub struct LogStore(Arc<Mutex<Inner>>);
struct Inner {
    lines: VecDeque<LogEntry>,
    sequence: u64,
    path: Option<PathBuf>,
    error: Option<String>,
    dropped: u64,
    sender: Option<mpsc::SyncSender<Message>>,
    filter: Option<FilterHandle>,
    active_filter: String,
    environment_override: bool,
}
enum Message {
    Line(LogEntry),
    Flush(mpsc::Sender<()>),
}
impl Default for LogStore {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Inner {
            lines: VecDeque::new(),
            sequence: 0,
            path: None,
            error: None,
            dropped: 0,
            sender: None,
            filter: None,
            active_filter: String::new(),
            environment_override: false,
        })))
    }
}
fn clipped(text: &str) -> String {
    let escaped: String = text
        .chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_debug().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect();
    if escaped.len() <= MAX_LINE_BYTES {
        return escaped;
    }
    let mut end = MAX_LINE_BYTES - 3;
    while !escaped.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &escaped[..end])
}
fn filter_text(level: &str) -> String {
    format!("lianwalld={level},lianwall_daemon={level},lianwall_core={level}")
}
pub fn valid_level(level: &str) -> bool {
    matches!(level, "trace" | "debug" | "info" | "warn" | "error")
}
fn rank(level: &str) -> u8 {
    match level.to_ascii_lowercase().as_str() {
        "trace" => 0,
        "debug" => 1,
        "info" => 2,
        "warn" => 3,
        _ => 4,
    }
}
impl LogStore {
    pub fn open(&self, path: PathBuf) {
        // Only the production entry point attaches a file. Test states are memory-only.
        if let Ok(file) = File::open(&path) {
            if file.metadata().is_ok_and(|m| m.len() <= MAX_FILE_BYTES) {
                let mut inner = self.0.lock().unwrap();
                for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
                    if let Ok(mut entry) = serde_json::from_str::<LogEntry>(&line) {
                        if !valid_level(&entry.level) || entry.timestamp.len() > 64 {
                            continue;
                        }
                        entry.line = clipped(&entry.line);
                        inner.sequence = inner.sequence.max(entry.sequence);
                        inner.lines.push_back(entry);
                        if inner.lines.len() > MAX_LINES {
                            inner.lines.pop_front();
                        }
                    }
                }
            }
        }
        let (sender, receiver) = mpsc::sync_channel(2048);
        {
            let mut inner = self.0.lock().unwrap();
            inner.path = Some(path.clone());
            inner.sender = Some(sender);
        }
        let weak = Arc::downgrade(&self.0);
        std::thread::spawn(move || {
            let mut file = None;
            for message in receiver {
                match message {
                    Message::Flush(done) => {
                        if let Some(file) = file.as_mut() {
                            let _: std::io::Result<()> = Write::flush(file);
                        }
                        let _ = done.send(());
                    }
                    Message::Line(entry) => {
                        let result = (|| -> std::io::Result<()> {
                            use std::os::unix::fs::OpenOptionsExt;
                            if file.is_none() {
                                std::fs::create_dir_all(path.parent().unwrap())?;
                                file = Some(
                                    OpenOptions::new()
                                        .append(true)
                                        .create(true)
                                        .mode(0o600)
                                        .open(&path)?,
                                );
                            }
                            let mut line = serde_json::to_vec(&entry)?;
                            line.push(b'\n');
                            if file.as_ref().unwrap().metadata()?.len() + line.len() as u64
                                > MAX_FILE_BYTES
                            {
                                file.take();
                                // Atomic replacement of one previous generation; no unbounded file series.
                                std::fs::rename(&path, path.with_extension("jsonl.1"))?;
                                file = Some(
                                    OpenOptions::new()
                                        .append(true)
                                        .create(true)
                                        .mode(0o600)
                                        .open(&path)?,
                                );
                            }
                            file.as_mut().unwrap().write_all(&line)
                        })();
                        if let Some(shared) = weak.upgrade() {
                            let mut inner = shared.lock().unwrap();
                            match result {
                                Ok(()) => inner.error = None,
                                Err(error) => {
                                    inner.error = Some(error.to_string());
                                    inner.dropped = inner.dropped.saturating_add(1);
                                    file = None;
                                }
                            }
                        }
                    }
                }
            }
        });
    }
    fn push(&self, level: &str, line: &str) {
        let mut inner = self.0.lock().unwrap();
        inner.sequence = inner.sequence.saturating_add(1);
        let entry = LogEntry {
            sequence: inner.sequence,
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            level: level.to_ascii_lowercase(),
            line: clipped(line),
        };
        if let Some(sender) = &inner.sender {
            if sender.try_send(Message::Line(entry.clone())).is_err() {
                inner.dropped = inner.dropped.saturating_add(1);
                inner.error =
                    Some("Log file queue is full or unavailable; disk entries were dropped".into());
            }
        }
        inner.lines.push_back(entry);
        if inner.lines.len() > MAX_LINES {
            inner.lines.pop_front();
        }
    }
    pub fn snapshot(&self, level: Option<&str>, limit: usize) -> LogSnapshot {
        let inner = self.0.lock().unwrap();
        let mut entries = inner
            .lines
            .iter()
            .rev()
            .filter(|entry| level.is_none_or(|level| rank(&entry.level) >= rank(level)))
            .take(limit.clamp(1, 128))
            .cloned()
            .collect::<Vec<_>>();
        entries.reverse();
        LogSnapshot {
            schema_version: 1,
            entries,
            path: inner.path.clone(),
            persistence_error: inner.error.clone(),
            dropped_disk_entries: inner.dropped,
            active_filter: inner.active_filter.clone(),
            environment_override: inner.environment_override,
        }
    }
    pub fn validate_filter(&self, level: &str) -> Result<(), String> {
        if !valid_level(level) {
            return Err("Invalid log level".into());
        }
        EnvFilter::try_new(filter_text(level))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    pub fn set_level(&self, level: &str) -> Result<(), String> {
        self.validate_filter(level)?;
        let mut inner = self.0.lock().unwrap();
        // An explicit RUST_LOG remains authoritative for this process.
        if inner.environment_override {
            return Ok(());
        }
        let text = filter_text(level);
        if let Some(handle) = &inner.filter {
            handle
                .reload(EnvFilter::try_new(&text).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        }
        inner.active_filter = text;
        Ok(())
    }
    pub async fn flush(&self) {
        let sender = self.0.lock().unwrap().sender.clone();
        if let Some(sender) = sender {
            let _ = tokio::task::spawn_blocking(move || {
                let (done, ack) = mpsc::channel();
                if sender.send(Message::Flush(done)).is_ok() {
                    let _ = ack.recv_timeout(std::time::Duration::from_secs(2));
                }
            })
            .await;
        }
    }
}
struct Fields(String);
impl tracing::field::Visit for Fields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        if field.name() != "message" {
            self.0.push_str(field.name());
            self.0.push('=');
        }
        self.0.push_str(&format!("{value:?}"));
    }
}
#[derive(Clone)]
struct Capture(LogStore);
impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Capture {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let mut fields = Fields(String::new());
        event.record(&mut fields);
        let meta = event.metadata();
        self.0.push(
            meta.level().as_str(),
            &format!("{} {}: {}", meta.level(), meta.target(), fields.0),
        );
    }
}
pub fn init(store: &LogStore, level: &str) -> anyhow::Result<()> {
    let override_text = std::env::var("RUST_LOG")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let text = override_text.clone().unwrap_or_else(|| filter_text(level));
    let (filter, handle) = reload::Layer::new(EnvFilter::try_new(&text)?);
    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_ansi(false))
        .with(Capture(store.clone()))
        .try_init()?;
    let mut inner = store.0.lock().unwrap();
    inner.filter = Some(handle);
    inner.active_filter = text;
    inner.environment_override = override_text.is_some();
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_ring_filter_rotation_and_restart_are_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("daemon.jsonl");
        let store = LogStore::default();
        store.open(path.clone());
        // Flush batches so the queue itself is not the source of data loss here.
        for batch in 0..20 {
            for n in 0..100 {
                store.push(
                    if n % 2 == 0 { "info" } else { "error" },
                    &format!("{batch}-{n} {}", "x".repeat(1900)),
                );
            }
            store.flush().await;
        }
        assert!(std::fs::metadata(&path).unwrap().len() <= MAX_FILE_BYTES);
        assert!(
            std::fs::metadata(path.with_extension("jsonl.1"))
                .unwrap()
                .len()
                <= MAX_FILE_BYTES
        );
        let snapshot = store.snapshot(Some("warn"), 999);
        assert_eq!(snapshot.entries.len(), 128);
        assert!(snapshot.entries.iter().all(|entry| entry.level == "error"));
        let reopened = LogStore::default();
        reopened.open(path);
        assert!(!reopened.snapshot(None, 1).entries.is_empty());
        assert_eq!(store.0.lock().unwrap().lines.len(), MAX_LINES);
    }
    #[tokio::test]
    async fn test_file_failure_keeps_memory_diagnostics_readable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("blocked.jsonl");
        std::fs::create_dir(&path).unwrap();
        let store = LogStore::default();
        store.open(path);
        store.push("error", "still readable");
        store.flush().await;
        let snapshot = store.snapshot(None, 128);
        assert_eq!(snapshot.entries.len(), 1);
        assert!(snapshot.persistence_error.is_some());
        assert!(snapshot.dropped_disk_entries > 0);
    }

    #[test]
    fn test_dynamic_level_changes_filter_capture() {
        let store = LogStore::default();
        let (layer, handle) = reload::Layer::new(EnvFilter::new(filter_text("error")));
        let subscriber = tracing_subscriber::registry()
            .with(layer)
            .with(Capture(store.clone()));
        store.0.lock().unwrap().filter = Some(handle);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("hidden");
            tracing::error!("visible");
            assert_eq!(store.snapshot(None, 128).entries.len(), 1);
            store.set_level("info").unwrap();
            tracing::info!("now visible");
            assert_eq!(store.snapshot(None, 128).entries.len(), 2);
        });
        store.push("info", &format!("line\n{}", "界".repeat(3000)));
        let entry = store.snapshot(None, 1).entries.pop().unwrap();
        assert!(entry.line.len() <= MAX_LINE_BYTES);
        assert!(!entry.line.contains('\n'));
    }
}
