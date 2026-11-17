//! Owns Perfetto sessions on one control thread and saves bounded captures outside the frontend.
//!
//! Session configuration follows the Perfetto Rust SDK tracing_session example
//! (Apache-2.0, Copyright 2025 Rivos Inc.).

use super::*;
use crate::source::Result;
use perfetto_sdk::{
    heap_buffer::HeapBuffer,
    pb_msg::{PbMsg, PbMsgWriter},
    producer::{Backends, Producer, ProducerInitArgsBuilder},
    protos::config::{
        data_source_config::DataSourceConfig,
        trace_config::{
            BufferConfigFillPolicy, TraceConfig, TraceConfigBufferConfig, TraceConfigDataSource,
            TraceConfigIncrementalStateConfig,
        },
        track_event::track_event_config::TrackEventConfig,
    },
    tracing_session::TracingSession,
    track_event::TrackEvent,
};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::AtomicBool,
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex, OnceLock,
    },
};

#[derive(Clone, Copy, Debug)]
pub struct CaptureConfig {
    pub mode: Mode,
    pub buffer_kib: u32,
}
impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            mode: Mode::Wall,
            buffer_kib: 128 * 1024,
        }
    }
}
#[derive(Debug)]
pub struct SavedCapture {
    pub directory: PathBuf,
    pub trace: PathBuf,
    pub manifest: PathBuf,
    pub summary: PathBuf,
    pub trace_bytes: u64,
}
type Reply<T> = mpsc::Sender<Result<T>>;
enum Command {
    Start(CaptureConfig, Reply<()>),
    Save(
        PathBuf,
        serde_json::Value,
        Option<usize>,
        Reply<SavedCapture>,
    ),
}
/// Clones share one bounded control mailbox. SDK waits and file output happen
/// only on this controller's thread. Callers poll the returned reply channels.
#[derive(Clone)]
pub struct Recorder(SyncSender<Command>);
static CLAIMED: AtomicBool = AtomicBool::new(false);
static INITIALIZED: OnceLock<std::result::Result<(), String>> = OnceLock::new();
impl Default for Recorder {
    fn default() -> Self {
        Self::new()
    }
}
impl Recorder {
    pub fn new() -> Self {
        let (send, receive) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("diagnostics-control".into())
            .spawn(move || {
                let mut active: Option<(TracingSession, CaptureConfig)> = None;
                while let Ok(command) = receive.recv() {
                    match command {
                        Command::Start(config, reply) => {
                            let result = if active.is_some() {
                                Err("a diagnostic capture is already active".into())
                            } else {
                                start(config).map(|session| {
                                    active = Some((session, config));
                                })
                            };
                            let _ = reply.send(result);
                        }
                        Command::Save(directory, manifest, keep, reply) => {
                            let result = active
                                .take()
                                .ok_or_else(|| "no diagnostic capture is active".into())
                                .and_then(|(mut session, config)| {
                                    event(Category::Scenario, "capture.stop", |_| {});
                                    stop(&mut session);
                                    let producer = manifest.get("application").and_then(|value| value.as_str()).map(str::to_owned);
                                    let result = save(&mut session, config, directory, manifest, keep);
                                    CLAIMED.store(false, Ordering::Release);
                                    if let (Ok(saved), Some(keep), Some(producer)) = (&result, keep, producer) {
                                        if let Err(error) = prune_captures(&saved.directory, &producer, keep) {
                                            eprintln!("Capture saved, but older captures could not be removed: {error}");
                                        }
                                    }
                                    result
                                });
                            let _ = reply.send(result);
                        }
                    }
                }
                if let Some((mut session, _)) = active {
                    stop(&mut session);
                    CLAIMED.store(false, Ordering::Release);
                }
            })
            .expect("create diagnostics control thread");
        Self(send)
    }
    pub fn start(&self, config: CaptureConfig) -> Result<Receiver<Result<()>>> {
        let (send, receive) = mpsc::channel();
        self.0
            .try_send(Command::Start(config, send))
            .map_err(|_| "diagnostics control is busy or closed")?;
        Ok(receive)
    }
    pub fn stop_save(
        &self,
        directory: PathBuf,
        manifest: serde_json::Value,
    ) -> Result<Receiver<Result<SavedCapture>>> {
        self.request_save(directory, manifest, None)
    }
    /// Save and retain the newest completed captures from this producer in the same parent.
    /// Cleanup runs after successful saving, on the recorder's existing control thread.
    pub fn stop_save_retaining(
        &self,
        directory: PathBuf,
        manifest: serde_json::Value,
        keep: usize,
    ) -> Result<Receiver<Result<SavedCapture>>> {
        if keep == 0 {
            return Err("capture retention must keep at least one capture".into());
        }
        if manifest
            .get("application")
            .and_then(|value| value.as_str())
            .is_none()
        {
            return Err("retained captures require an application identity".into());
        }
        self.request_save(directory, manifest, Some(keep))
    }
    fn request_save(
        &self,
        directory: PathBuf,
        manifest: serde_json::Value,
        keep: Option<usize>,
    ) -> Result<Receiver<Result<SavedCapture>>> {
        let (send, receive) = mpsc::channel();
        self.0
            .try_send(Command::Save(directory, manifest, keep, send))
            .map_err(|_| "diagnostics control is busy or closed")?;
        Ok(receive)
    }
}

fn prune_captures(current: &Path, producer: &str, keep: usize) -> Result<()> {
    let parent = current.parent().ok_or("capture directory has no parent")?;
    let mut completed = Vec::new();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || entry.path() == current {
            continue;
        }
        let path = entry.path();
        // Recognize our completed bundles before removing any directory or derived reports.
        let owned = (|| -> Result<bool> {
            let manifest: serde_json::Value =
                serde_json::from_reader(File::open(path.join("manifest.json"))?)?;
            let summary: serde_json::Value =
                serde_json::from_reader(File::open(path.join("summary.json"))?)?;
            Ok(manifest["backend"] == "perfetto-sdk"
                && manifest["application"]["application"] == producer
                && manifest["retention"]["keep_latest"]
                    .as_u64()
                    .is_some_and(|keep| keep > 0)
                && summary["complete"] == true)
        })()
        .unwrap_or(false);
        if owned {
            if let Ok(trace) = fs::symlink_metadata(path.join("trace.pftrace")) {
                if trace.is_file() {
                    completed.push((trace.modified()?, path));
                }
            }
        }
    }
    completed.sort_by(|left, right| right.cmp(left));
    for (_, path) in completed.into_iter().skip(keep - 1) {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn initialize() -> Result<()> {
    INITIALIZED
        .get_or_init(|| {
            Producer::init(
                ProducerInitArgsBuilder::new()
                    .backends(Backends::IN_PROCESS)
                    .shmem_size_hint_kb(1024)
                    .build(),
            );
            TrackEvent::init();
            categories::register().map_err(|error| error.to_string())
        })
        .as_ref()
        .map(|_| ())
        .map_err(|error| error.clone().into())
}
fn start(config: CaptureConfig) -> Result<TracingSession> {
    if config.mode == Mode::Off || !(64..=256 * 1024).contains(&config.buffer_kib) {
        return Err("capture requires Wall/Gpu mode and 64..262144 KiB buffer".into());
    }
    if CLAIMED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("a diagnostic capture is already active".into());
    }
    let result = (|| {
        let transition = EMISSION
            .write()
            .map_err(|_| "capture emission gate poisoned")?;
        initialize()?;
        let writer = PbMsgWriter::new();
        let heap = HeapBuffer::new(writer.stream_writer());
        let mut message = PbMsg::new(&writer)?;
        {
            let mut trace = TraceConfig { msg: &mut message };
            trace.set_buffers(|buffer: &mut TraceConfigBufferConfig| {
                buffer.set_size_kb(config.buffer_kib);
                buffer.set_fill_policy(BufferConfigFillPolicy::RingBuffer);
            });
            // Re-emit descriptors so the recent tail remains readable after the
            // ring overwrites earlier string and thread metadata.
            trace.set_incremental_state_config(|state: &mut TraceConfigIncrementalStateConfig| {
                state.set_clear_period_ms(1000);
            });
            trace.set_data_sources(|source: &mut TraceConfigDataSource| {
                source.set_config(|source: &mut DataSourceConfig| {
                    source.set_name("track_event");
                    source.set_track_event_config(|events: &mut TrackEventConfig| {
                        events.set_enabled_categories("foveonic.*");
                    });
                });
            });
        }
        message.finalize();
        let mut bytes = vec![0; writer.stream_writer().get_written_size()];
        heap.copy_into(&mut bytes);
        let mut session = TracingSession::in_process()?;
        session.setup(&bytes);
        session.start_blocking();
        GENERATION.fetch_add(1, Ordering::AcqRel);
        TRANSITION_DROPS.store(0, Ordering::Relaxed);
        drop(transition);
        MODE.store(
            if config.mode == Mode::Gpu { 2 } else { 1 },
            Ordering::Release,
        );
        event(Category::Scenario, "capture.start", |fields| {
            fields
                .u64("capture_id", capture_id().unwrap())
                .u64("buffer_kib", config.buffer_kib as u64);
        });
        Ok(session)
    })();
    if result.is_err() {
        CLAIMED.store(false, Ordering::Release);
    }
    result
}

fn stop(session: &mut TracingSession) {
    // Close admission before draining emitters already accepted by this capture.
    MODE.store(0, Ordering::Release);
    let _transition = EMISSION.write().expect("capture emission gate");
    session.stop_blocking();
}

fn save(
    session: &mut TracingSession,
    config: CaptureConfig,
    directory: PathBuf,
    manifest: serde_json::Value,
    keep: Option<usize>,
) -> Result<SavedCapture> {
    fs::create_dir_all(&directory)?;
    let trace = directory.join("trace.pftrace");
    // A capture never silently overwrites an earlier measurement.
    let file = File::options().write(true).create_new(true).open(&trace)?;
    let output = Arc::new(Mutex::new((file, None::<std::io::Error>, 0_u64)));
    let sink = output.clone();
    session.read_trace_blocking(move |bytes, _| {
        let mut output = sink.lock().unwrap();
        if output.1.is_none() {
            match output.0.write_all(bytes) {
                Ok(()) => output.2 += bytes.len() as u64,
                Err(error) => output.1 = Some(error),
            }
        }
    });
    let mut output = output.lock().unwrap();
    if let Some(error) = output.1.take() {
        return Err(error.into());
    }
    output.0.flush()?;
    let trace_bytes = output.2;
    let manifest_path = directory.join("manifest.json");
    let summary = directory.join("summary.json");
    let metadata = serde_json::json!({"schema_version":1,"backend":"perfetto-sdk","backend_version":"1.2.1",
        "mode":config.mode,"buffer_kib":config.buffer_kib,"cpu_clock":"perfetto","gpu_clock":"uncalibrated",
        "executable":executable_fingerprint(),"application":manifest,
        "retention":keep.map(|keep| serde_json::json!({"keep_latest":keep}))});
    let results = serde_json::json!({"schema_version":1,"trace_bytes":trace_bytes,"mode":config.mode,
        "buffer_kib":config.buffer_kib,"complete":true,"loss_checked":false,
        "emission_transition_drops":TRANSITION_DROPS.load(Ordering::Relaxed),
        "gpu_coverage_checked":false});
    serde_json::to_writer_pretty(
        File::options()
            .write(true)
            .create_new(true)
            .open(&manifest_path)?,
        &metadata,
    )?;
    serde_json::to_writer_pretty(
        File::options()
            .write(true)
            .create_new(true)
            .open(&summary)?,
        &results,
    )?;
    Ok(SavedCapture {
        directory,
        trace,
        manifest: manifest_path,
        summary,
        trace_bytes,
    })
}

/// This fingerprints an opened file on disk, not the already loaded code image.
/// It is optional provenance and never makes otherwise usable trace saving fail.
fn executable_fingerprint() -> serde_json::Value {
    use sha2::{Digest, Sha256};
    let result = (|| -> Result<serde_json::Value> {
        let path = std::env::current_exe()?;
        let mut file = File::open(&path)?;
        let before = file.metadata()?;
        let mut hash = Sha256::new();
        let mut bytes = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hash.update(&buffer[..read]);
            bytes += read as u64;
        }
        let after = file.metadata()?;
        if bytes != before.len()
            || before.len() != after.len()
            || before.modified()? != after.modified()?
        {
            return Err("executable file changed while fingerprinting".into());
        }
        Ok(
            serde_json::json!({"available":true,"path":path,"bytes":bytes,
            "sha256":format!("{:x}",hash.finalize()),
            "modified_unix_ns":before.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_nanos(),
            "fingerprint_scope":"on-disk-executable-at-save","loaded_image_attested":false}),
        )
    })();
    result.unwrap_or_else(|error| {
        serde_json::json!({"available":false,
        "fingerprint_scope":"on-disk-executable-at-save","loaded_image_attested":false,
        "error":error.to_string()})
    })
}

#[cfg(test)]
mod retention_tests {
    use super::*;

    fn bundle(parent: &Path, name: &str, owner: &str, complete: bool, order: u64) -> PathBuf {
        let path = parent.join(name);
        fs::create_dir(&path).unwrap();
        fs::write(
            path.join("manifest.json"),
            serde_json::to_vec(&serde_json::json!({
                "backend":"perfetto-sdk", "application":{"application":owner},
                "retention":{"keep_latest":4}
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            path.join("summary.json"),
            serde_json::to_vec(&serde_json::json!({"complete":complete})).unwrap(),
        )
        .unwrap();
        let trace = File::create(path.join("trace.pftrace")).unwrap();
        trace
            .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(order))
            .unwrap();
        fs::create_dir(path.join("analysis")).unwrap();
        fs::write(path.join("analysis/report.json"), "derived").unwrap();
        path
    }

    #[test]
    fn keeps_current_and_three_newest_owned_bundles_without_touching_other_data() {
        let temp = tempfile::tempdir().unwrap();
        let old: Vec<_> = (1..=5)
            .map(|order| bundle(temp.path(), &format!("run-{order}"), "app", true, order))
            .collect();
        let current = bundle(temp.path(), "current", "app", true, 0);
        let foreign = bundle(temp.path(), "foreign", "other-app", true, 0);
        let partial = bundle(temp.path(), "partial", "app", false, 0);
        let missing = bundle(temp.path(), "missing-trace", "app", true, 0);
        fs::remove_file(missing.join("trace.pftrace")).unwrap();
        let scripted = bundle(temp.path(), "scripted", "app", true, 0);
        let mut manifest: serde_json::Value =
            serde_json::from_reader(File::open(scripted.join("manifest.json")).unwrap()).unwrap();
        manifest["retention"] = serde_json::Value::Null;
        fs::write(
            scripted.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(temp.path().join("notes.txt"), "keep").unwrap();
        prune_captures(&current, "app", 4).unwrap();
        assert!(!old[0].exists() && !old[1].exists());
        assert!(old[2..]
            .iter()
            .all(|path| path.join("analysis/report.json").is_file()));
        assert!([current, foreign, partial, missing, scripted]
            .iter()
            .all(|path| path.is_dir()));
        assert!(temp.path().join("notes.txt").is_file());
    }

    #[test]
    fn keeping_one_protects_new_capture_even_if_its_clock_is_older() {
        let temp = tempfile::tempdir().unwrap();
        let old = bundle(temp.path(), "old", "app", true, 10);
        let current = bundle(temp.path(), "current", "app", true, 1);
        prune_captures(&current, "app", 1).unwrap();
        assert!(current.is_dir());
        assert!(!old.exists());
    }
}
