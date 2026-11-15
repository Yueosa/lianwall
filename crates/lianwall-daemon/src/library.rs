//! Daemon-owned metadata. Filesystem writes are serialized off the async workers.
use lianwall_core::socket::{LibraryEntry, LibrarySnapshot};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

#[derive(Default, Serialize, Deserialize)]
struct FileData {
    schema_version: u32,
    entries: Vec<LibraryEntry>,
}
struct Inner {
    entries: BTreeMap<PathBuf, LibraryEntry>,
    active: Option<(PathBuf, Instant)>,
    revision: u64,
    file: Option<PathBuf>,
    /// An unreadable/unknown file is never overwritten automatically.
    load_error: Option<String>,
    write_error: Option<String>,
}
pub struct LibraryStore(Mutex<Inner>);
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
impl Default for LibraryStore {
    fn default() -> Self {
        Self(Mutex::new(Inner {
            entries: BTreeMap::new(),
            active: None,
            revision: 0,
            file: None,
            load_error: None,
            write_error: None,
        }))
    }
}
impl LibraryStore {
    /// Called only by the production entry point; unit/integration states stay in memory.
    pub async fn open(&self, path: PathBuf) {
        let file = path.clone();
        let loaded = tokio::task::spawn_blocking(move || -> Result<FileData, String> {
            let metadata = match std::fs::metadata(&file) {
                Ok(value) => value,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(FileData {
                        schema_version: 1,
                        entries: vec![],
                    });
                }
                Err(e) => return Err(e.to_string()),
            };
            if metadata.len() > 32 * 1024 * 1024 {
                return Err("Library index exceeds 32 MiB".into());
            }
            let data: FileData =
                serde_json::from_slice(&std::fs::read(file).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            if data.schema_version != 1 {
                return Err("Unsupported library schema; original file preserved".into());
            }
            if data.entries.iter().any(|entry| {
                !entry.path.is_absolute()
                    || entry.path.to_str().is_none_or(|path| path.len() > 4096)
            }) {
                return Err("Library contains an invalid path; original file preserved".into());
            }
            Ok(data)
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|value| value);
        let mut inner = self.0.lock().await;
        inner.file = Some(path);
        match loaded {
            Ok(data) => {
                inner.entries = data
                    .entries
                    .into_iter()
                    .map(|entry| (entry.path.clone(), entry))
                    .collect()
            }
            Err(error) => {
                tracing::warn!(%error, "Library index unavailable; preserving original file");
                inner.load_error = Some(error);
            }
        }
    }
    pub async fn discover(&self, paths: impl IntoIterator<Item = PathBuf>) {
        let mut inner = self.0.lock().await;
        let timestamp = now();
        let mut changed = false;
        for path in paths {
            if let std::collections::btree_map::Entry::Vacant(slot) =
                inner.entries.entry(path.clone())
            {
                slot.insert(LibraryEntry {
                    path,
                    added_at: timestamp,
                    ..Default::default()
                });
                changed = true;
            }
        }
        if changed {
            inner.revision = inner.revision.saturating_add(1);
            persist(&mut inner).await;
        }
    }
    pub async fn activate(&self, path: &Path) {
        let mut inner = self.0.lock().await;
        if inner
            .active
            .as_ref()
            .is_some_and(|(active, _)| active == path)
        {
            return;
        }
        settle(&mut inner);
        let entry = inner
            .entries
            .entry(path.to_owned())
            .or_insert_with(|| LibraryEntry {
                path: path.to_owned(),
                added_at: now(),
                ..Default::default()
            });
        entry.play_count = entry.play_count.saturating_add(1);
        entry.last_used = Some(now());
        inner.active = Some((path.to_owned(), Instant::now()));
        inner.revision = inner.revision.saturating_add(1);
        persist(&mut inner).await;
    }
    pub async fn favorite(&self, path: &Path, favorite: bool) -> Result<(), String> {
        let mut inner = self.0.lock().await;
        if let Some(error) = &inner.load_error {
            return Err(error.clone());
        }
        let old = inner
            .entries
            .get(path)
            .ok_or("Wallpaper is not in the library index")?
            .favorite;
        inner.entries.get_mut(path).unwrap().favorite = favorite;
        if let Some(error) = persist(&mut inner).await {
            inner.entries.get_mut(path).unwrap().favorite = old;
            return Err(error);
        }
        if old != favorite {
            inner.revision = inner.revision.saturating_add(1);
        }
        Ok(())
    }
    pub async fn snapshot(&self, offset: usize, limit: usize) -> LibrarySnapshot {
        let inner = self.0.lock().await;
        let entries = inner
            .entries
            .values()
            .skip(offset)
            .take(limit.clamp(1, 128))
            .cloned()
            .map(|mut entry| {
                if let Some((path, started)) = &inner.active {
                    if path == &entry.path {
                        entry.display_seconds = entry
                            .display_seconds
                            .saturating_add(started.elapsed().as_secs());
                    }
                }
                entry
            })
            .collect::<Vec<_>>();
        let end = offset.saturating_add(entries.len());
        LibrarySnapshot {
            schema_version: 1,
            revision: inner.revision,
            total: inner.entries.len(),
            next_offset: (end < inner.entries.len()).then_some(end),
            entries,
            persistence_error: inner
                .load_error
                .clone()
                .or_else(|| inner.write_error.clone()),
        }
    }
    pub async fn checkpoint(&self) {
        let mut inner = self.0.lock().await;
        if inner.active.is_none() && inner.write_error.is_none() {
            return;
        }
        settle(&mut inner);
        persist(&mut inner).await;
    }
}
fn settle(inner: &mut Inner) {
    if let Some((path, started)) = &mut inner.active {
        let seconds = started.elapsed().as_secs();
        if let Some(entry) = inner.entries.get_mut(path) {
            entry.display_seconds = entry.display_seconds.saturating_add(seconds);
        }
        // Retain fractional seconds across checkpoints.
        *started += std::time::Duration::from_secs(seconds);
    }
}
async fn persist(inner: &mut Inner) -> Option<String> {
    if let Some(error) = &inner.load_error {
        return Some(error.clone());
    }
    let Some(path) = inner.file.clone() else {
        return None;
    };
    let data = FileData {
        schema_version: 1,
        entries: inner.entries.values().cloned().collect(),
    };
    let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
        use std::io::Write;
        let parent = path.parent().ok_or("Library path has no parent")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut temp, &data).map_err(|e| e.to_string())?;
        temp.write_all(b"\n").map_err(|e| e.to_string())?;
        if temp.as_file().metadata().map_err(|e| e.to_string())?.len() > 32 * 1024 * 1024 {
            return Err("Library index exceeds 32 MiB; previous file preserved".into());
        }
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        temp.persist(&path).map_err(|e| e.to_string())?;
        // A successful rename is the commit point; subsequent durability errors
        // must not report a failed favorite whose new value is already visible.
        if let Err(error) = std::fs::File::open(parent).and_then(|dir| dir.sync_all()) {
            tracing::warn!(%error, "Library directory sync failed");
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|value| value);
    inner.write_error = result.err();
    if let Some(error) = &inner.write_error {
        tracing::warn!(%error, "Library metadata could not be saved");
    }
    inner.write_error.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_favorite_and_usage_survive_restart_without_offline_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library.json");
        let store = LibraryStore::default();
        store.open(path.clone()).await;
        store.discover([PathBuf::from("/wall.jpg")]).await;
        store.favorite(Path::new("/wall.jpg"), true).await.unwrap();
        store.activate(Path::new("/wall.jpg")).await;
        store.activate(Path::new("/wall.jpg")).await;
        {
            let mut inner = store.0.lock().await;
            inner.active.as_mut().unwrap().1 -= std::time::Duration::from_secs(5);
        }
        store.checkpoint().await;
        let restored = LibraryStore::default();
        restored.open(path).await;
        let snapshot = restored.snapshot(0, 128).await;
        assert!(snapshot.entries[0].favorite);
        assert_eq!(snapshot.entries[0].play_count, 1);
        assert_eq!(snapshot.entries[0].display_seconds, 5);
        assert!(restored.0.lock().await.active.is_none());
    }
    #[tokio::test]
    async fn test_corrupt_or_future_index_is_never_overwritten() {
        for content in [
            "broken json",
            "{\"schema_version\":99,\"entries\":[]}",
            r#"{"schema_version":1,"entries":[{"path":"relative.jpg","favorite":false,"added_at":0,"last_used":null,"play_count":0,"display_seconds":0}]}"#,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("library.json");
            std::fs::write(&path, content).unwrap();
            let store = LibraryStore::default();
            store.open(path.clone()).await;
            store.discover([PathBuf::from("/wall.jpg")]).await;
            assert!(store.favorite(Path::new("/wall.jpg"), true).await.is_err());
            store.activate(Path::new("/wall.jpg")).await;
            store.checkpoint().await;
            assert_eq!(std::fs::read_to_string(path).unwrap(), content);
            assert!(store.snapshot(0, 128).await.persistence_error.is_some());
        }
    }
    #[tokio::test]
    async fn test_failed_save_rolls_back_and_paging_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index");
        std::fs::write(&path, "{\"schema_version\":1,\"entries\":[]}").unwrap();
        let store = LibraryStore::default();
        store.open(path.clone()).await;
        store
            .discover((0..260).map(|n| PathBuf::from(format!("/wall-{n:03}.jpg"))))
            .await;
        let first = store.snapshot(0, 9999).await;
        assert_eq!(first.entries.len(), 128);
        assert_eq!(first.next_offset, Some(128));
        let last = store.snapshot(256, 128).await;
        assert_eq!(last.entries.len(), 4);
        assert_eq!(last.next_offset, None);
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(
            store
                .favorite(Path::new("/wall-000.jpg"), true)
                .await
                .is_err()
        );
        assert!(!store.snapshot(0, 1).await.entries[0].favorite);
    }
}
