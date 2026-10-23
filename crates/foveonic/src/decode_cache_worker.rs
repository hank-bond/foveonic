//! Schedules bounded cache writes, folder prefetch, and readiness scans behind foreground work.

use super::{
    previews, prune, read_header, ready, store_checked, Command, Control, EntryState, Identity,
    Settings, Status, NAMESPACE,
};
use crate::background::{self, Gate};
use crate::source::{ProcessingFamily, Result, Source};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};

// Wake often enough to notice folder/policy changes without spinning while idle.
const WORK_POLL_INTERVAL: Duration = Duration::from_millis(100);
// External source changes and eviction need no per-frame filesystem scan.
const READINESS_SCAN_INTERVAL: Duration = Duration::from_secs(2);
// One source may wait while another is being written. Extra insertions are disposable.
const PENDING_WRITES: usize = 1;

pub(super) fn start(
    root: PathBuf,
    control: Arc<Mutex<Control>>,
    status: Arc<Mutex<Status>>,
    gate: Arc<Gate>,
) -> mpsc::SyncSender<Command> {
    let (writer, receiver) = mpsc::sync_channel(PENDING_WRITES);
    std::thread::spawn(move || {
        background::lower_priority();
        background::run(Some(gate.clone()), || {
            let mut initialized = false;
            let mut last_scan = Instant::now();
            let mut families = HashMap::new();
            loop {
                let command = match receiver.recv_timeout(WORK_POLL_INTERVAL) {
                    Ok(command) => Some(command),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                let (settings, clear) = {
                    let mut state = control.lock().unwrap();
                    (state.settings.clone(), std::mem::take(&mut state.clear))
                };
                if !initialized {
                    if let Some(parent) = root
                        .parent()
                        .filter(|p| p.file_name().is_some_and(|n| n == NAMESPACE))
                    {
                        if let Ok(entries) = fs::read_dir(parent) {
                            for entry in entries.flatten().filter(|e| e.path() != root) {
                                let _ = fs::remove_dir_all(entry.path());
                            }
                        }
                    }
                    initialized = true;
                }
                let maintenance = command.is_some() || clear;
                if clear {
                    let _ = fs::remove_dir_all(&root);
                }
                if let Some(Command::Store {
                    path,
                    identity,
                    source,
                }) = command
                {
                    if !clear
                        && source
                            .metadata
                            .processing_family()
                            .is_ok_and(|f| settings.permits(f))
                    {
                        status.lock().unwrap().writing = true;
                        let _ = store_checked(
                            &root,
                            &path,
                            identity,
                            &source,
                            settings.max_bytes,
                            || {
                                let state = control.lock().unwrap();
                                !state.clear
                                    && source
                                        .metadata
                                        .processing_family()
                                        .is_ok_and(|f| state.settings.permits(f))
                                    && state.settings.max_bytes == settings.max_bytes
                            },
                        );
                        status.lock().unwrap().writing = false;
                    }
                    control.lock().unwrap().stores.remove(&path);
                }
                if gate.idle() && !clear {
                    let preview = control.lock().unwrap().preview.take();
                    if let Some(preview) = preview {
                        let _ = previews::store(&root, preview, &control);
                    }
                }
                // Only idle time admits new prefetch work. Decode and writes
                // also yield at bounded checkpoints if a photo becomes active.
                let next = if gate.idle() && settings.enabled && settings.max_bytes > 0 {
                    let mut state = control.lock().unwrap();
                    let generation = state.generation;
                    state.queue.pop_front().map(|p| (p, generation))
                } else {
                    None
                };
                let mut prefetched = false;
                if let Some((path, generation)) = next {
                    status.lock().unwrap().prefetching = Some(path.clone());
                    let result = prefetch(&root, &path, &settings, || {
                        let current = control.lock().unwrap();
                        current.generation == generation && !current.clear
                    });
                    prefetched = result.is_ok();
                    status.lock().unwrap().prefetching = None;
                }
                if maintenance
                    || prefetched
                    || (gate.idle() && last_scan.elapsed() >= READINESS_SCAN_INTERVAL)
                {
                    refresh(&root, &control, &status, &mut families);
                    last_scan = Instant::now();
                }
            }
        });
    });
    writer
}

fn prefetch(
    root: &Path,
    path: &Path,
    settings: &Settings,
    current: impl Fn() -> bool,
) -> Result<()> {
    crate::background::checkpoint();
    if !current() {
        return Ok(());
    }
    let family = crate::source::read_capture_properties(path)?
        .get("CAMMODEL")
        .and_then(|m| ProcessingFamily::from_camera_model(m))
        .ok_or("unknown prefetch family")?;
    if !settings.permits(family) {
        return Ok(());
    }
    let _ = previews::embedded(root, path, family, settings, &current);
    if !current() || ready(root, path, settings) {
        return Ok(());
    }
    let identity = Identity::read(path)?;
    let source = Source::open(path)?;
    crate::background::checkpoint();
    if current() && !ready(root, path, settings) {
        store_checked(root, path, identity, &source, settings.max_bytes, current)?;
    }
    Ok(())
}
pub(super) type FamilyCatalog = HashMap<PathBuf, (Identity, Option<ProcessingFamily>)>;

pub(super) fn entry_state(
    root: &Path,
    path: &Path,
    settings: &Settings,
    queued: bool,
    families: &mut FamilyCatalog,
) -> EntryState {
    if !settings.enabled || settings.max_bytes == 0 {
        return EntryState::Disabled;
    }
    let Ok(identity) = Identity::read(path) else {
        return EntryState::Unavailable;
    };
    let family = if let Some((_, family)) = families.get(path).filter(|(old, _)| old == &identity) {
        *family
    } else {
        // Existing cache headers already identify the architecture. Otherwise
        // read capture properties once per source identity, away from the UI.
        let all = Settings {
            classic: true,
            max_bytes: u64::MAX,
            ..Settings::default()
        };
        let properties = File::open(root.join(identity.name()))
            .ok()
            .and_then(|mut file| read_header(&mut file, &identity, &all).ok())
            .map(|header| header.properties)
            .or_else(|| crate::source::read_capture_properties(path).ok());
        let family = properties.and_then(|p| {
            p.get("CAMMODEL")
                .and_then(|m| ProcessingFamily::from_camera_model(m))
        });
        families.insert(path.to_owned(), (identity, family));
        family
    };
    let Some(family) = family else {
        return EntryState::Unsupported;
    };
    if !settings.permits(family) {
        return EntryState::Disabled;
    }
    if ready(root, path, settings) {
        EntryState::Ready
    } else if queued {
        EntryState::Queued
    } else {
        EntryState::Uncached
    }
}
fn refresh(
    root: &Path,
    control: &Mutex<Control>,
    status: &Mutex<Status>,
    families: &mut FamilyCatalog,
) {
    crate::background::checkpoint();
    let (settings, watched, generation, queued) = {
        let state = control.lock().unwrap();
        (
            state.settings.clone(),
            state.watched.clone(),
            state.generation,
            state
                .queue
                .iter()
                .chain(state.stores.iter())
                .cloned()
                .collect::<HashSet<_>>(),
        )
    };
    let entries = prune(root, settings.max_bytes).unwrap_or_default();
    let watching = watched.iter().cloned().collect::<HashSet<_>>();
    families.retain(|p, _| watching.contains(p));
    let files: HashMap<_, _> = watched
        .into_iter()
        .map(|path| {
            crate::background::checkpoint();
            let state = entry_state(root, &path, &settings, queued.contains(&path), families);
            (path, state)
        })
        .collect();
    let ready = files
        .iter()
        .filter(|(_, state)| **state == EntryState::Ready)
        .map(|(p, _)| p.clone())
        .collect();
    let state = control.lock().unwrap();
    let mut status = status.lock().unwrap();
    status.entries = entries
        .iter()
        .filter(|e| e.0.extension().is_some_and(|s| s == "decode"))
        .count();
    status.bytes = entries.iter().map(|e| e.1).sum();
    if state.generation == generation && !state.clear {
        status.ready = ready;
        status.files = Arc::new(files);
    }
}
