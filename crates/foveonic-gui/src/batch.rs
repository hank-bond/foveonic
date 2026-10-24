//! Runs frozen import jobs independently of review while yielding work to interactive rendering.

use crate::export::{self, Export, ExportFormat};
use eframe::egui;
use foveonic::{
    decode_cache::Cache,
    importing::{self, Disposition, History, Identity, Record},
    profile::Profile,
    render::{Engine, RenderSession, Settings},
    source::Result,
};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
};

#[derive(Clone)]
pub struct Photo {
    pub path: PathBuf,
    pub identity: Identity,
    pub settings: Settings,
}
/// Identity/history remains usable even when a photo's editing recipe is unreadable.
pub struct SourceInfo {
    pub identity: Identity,
    pub settings: std::result::Result<Settings, String>,
}
pub struct Scan {
    pub generation: u64,
    pub destination: Option<PathBuf>,
    pub files: Vec<PathBuf>,
}
pub struct Scanned {
    pub generation: u64,
    pub sources: HashMap<PathBuf, SourceInfo>,
    // This worker serializes scans with imports. Earlier completions are already in history.
    pub completed_imports: usize,
    pub dispositions: HashMap<PathBuf, Disposition>,
    pub errors: Vec<String>,
}
pub struct Batch {
    pub destination: PathBuf,
    pub photos: Vec<Photo>,
    pub decisions: Vec<Decision>,
    pub formats: Vec<ExportFormat>,
    pub tiff_color_space: Option<foveonic::render::TiffColorSpace>,
}
pub struct Decision {
    pub path: PathBuf,
    pub identity: Identity,
    pub hidden: bool,
}
#[derive(Clone, Default)]
pub struct Progress {
    pub destination: Option<PathBuf>,
    pub running: bool,
    pub total: usize,
    pub completed: usize,
    pub imported: Vec<PathBuf>,
    pub errors: Vec<String>,
}
#[derive(Default)]
struct Pending {
    scan: Option<Scan>,
    batch: Option<Batch>,
    stop: bool,
}
pub struct Worker {
    pending: Arc<(Mutex<Pending>, Condvar)>,
    progress: Arc<Mutex<Progress>>,
    scans: Arc<Mutex<Option<Scanned>>>,
}
impl Worker {
    pub fn new(engine: Engine, cache: Arc<Cache>, context: egui::Context) -> Self {
        let pending = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let progress = Arc::new(Mutex::new(Progress::default()));
        let slot = pending.clone();
        let status = progress.clone();
        let scans = Arc::new(Mutex::new(None));
        let scanned = scans.clone();
        std::thread::spawn(move || loop {
            let (scan, batch) = {
                let mut state = slot.0.lock().unwrap();
                while !state.stop && state.scan.is_none() && state.batch.is_none() {
                    state = slot.1.wait(state).unwrap();
                }
                if state.stop {
                    break;
                }
                let batch = state.batch.take();
                let scan = if batch.is_none() {
                    state.scan.take()
                } else {
                    None
                };
                (scan, batch)
            };
            if let Some(scan) = scan {
                let completed = status.lock().unwrap().imported.len();
                let result = cache
                    .background(|| scan_files(scan, completed, &cache, || cache.background(|| ())));
                // A stale scan can be discarded. Batch completion never waits for UI receipt.
                *scanned.lock().unwrap() = Some(result);
            }
            if let Some(batch) = batch {
                cache.background(|| {
                    run_batch(&engine, &cache, batch, &status, &context, || {
                        let (stop, scan) = {
                            let mut state = slot.0.lock().unwrap();
                            (state.stop, state.scan.take())
                        };
                        if let Some(scan) = scan {
                            let completed = status.lock().unwrap().imported.len();
                            *scanned.lock().unwrap() =
                                Some(scan_files(scan, completed, &cache, || {
                                    cache.background(|| ())
                                }));
                            context.request_repaint();
                        }
                        stop
                    })
                });
                status.lock().unwrap().running = false;
            }
            context.request_repaint();
        });
        Self {
            pending,
            progress,
            scans,
        }
    }
    pub fn scan(&self, scan: Scan) {
        self.pending.0.lock().unwrap().scan = Some(scan);
        self.pending.1.notify_one();
    }
    pub fn receive_scan(&self) -> Option<Scanned> {
        self.scans.lock().unwrap().take()
    }
    pub fn submit(&self, batch: Batch) -> bool {
        let mut progress = self.progress.lock().unwrap();
        if progress.running {
            return false;
        }
        *progress = Progress {
            destination: Some(batch.destination.clone()),
            running: true,
            total: batch.photos.len(),
            ..Default::default()
        };
        self.pending.0.lock().unwrap().batch = Some(batch);
        self.pending.1.notify_one();
        true
    }
    pub fn progress(&self) -> Progress {
        self.progress.lock().unwrap().clone()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let mut state = self.pending.0.lock().unwrap();
        state.stop = true;
        state.scan = None;
        state.batch = None;
        self.pending.1.notify_one();
    }
}

fn scan_files(
    scan: Scan,
    completed_imports: usize,
    cache: &Cache,
    checkpoint: impl Fn(),
) -> Scanned {
    let mut result = Scanned {
        generation: scan.generation,
        sources: HashMap::new(),
        completed_imports,
        dispositions: HashMap::new(),
        errors: Vec::new(),
    };
    let history = scan
        .destination
        .as_ref()
        .map(|destination| History::load(destination))
        .transpose();
    let history = match history {
        Ok(history) => history,
        Err(error) => {
            result.errors.push(error.to_string());
            return result;
        }
    };
    for path in scan.files {
        checkpoint();
        let identity = match cache
            .import_identity(&path)
            .map(Ok)
            .unwrap_or_else(|| Identity::read(&path))
        {
            Ok(identity) => identity,
            Err(error) => {
                result.errors.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        if let Some((destination, history)) = scan.destination.as_ref().zip(history.as_ref()) {
            if let Some(disposition) = history.disposition(destination, &identity) {
                result
                    .dispositions
                    .insert(path.clone(), disposition.clone());
            }
        }
        let settings = (|| -> Result<Settings> {
            if let Some(selection) = cache.selection(&path) {
                selection
                    .settings
                    .map(Ok)
                    .unwrap_or_else(|| crate::sidecars::capture_defaults(selection.properties))
            } else {
                foveonic::sidecar::load(&path)?.map(Ok).unwrap_or_else(|| {
                    crate::sidecars::capture_defaults(cache.capture_properties(&path)?)
                })
            }
        })()
        .map_err(|error| error.to_string());
        if let Err(error) = &settings {
            result.errors.push(format!("{}: {error}", path.display()));
        }
        result
            .sources
            .insert(path, SourceInfo { identity, settings });
    }
    result
}

fn apply_decisions(history: &mut History, destination: &std::path::Path, decisions: Vec<Decision>) {
    for decision in decisions {
        if decision.hidden {
            // Already imported photos are concealable without discarding
            // the durable link to their copied original.
            if !matches!(
                history.disposition(destination, &decision.identity),
                Some(Disposition::Imported { .. })
            ) {
                history.set(Record {
                    identity: decision.identity,
                    source: decision.path.to_string_lossy().into_owned(),
                    disposition: Disposition::Hidden,
                });
            }
        } else if matches!(
            history.disposition(destination, &decision.identity),
            Some(Disposition::Hidden)
        ) {
            history.forget(&decision.identity);
        }
    }
}

fn run_batch(
    engine: &Engine,
    cache: &Cache,
    batch: Batch,
    progress: &Mutex<Progress>,
    context: &egui::Context,
    stop: impl Fn() -> bool,
) {
    let result = (|| -> Result<()> {
        fs::create_dir_all(&batch.destination)?;
        let mut history = History::load(&batch.destination)?;
        apply_decisions(&mut history, &batch.destination, batch.decisions);
        history.save(&batch.destination)?;
        for photo in batch.photos {
            if stop() {
                break;
            }
            let result = import_photo(
                engine,
                cache,
                &photo,
                &batch.destination,
                &batch.formats,
                batch.tiff_color_space,
                |original| {
                    // Publish history only after outputs finish. Keep the old
                    // in-memory history if publication fails and outputs roll back.
                    let mut updated = history.clone();
                    updated.set(Record {
                        identity: photo.identity.clone(),
                        source: photo.path.to_string_lossy().into_owned(),
                        disposition: Disposition::Imported {
                            original: original.to_owned(),
                        },
                    });
                    updated.save(&batch.destination)?;
                    history = updated;
                    Ok(())
                },
            );
            let mut status = progress.lock().unwrap();
            status.completed += 1;
            match result {
                Ok(_) => status.imported.push(photo.path),
                Err(error) => status
                    .errors
                    .push(format!("{}: {error}", photo.path.display())),
            }
            drop(status);
            context.request_repaint();
        }
        Ok(())
    })();
    if let Err(error) = result {
        progress.lock().unwrap().errors.push(error.to_string());
    }
}

fn import_photo(
    engine: &Engine,
    cache: &Cache,
    photo: &Photo,
    destination: &std::path::Path,
    formats: &[ExportFormat],
    tiff_color_space: Option<foveonic::render::TiffColorSpace>,
    finish: impl FnOnce(&std::path::Path) -> Result<()>,
) -> Result<PathBuf> {
    let originals = if formats.is_empty() {
        destination.to_owned()
    } else {
        destination.join("x3f")
    };
    fs::create_dir_all(&originals)?;
    let extensions: Vec<_> = formats.iter().map(|format| format.extension()).collect();
    let name = importing::available_name(&photo.path, &originals, destination, &extensions)?;
    let original = originals.join(&name);
    let mut created = Vec::new();
    let result = (|| -> Result<PathBuf> {
        if !cache.copy_original(&photo.path, &original, &photo.identity)? {
            importing::copy_original(&photo.path, &original, &photo.identity)?;
        }
        created.push(original.clone());
        foveonic::sidecar::save(&original, &photo.settings)?;
        created.push(foveonic::sidecar::path(&original));
        if !formats.is_empty() {
            // Reuse the source's decode cache. No second camera-card read after copying
            // when prefetch has prepared the source. Cache transfer is deferred.
            let cached = if cache
                .import_identity(&photo.path)
                .or_else(|| Identity::read(&photo.path).ok())
                .as_ref()
                == Some(&photo.identity)
            {
                cache.open_cached(&photo.path, |_| {})
            } else {
                None
            };
            let source = match cached {
                Some(source) => source,
                None => cache.open(&original, |_| {})?,
            };
            let mut session = RenderSession::new(source);
            for &format in formats {
                let output = destination.join(&name).with_extension(format.extension());
                export::write(
                    engine,
                    &mut session,
                    &photo.settings,
                    &Export {
                        path: output.clone(),
                        format,
                        tiff_color_space,
                    },
                    &mut Profile::default(),
                    |_| {},
                )?;
                created.push(output);
            }
        }
        let relative = original.strip_prefix(destination)?.to_owned();
        finish(&relative)?;
        Ok(relative)
    })();
    if result.is_err() {
        // Runtime failures leave this photo picked for retry. Other successful
        // photos remain imported. Existing destination files are never removed.
        for path in created.into_iter().rev() {
            let _ = fs::remove_file(path);
        }
    }
    result
}

#[cfg(test)]
#[path = "batch_test.rs"]
mod tests;
