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
    sync::{mpsc, Arc, Mutex, MutexGuard, TryLockError},
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
                let (paths, generation, prefetch_settings) = {
                    let mut state = control.lock().unwrap();
                    let count = if gate.idle()
                        && state.settings.enabled
                        && !state.clear
                        && state.settings.max_bytes > 0
                    {
                        state.settings.decode_threads.min(state.queue.len())
                    } else {
                        0
                    };
                    (
                        state.queue.drain(..count).collect::<Vec<_>>(),
                        state.generation,
                        state.settings.clone(),
                    )
                };
                let prefetched = !paths.is_empty();
                if prefetched {
                    status.lock().unwrap().prefetching = paths.clone();
                    let results = prefetch_group(&paths, |path, publication| {
                        background::interruptible(gate.clone(), || {
                            prefetch(&root, path, &prefetch_settings, publication, || {
                                let current = control.lock().unwrap();
                                current.generation == generation && !current.clear
                            })
                        })
                    });
                    let mut current = control.lock().unwrap();
                    if current.generation == generation && !current.clear {
                        // Completion order never changes browser order. Interrupted
                        // attempts go back ahead of rows that have not started yet.
                        for (path, result) in paths.into_iter().zip(results).rev() {
                            if result.is_err_and(|error| error.is::<background::Interrupted>()) {
                                current.queue.push_front(path);
                            }
                        }
                    }
                    status.lock().unwrap().prefetching.clear();
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

/// A bounded group owns its partial decodes only until completion or cancellation.
/// Publication is serialized so concurrent photos cannot race LRU budget checks.
fn prefetch_group(
    paths: &[PathBuf],
    work: impl Fn(&Path, &Mutex<()>) -> Result<()> + Sync,
) -> Vec<Result<()>> {
    let publication = Mutex::new(());
    std::thread::scope(|scope| {
        let workers: Vec<_> = paths
            .iter()
            .map(|path| {
                let work = &work;
                let publication = &publication;
                scope.spawn(move || work(path, publication))
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| "cache prefetch worker panicked".into())
                    .and_then(|result| result)
            })
            .collect()
    })
}

// A completed speculative decode must also unwind while another photo owns
// publication. Blocking on Mutex::lock would retain its buffers during foreground work.
fn publication_lock(publication: &Mutex<()>) -> Result<MutexGuard<'_, ()>> {
    const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(1);
    loop {
        background::checkpoint_or_cancel()?;
        match publication.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(TryLockError::Poisoned(_)) => return Err("cache publication lock poisoned".into()),
            Err(TryLockError::WouldBlock) => std::thread::sleep(LOCK_RETRY_INTERVAL),
        }
    }
}

fn prefetch(
    root: &Path,
    path: &Path,
    settings: &Settings,
    publication: &Mutex<()>,
    current: impl Fn() -> bool,
) -> Result<()> {
    crate::background::checkpoint_or_cancel()?;
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
    // Keep preview publication under the same LRU lock as decoded sources.
    let preview_result = {
        let _publication = publication_lock(publication)?;
        previews::embedded(root, path, family, settings, &current)
    };
    if let Err(error) = preview_result {
        if error.is::<background::Interrupted>() {
            return Err(error);
        }
    }
    if !current() || ready(root, path, settings) {
        return Ok(());
    }
    let identity = Identity::read(path)?;
    let source = Source::open(path)?;
    crate::background::checkpoint_or_cancel()?;
    let _publication = publication_lock(publication)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Barrier,
    };

    #[test]
    fn photo_workers_run_concurrently_and_publication_waits_cancel() {
        const WORKERS: usize = 3;
        let paths: Vec<_> = (0..WORKERS).map(|i| PathBuf::from(i.to_string())).collect();
        let rendezvous = Barrier::new(WORKERS);
        let active = AtomicUsize::new(0);
        let gate = Arc::new(Gate::default());
        let results = prefetch_group(&paths, |_, publication| {
            background::interruptible(gate.clone(), || {
                active.fetch_add(1, Ordering::SeqCst);
                rendezvous.wait();
                assert_eq!(active.load(Ordering::SeqCst), WORKERS);
                let _busy = publication.lock().unwrap();
                // Even when publication is unavailable, foreground work cancels
                // this speculative worker rather than leaving it blocked with a source.
                let _foreground = gate.foreground();
                assert!(publication_lock(publication)
                    .unwrap_err()
                    .is::<background::Interrupted>());
                Ok(())
            })
        });
        assert!(results.into_iter().all(|r| r.is_ok()));
    }
}
