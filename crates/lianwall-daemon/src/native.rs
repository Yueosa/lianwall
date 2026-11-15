//! Experimental in-process image/software-video renderer. All native objects stay on one OS thread.
use lianwall_core::config::{VideoDecode, WallMode};
use lianwall_core::socket::{
    DecoderKind, EngineOperation, EngineStatistics, NativePhase, NativeRuntime,
};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    io::{Read, Write},
    os::fd::AsRawFd,
    os::unix::{ffi::OsStrExt, net::UnixStream},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, oneshot};
struct RuntimeState {
    value: std::sync::RwLock<NativeRuntime>,
    updates: Option<broadcast::Sender<NativeRuntime>>,
}
type RuntimeCache = Arc<RuntimeState>;
fn update_runtime(cache: &RuntimeCache, update: impl FnOnce(&mut NativeRuntime)) {
    let mut snapshot = cache.value.write().unwrap_or_else(|e| e.into_inner());
    update(&mut snapshot);
    snapshot.revision = snapshot.revision.saturating_add(1);
    let value = snapshot.clone();
    drop(snapshot);
    if let Some(updates) = &cache.updates {
        let _ = updates.send(value);
    }
}
impl From<Statistics> for EngineStatistics {
    fn from(s: Statistics) -> Self {
        Self {
            decodes: s.decodes,
            uploads: s.uploads,
            cache_hits: s.cache_hits,
            frames: s.frames,
            video_frames: s.video_frames,
            video_dropped: s.video_dropped,
            video_loops: s.video_loops,
            video_hardware_frames: s.video_hardware_frames,
            video_software_frames: s.video_software_frames,
            video_fallbacks: s.video_fallbacks,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Options {
    pub fit: c_int,
    pub duration_ms: c_int,
    pub fps: c_int,
}
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Statistics {
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
unsafe extern "C" {
    fn lw_create(wake_fd: c_int) -> *mut c_void;
    fn lw_show(
        engine: *mut c_void,
        path: *const c_char,
        output: *const c_char,
        options: Options,
    ) -> c_int;
    fn lw_show_video_transition(
        engine: *mut c_void,
        path: *const c_char,
        output: *const c_char,
        options: Options,
        decode: c_int,
    ) -> c_int;
    fn lw_decoder_kind(engine: *mut c_void) -> c_int;
    fn lw_fallback_reason(engine: *mut c_void) -> *const c_char;
    fn lw_decode_diagnostic(engine: *mut c_void) -> *const c_char;
    fn lw_pump(engine: *mut c_void, timeout: c_int) -> c_int;
    fn lw_hide(engine: *mut c_void);
    fn lw_destroy(engine: *mut c_void);
    fn lw_error(engine: *mut c_void) -> *const c_char;
    fn lw_statistics(engine: *mut c_void) -> Statistics;
}
type Reply = oneshot::Sender<Result<Statistics, String>>;
enum Command {
    Show(CString, CString, Options, Option<c_int>, Reply),
    Hide(Reply),
    Statistics(Reply),
}
pub struct NativeEngine {
    runtime: RuntimeCache,
    sender: mpsc::SyncSender<Command>,
    wake: Mutex<UnixStream>,
    active: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}
impl NativeEngine {
    pub fn new() -> anyhow::Result<Self> {
        Self::build(None)
    }
    pub fn with_updates(updates: broadcast::Sender<NativeRuntime>) -> anyhow::Result<Self> {
        Self::build(Some(updates))
    }
    fn build(updates: Option<broadcast::Sender<NativeRuntime>>) -> anyhow::Result<Self> {
        let (mut reader, writer) = UnixStream::pair()?;
        reader.set_nonblocking(true)?;
        writer.set_nonblocking(true)?;
        let (sender, receiver) = mpsc::sync_channel(8);
        let active = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));
        let stop = Arc::new(AtomicBool::new(false));
        let runtime = Arc::new(RuntimeState {
            value: std::sync::RwLock::new(NativeRuntime::default()),
            updates,
        });
        let worker_runtime = runtime.clone();
        let worker_stop = stop.clone();
        let (worker_active, worker_alive) = (active.clone(), alive.clone());
        let worker = thread::Builder::new()
            .name("native-media".into())
            .spawn(move || {
                // SAFETY: native objects stay on this thread. Commands own their
                // C strings; the wake socket outlives the native context.
                unsafe {
                    let engine = lw_create(reader.as_raw_fd());
                    if !engine.is_null() {
                        let mut sampled = Instant::now();
                        'worker: loop {
                            if worker_stop.load(Ordering::Acquire) {
                                break;
                            }
                            let mut bytes = [0u8; 64];
                            while reader.read(&mut bytes).is_ok_and(|count| count > 0) {}
                            while let Ok(command) = receiver.try_recv() {
                                while reader.read(&mut bytes).is_ok_and(|count| count > 0) {}
                                if worker_stop.load(Ordering::Acquire) {
                                    break 'worker;
                                }
                                match command {
                                    Command::Statistics(reply) => {
                                        let _ = reply.send(Ok(lw_statistics(engine)));
                                    }
                                    Command::Hide(reply) => {
                                        lw_hide(engine);
                                        update_runtime(&worker_runtime, |r| {
                                            let revision = r.revision;
                                            *r = NativeRuntime { revision, statistics: lw_statistics(engine).into(), ..Default::default() };
                                        });
                                        worker_active.store(false, Ordering::Release);
                                        let _ = reply.send(Ok(lw_statistics(engine)));
                                    }
                                    Command::Show(path, output, options, video, reply) => {
                                        let requested = video.map(|v| match v { 1 => VideoDecode::Vulkan, 2 => VideoDecode::Auto, _ => VideoDecode::Software });
                                        let selected_path = std::path::PathBuf::from(std::ffi::OsStr::from_bytes(path.as_bytes()));
                                        let selected_output = output.to_string_lossy().into_owned();
                                        let media = if video.is_some() { WallMode::Video } else { WallMode::Image };
                                        update_runtime(&worker_runtime, |r| {
                                            r.phase = NativePhase::Preparing;
                                            r.pending = Some(EngineOperation { path: selected_path.clone(), output: selected_output.clone(), media, requested_decode: requested });
                                            r.last_error = None;
                                        });
                                        let result = if let Some(decode) = video {
                                            lw_show_video_transition(
                                                engine,
                                                path.as_ptr(),
                                                output.as_ptr(),
                                                options,
                                                decode,
                                            )
                                        } else {
                                            lw_show(engine, path.as_ptr(), output.as_ptr(), options)
                                        };
                                        let response = if result == 0 {
                                            worker_active.store(true, Ordering::Release);
                                            let stats = lw_statistics(engine);
                                            if video.is_some() {
                                                tracing::info!(decode = %CStr::from_ptr(lw_decode_diagnostic(engine)).to_string_lossy(), frames = stats.video_frames, dropped = stats.video_dropped, hardware_frames = stats.video_hardware_frames, software_frames = stats.video_software_frames, fallbacks = stats.video_fallbacks, "Native video decoder selected");
                                            }
                                            Ok(stats)
                                        } else {
                                            Err(CStr::from_ptr(lw_error(engine))
                                                .to_string_lossy()
                                                .into_owned())
                                        };
                                        update_runtime(&worker_runtime, |r| {
                                            r.pending = None;
                                            r.statistics = lw_statistics(engine).into();
                                            if result == 0 {
                                                r.phase = NativePhase::Running;
                                                r.current = Some(selected_path);
                                                r.output = Some(selected_output);
                                                r.media = Some(media);
                                                r.requested_decode = requested;
                                                r.decoder = match lw_decoder_kind(engine) { 1 => Some(DecoderKind::Software), 2 => Some(DecoderKind::Vulkan), _ => None };
                                                let reason = CStr::from_ptr(lw_fallback_reason(engine)).to_string_lossy();
                                                r.fallback_reason = (!reason.is_empty()).then(|| reason.into_owned());
                                            } else {
                                                r.phase = if result == -2 { NativePhase::Failed } else if worker_active.load(Ordering::Acquire) { NativePhase::Running } else { NativePhase::Idle };
                                                r.last_error = response.as_ref().err().cloned();
                                            }
                                        });
                                        let _ = reply.send(response);
                                        if result == -2 {
                                            break 'worker;
                                        }
                                    }
                                }
                            }
                            if lw_pump(engine, -1) < 0 {
                                tracing::warn!(
                                    error = %CStr::from_ptr(lw_error(engine)).to_string_lossy(),
                                    "Native media renderer stopped"
                                );
                                update_runtime(&worker_runtime, |r| {
                                    r.phase = NativePhase::Failed;
                                    r.last_error = Some(CStr::from_ptr(lw_error(engine)).to_string_lossy().into_owned());
                                });
                                break;
                            }
                            if sampled.elapsed() >= Duration::from_secs(1) {
                                worker_runtime.value.write().unwrap_or_else(|e| e.into_inner()).statistics = lw_statistics(engine).into();
                                sampled = Instant::now();
                            }
                        }
                        lw_destroy(engine);
                    }
                }
                update_runtime(&worker_runtime, |r| {
                    if r.phase != NativePhase::Failed {
                        r.phase = if worker_stop.load(Ordering::Acquire) { NativePhase::Stopped } else { NativePhase::Failed };
                        if r.phase == NativePhase::Failed { r.last_error = Some("Native media owner could not start".into()); }
                    }
                    r.current = None;
                    r.output = None;
                    r.media = None;
                    r.pending = None;
                    r.decoder = None;
                    r.requested_decode = None;
                    r.fallback_reason = None;
                });
                worker_active.store(false, Ordering::Release);
                worker_alive.store(false, Ordering::Release);
            })?;
        Ok(Self {
            runtime,
            sender,
            wake: Mutex::new(writer),
            active,
            alive,
            stop,
            worker: Mutex::new(Some(worker)),
        })
    }
    /// Does not send a worker command or touch its wake socket.
    pub fn runtime(&self) -> NativeRuntime {
        self.runtime
            .value
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    fn send(&self, command: Command) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.alive.load(Ordering::Acquire) && !self.stop.load(Ordering::Acquire),
            "Native media worker stopped; restart the daemon"
        );
        self.sender
            .try_send(command)
            .map_err(|e| anyhow::anyhow!("Native media queue: {e}"))?;
        let _ = self
            .wake
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .write(&[1]);
        Ok(())
    }
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
    pub async fn show(
        &self,
        path: &Path,
        output: &str,
        options: Options,
    ) -> anyhow::Result<Statistics> {
        anyhow::ensure!(
            supported(path),
            "Unsupported native image extension: {}",
            path.display()
        );
        anyhow::ensure!(
            (0..=2000).contains(&options.duration_ms) && (1..=120).contains(&options.fps),
            "Invalid native transition duration or frame rate"
        );
        let path = CString::new(path.as_os_str().as_bytes())?;
        let output = CString::new(output)?;
        let (tx, rx) = oneshot::channel();
        self.send(Command::Show(path, output, options, None, tx))?;
        rx.await
            .map_err(|_| anyhow::anyhow!("Native media worker exited"))?
            .map_err(anyhow::Error::msg)
    }
    pub async fn show_video(
        &self,
        path: &Path,
        output: &str,
        fit: i32,
    ) -> anyhow::Result<Statistics> {
        self.show_video_with_decode(path, output, fit, 0).await
    }
    /// decode: 0 software, 1 required Vulkan, 2 first-frame software fallback.
    pub async fn show_video_with_decode(
        &self,
        path: &Path,
        output: &str,
        fit: i32,
        decode: i32,
    ) -> anyhow::Result<Statistics> {
        self.show_video_with_options(
            path,
            output,
            Options {
                fit,
                duration_ms: 0,
                fps: 60,
            },
            decode,
        )
        .await
    }
    pub async fn show_video_with_options(
        &self,
        path: &Path,
        output: &str,
        options: Options,
        decode: i32,
    ) -> anyhow::Result<Statistics> {
        anyhow::ensure!(
            (0..=2000).contains(&options.duration_ms) && (1..=120).contains(&options.fps),
            "Invalid native transition duration or frame rate"
        );
        anyhow::ensure!((0..=2).contains(&decode), "Invalid decoder policy");
        anyhow::ensure!(
            lianwall_core::wallpaper::is_video_path(path),
            "Unsupported native video extension"
        );
        let path = CString::new(path.as_os_str().as_bytes())?;
        let output = CString::new(output)?;
        let (tx, rx) = oneshot::channel();
        self.send(Command::Show(path, output, options, Some(decode), tx))?;
        rx.await
            .map_err(|_| anyhow::anyhow!("Native media worker exited"))?
            .map_err(anyhow::Error::msg)
    }
    pub async fn statistics(&self) -> anyhow::Result<Statistics> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Statistics(tx))?;
        rx.await
            .map_err(|_| anyhow::anyhow!("Native media worker exited"))?
            .map_err(anyhow::Error::msg)
    }
    pub async fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);
        let _ = self
            .wake
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .write(&[1]);
        let worker = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(worker) = worker {
            let _ = tokio::task::spawn_blocking(move || worker.join()).await;
        }
    }
    pub async fn hide(&self) -> anyhow::Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(Command::Hide(tx))?;
        rx.await
            .map_err(|_| anyhow::anyhow!("Native media worker exited"))?
            .map_err(anyhow::Error::msg)?;
        Ok(())
    }
}
impl Drop for NativeEngine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self
            .wake
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .write(&[1]);
        // Last-resort ownership barrier (including unwinding). Normal daemon
        // shutdown uses the asynchronous join above. Never leave a GPU thread
        // running while process/library teardown may unload its driver.
        if let Some(worker) = self
            .worker
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = worker.join();
        }
    }
}
pub const FORMATS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "webp", "tif", "tiff", "pnm", "ppm", "pgm", "pbm", "pam",
    "tga", "farbfeld",
];
fn supported(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| FORMATS.contains(&e.to_ascii_lowercase().as_str()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats_are_explicit_and_case_insensitive() {
        for extension in FORMATS {
            assert!(supported(Path::new(&format!("wall.{extension}"))));
        }
        assert!(supported(Path::new("wall.PNG")));
        assert!(!supported(Path::new("video.mp4")));
        assert!(!supported(Path::new("wall.avif")));
        assert!(!supported(Path::new("wall.svg")));
    }
}
