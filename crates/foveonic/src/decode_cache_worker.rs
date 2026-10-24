//! Schedules bounded cache writes, selection prefetch, and readiness scans behind foreground work.

use super::{
    previews, prune, read_header, ready, store_checked, Command, Control, EntryState, Identity,
    Settings, Status, NAMESPACE,
};
use crate::background::{self, Gate};
use crate::source::{ProcessingFamily, Result};
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
            let mut decoder_pool: Option<Arc<crate::decode_pool::Pool>> = None;
            loop {
                let work_ready = {
                    let state = control.lock().unwrap();
                    gate.idle()
                        && state.settings.enabled
                        && state.settings.max_bytes > 0
                        && !state.queue.is_empty()
                };
                // A queued neighborhood needs no idle poll delay between worker groups.
                let wait = if work_ready {
                    Duration::ZERO
                } else {
                    WORK_POLL_INTERVAL
                };
                let command = match receiver.recv_timeout(wait) {
                    Ok(command) => Some(command),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                let (settings, clear, nearby, generation) = {
                    let mut state = control.lock().unwrap();
                    (
                        state.settings.clone(),
                        std::mem::take(&mut state.clear),
                        state.nearby.clone(),
                        state.generation,
                    )
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
                    original,
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
                            identity.clone(),
                            &source,
                            settings.max_bytes,
                            &nearby,
                            || {
                                let state = control.lock().unwrap();
                                !state.clear
                                    && state.generation == generation
                                    && source
                                        .metadata
                                        .processing_family()
                                        .is_ok_and(|f| state.settings.permits(f))
                                    && state.settings.max_bytes == settings.max_bytes
                            },
                        );
                        let current = || {
                            let state = control.lock().unwrap();
                            !state.clear
                                && state.generation == generation
                                && state.settings == settings
                        };
                        let _ = super::selection::prepare(
                            &root,
                            &path,
                            &identity,
                            &source.metadata,
                            &original,
                            &settings,
                            &nearby,
                            &current,
                        );
                        publish_ready(&root, &path, &settings, &control, &status, generation);
                        status.lock().unwrap().writing = false;
                    }
                    control.lock().unwrap().stores.remove(&path);
                }
                if gate.idle() && !clear {
                    loop {
                        background::checkpoint();
                        let update = {
                            let mut state = control.lock().unwrap();
                            state.recipe_updates.pop_first().and_then(|(path, recipe)| {
                                state
                                    .identities
                                    .get(&path)
                                    .cloned()
                                    .map(|identity| (identity, recipe))
                            })
                        };
                        let Some((identity, recipe)) = update else {
                            break;
                        };
                        let _ = super::selection::update_recipe(
                            &root,
                            &identity,
                            Some(recipe),
                            &settings,
                            &nearby,
                            &|| {
                                let state = control.lock().unwrap();
                                !state.clear && state.generation == generation
                            },
                        );
                    }
                    let preview = control.lock().unwrap().preview.take();
                    if let Some(preview) = preview {
                        let _ = previews::store(&root, preview, &control);
                    }
                }
                let (generation, prefetch_settings, nearby, queued) = {
                    let state = control.lock().unwrap();
                    (
                        state.generation,
                        state.settings.clone(),
                        state.nearby.clone(),
                        !state.queue.is_empty(),
                    )
                };
                let prefetched = gate.idle()
                    && prefetch_settings.enabled
                    && prefetch_settings.max_bytes > 0
                    && queued;
                if prefetched {
                    if decoder_pool
                        .as_ref()
                        .is_none_or(|pool| pool.threads() != prefetch_settings.decode_threads)
                    {
                        decoder_pool =
                            crate::decode_pool::Pool::new(prefetch_settings.decode_threads)
                                .ok()
                                .map(Arc::new);
                    }
                    if let Some(pool) = &decoder_pool {
                        background::interruptible(gate.clone(), || {
                            prefetch_pass(
                                &root,
                                &control,
                                &status,
                                pool,
                                &prefetch_settings,
                                &nearby,
                                generation,
                            );
                        });
                    } else {
                        // Cache preparation is optional if native workers cannot be created.
                        let mut state = control.lock().unwrap();
                        if state.generation == generation {
                            state.queue.clear();
                        }
                    }
                }
                if !prefetch_settings.enabled {
                    decoder_pool = None;
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

/// Photo coordinators keep a bounded number of compressed inputs in flight.
/// All sensor layers use the same FIFO pool, and completed photos free their
/// coordinator immediately. There is no group completion barrier between photos.
fn prefetch_pass(
    root: &Path,
    control: &Mutex<Control>,
    status: &Mutex<Status>,
    pool: &Arc<crate::decode_pool::Pool>,
    settings: &Settings,
    nearby: &[PathBuf],
    generation: u64,
) {
    let publication = Mutex::new(());
    let order = Arc::new(crate::decode_pool::Order::default());
    let scheduling = background::context();
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..pool.threads())
            .map(|_| {
                let publication = &publication;
                let order = &order;
                let scheduling = scheduling.clone();
                scope.spawn(move || {
                    background::with_context(scheduling, || loop {
                        if background::checkpoint_or_cancel().is_err() {
                            break;
                        }
                        let next = {
                            let mut current = control.lock().unwrap();
                            if current.generation != generation || current.clear {
                                break;
                            }
                            current.queue.pop_front().map(|path| (path, order.issue()))
                        };
                        let Some((path, turn)) = next else {
                            break;
                        };
                        status.lock().unwrap().prefetching.push(path.clone());
                        let result = background::decoder(
                            crate::decode_pool::Task {
                                pool: pool.clone(),
                                turn,
                            },
                            || {
                                prefetch(
                                    root,
                                    &path,
                                    settings,
                                    nearby,
                                    publication,
                                    || {
                                        let current = control.lock().unwrap();
                                        current.generation == generation && !current.clear
                                    },
                                    || {
                                        publish_ready(
                                            root, &path, settings, control, status, generation,
                                        )
                                    },
                                )
                            },
                        );
                        let mut current = control.lock().unwrap();
                        if current.generation == generation
                            && !current.clear
                            && result
                                .as_ref()
                                .is_err_and(|e| e.is::<background::Interrupted>())
                        {
                            // Several coordinators can unwind in any order. Restore only
                            // unfinished paths, in the current neighborhood's priority order.
                            let mut pending: HashSet<_> = current.queue.iter().cloned().collect();
                            pending.insert(path.clone());
                            current.queue = nearby
                                .iter()
                                .filter(|p| pending.contains(*p))
                                .cloned()
                                .collect();
                        }
                        status.lock().unwrap().prefetching.retain(|p| p != &path);
                    })
                })
            })
            .collect();
        for worker in workers {
            let _ = worker.join();
        }
    });
    status.lock().unwrap().prefetching.clear();
}

fn publish_ready(
    root: &Path,
    path: &Path,
    settings: &Settings,
    control: &Mutex<Control>,
    status: &Mutex<Status>,
    generation: u64,
) {
    let identity = match Identity::read(path) {
        Ok(i) => i,
        Err(_) => return,
    };
    if !complete_identity(root, &identity, settings) {
        return;
    }
    let mut current = control.lock().unwrap();
    if current.generation == generation && !current.clear {
        current.identities.insert(path.to_owned(), identity);
        let mut status = status.lock().unwrap();
        status.ready.insert(path.to_owned());
        Arc::make_mut(&mut status.files).insert(path.to_owned(), EntryState::Ready);
    }
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
    nearby: &[PathBuf],
    publication: &Mutex<()>,
    current: impl Fn() -> bool,
    published: impl Fn(),
) -> Result<()> {
    crate::background::checkpoint_or_cancel()?;
    if !current() {
        return Ok(());
    }
    if complete(root, path, settings) {
        published();
        return Ok(());
    }
    let identity = Identity::read(path)?;
    let Some((source, original)) =
        crate::x3f::read_cacheable(path, |family| settings.permits(family))?
    else {
        return Ok(());
    };
    source.validate()?;
    crate::background::checkpoint_or_cancel()?;
    {
        let _publication = publication_lock(publication)?;
        if current() && !ready(root, path, settings) {
            store_checked(
                root,
                path,
                identity.clone(),
                &source,
                settings.max_bytes,
                nearby,
                &current,
            )?;
        }
    }
    let _publication = publication_lock(publication)?;
    super::selection::prepare(
        root,
        path,
        &identity,
        &source.metadata,
        &original,
        settings,
        nearby,
        &current,
    )?;
    drop(source);
    published();
    Ok(())
}
fn complete(root: &Path, path: &Path, settings: &Settings) -> bool {
    Identity::read(path).is_ok_and(|identity| complete_identity(root, &identity, settings))
}
fn complete_identity(root: &Path, identity: &Identity, settings: &Settings) -> bool {
    let decoded = std::fs::File::open(root.join(identity.name()))
        .ok()
        .is_some_and(|mut file| read_header(&mut file, identity, settings).is_ok());
    decoded
        && fs::metadata(root.join(identity.name()).with_extension("original"))
            .is_ok_and(|m| m.is_file() && m.len() == identity.length)
        && super::selection::read(root, identity, settings).is_ok_and(|s| {
            s.thumbnail.is_none()
                || root
                    .join(identity.name())
                    .with_extension("preview")
                    .is_file()
        })
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
    let identity = match Identity::read(path) {
        Ok(identity) => identity,
        Err(_) => match families.get(path) {
            Some((identity, _)) if complete_identity(root, identity, settings) => identity.clone(),
            _ => return EntryState::Unavailable,
        },
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
        families.insert(path.to_owned(), (identity.clone(), family));
        family
    };
    let Some(family) = family else {
        return EntryState::Unsupported;
    };
    if !settings.permits(family) {
        return EntryState::Disabled;
    }
    if complete_identity(root, &identity, settings) {
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
    let (settings, watched, generation, queued, nearby, known) = {
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
            state.nearby.clone(),
            state.identities.clone(),
        )
    };
    let entries = prune(root, settings.max_bytes, &nearby).unwrap_or_default();
    let watching = watched.iter().cloned().collect::<HashSet<_>>();
    families.retain(|p, _| watching.contains(p));
    for (path, identity) in known {
        if watching.contains(&path) && !families.contains_key(&path) {
            let family = File::open(root.join(identity.name()))
                .ok()
                .and_then(|mut file| read_header(&mut file, &identity, &settings).ok())
                .and_then(|header| {
                    header
                        .properties
                        .get("CAMMODEL")
                        .and_then(|m| ProcessingFamily::from_camera_model(m))
                });
            families.insert(path, (identity, family));
        }
    }
    let files: HashMap<_, _> = watched
        .into_iter()
        .map(|path| {
            crate::background::checkpoint();
            let state = entry_state(root, &path, &settings, queued.contains(&path), families);
            if state == EntryState::Ready && Identity::read(&path).is_ok() {
                if let Some((identity, _)) = families.get(&path) {
                    let previous = control.lock().unwrap().recipes.get(&path).cloned();
                    if let Ok(recipe) = crate::sidecar::load(&path) {
                        let updated = super::selection::update_recipe(
                            root,
                            identity,
                            recipe,
                            &settings,
                            &nearby,
                            &|| {
                                let state = control.lock().unwrap();
                                !state.clear && state.generation == generation
                            },
                        );
                        if updated.is_ok() {
                            let mut state = control.lock().unwrap();
                            if state.generation == generation
                                && state.recipes.get(&path) == previous.as_ref()
                            {
                                state.recipes.remove(&path);
                            }
                        }
                    }
                }
            }
            (path, state)
        })
        .collect();
    let ready: HashSet<PathBuf> = files
        .iter()
        .filter(|(_, state)| **state == EntryState::Ready)
        .map(|(p, _)| p.clone())
        .collect();
    let identities: HashMap<_, _> = ready
        .iter()
        .filter_map(|p| {
            // Keep the identity whose bundle entry_state verified. Re-reading
            // the source here could attach a replacement file to an older Ready result.
            families.get(p).map(|(i, _)| (p.clone(), i.clone()))
        })
        .collect();
    let mut state = control.lock().unwrap();
    let mut status = status.lock().unwrap();
    status.entries = entries
        .iter()
        .filter(|e| e.0.extension().is_some_and(|s| s == "decode"))
        .count();
    status.bytes = entries.iter().map(|e| e.1).sum();
    if state.generation == generation && !state.clear {
        state.identities = identities;
        state.recipes.retain(|p, _| watching.contains(p));
        status.ready = ready;
        status.files = Arc::new(files);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn speculative_publication_wait_cancels_while_another_photo_owns_the_lock() {
        let publication = Mutex::new(());
        let _busy = publication.lock().unwrap();
        let gate = Arc::new(Gate::default());
        background::interruptible(gate.clone(), || {
            let _foreground = gate.foreground();
            assert!(publication_lock(&publication)
                .unwrap_err()
                .is::<background::Interrupted>());
        });
    }
}
