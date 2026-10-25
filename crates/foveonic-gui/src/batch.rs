//! Runs frozen Browse and Import batches while yielding work to interactive rendering.

use crate::export::{self, Export, ExportFormat};
use eframe::egui;
use foveonic::{
    decode_cache::Cache,
    importing::{self, ConflictPolicy, Disposition, History, Identity, Record},
    profile::Profile,
    render::{Engine, RenderSession, Settings},
    source::Result,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::Instant,
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
    pub operation: Operation,
    pub output: Output,
}
pub struct Output {
    pub formats: Vec<ExportFormat>,
    pub tiff_color_space: Option<foveonic::render::TiffColorSpace>,
    pub conflict: ConflictPolicy,
}
pub enum Operation {
    Browse,
    Import { decisions: Vec<Decision> },
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
                    .processing(|| scan_files(scan, completed, &cache, || cache.processing(|| ())));
                // A stale scan can be discarded. Batch completion never waits for UI receipt.
                *scanned.lock().unwrap() = Some(result);
            }
            if let Some(batch) = batch {
                cache.processing(|| {
                    run_batch(&engine, &cache, batch, &status, &context, || {
                        let (stop, scan) = {
                            let mut state = slot.0.lock().unwrap();
                            (state.stop, state.scan.take())
                        };
                        if let Some(scan) = scan {
                            let completed = status.lock().unwrap().imported.len();
                            *scanned.lock().unwrap() =
                                Some(scan_files(scan, completed, &cache, || {
                                    cache.processing(|| ())
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
        let mut history = match batch.operation {
            Operation::Browse => None,
            Operation::Import { decisions } => {
                let mut history = History::load(&batch.destination)?;
                apply_decisions(&mut history, &batch.destination, decisions);
                history.save(&batch.destination)?;
                Some(history)
            }
        };
        let mut reserved: HashSet<_> = batch
            .photos
            .iter()
            .map(|photo| photo.path.clone())
            .collect();
        for photo in &batch.photos {
            let started = crate::timing::enabled().then(Instant::now);
            if stop() {
                break;
            }
            let result = if let Some(history) = &mut history {
                import_photo(
                    engine,
                    cache,
                    photo,
                    &batch.destination,
                    &batch.output,
                    &mut reserved,
                    |original| {
                        // Record imports only after all requested files finish.
                        let mut updated = history.clone();
                        updated.set(Record {
                            identity: photo.identity.clone(),
                            source: photo.path.to_string_lossy().into_owned(),
                            disposition: Disposition::Imported {
                                original: original.to_owned(),
                            },
                        });
                        updated.save(&batch.destination)?;
                        *history = updated;
                        Ok(())
                    },
                )
                .map(|_| ())
            } else {
                export_photo(
                    engine,
                    cache,
                    photo,
                    &batch.destination,
                    &batch.output,
                    &mut reserved,
                )
            };
            if let Some(started) = started {
                crate::timing::emit(
                    "batch_photo",
                    serde_json::json!({
                        "source":photo.path,"destination":batch.destination,
                        "operation":if history.is_some(){"import"}else{"browse"},
                        "completed_ms":started.elapsed().as_secs_f64()*1000.,
                        "status":if result.is_ok(){"complete"}else{"error"},
                        "error":result.as_ref().err().map(ToString::to_string),
                    }),
                );
            }
            let mut status = progress.lock().unwrap();
            status.completed += 1;
            match result {
                Ok(()) if history.is_some() => status.imported.push(photo.path.clone()),
                Ok(()) => (),
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

fn export_photo(
    engine: &Engine,
    cache: &Cache,
    photo: &Photo,
    destination: &std::path::Path,
    output: &Output,
    reserved: &mut HashSet<PathBuf>,
) -> Result<()> {
    // Exports consume cache hits without filling the browsing cache on a miss.
    // Writing a whole decode/preview bundle here competes with the requested output.
    let source = match cache.open_cached(&photo.path, |_| {}) {
        Some(source) => source,
        None => Arc::new(foveonic::source::Source::open(&photo.path)?),
    };
    source_loaded(&photo.path, &source, None);
    let mut session = RenderSession::new(source);
    for &format in &output.formats {
        let path = export::next_path(&photo.path, destination, format, output.conflict, reserved)?;
        reserved.insert(path.clone());
        write_export(
            engine,
            &mut session,
            photo,
            &Export {
                path,
                format,
                tiff_color_space: output.tiff_color_space,
                conflict: output.conflict,
            },
        )?;
    }
    Ok(())
}

fn import_photo(
    engine: &Engine,
    cache: &Cache,
    photo: &Photo,
    destination: &std::path::Path,
    output: &Output,
    reserved: &mut HashSet<PathBuf>,
    finish: impl FnOnce(&std::path::Path) -> Result<()>,
) -> Result<PathBuf> {
    let formats = &output.formats;
    let originals = if formats.is_empty() {
        destination.to_owned()
    } else {
        destination.join("x3f")
    };
    fs::create_dir_all(&originals)?;
    let extensions: Vec<_> = formats.iter().map(|format| format.extension()).collect();
    let name = importing::available_name(
        &photo.path,
        &originals,
        destination,
        &extensions,
        output.conflict,
        reserved,
    )?;
    let original = originals.join(&name);
    let sidecar = foveonic::sidecar::path(&original);
    let rendered: Vec<_> = formats
        .iter()
        .map(|format| destination.join(&name).with_extension(format.extension()))
        .collect();
    reserved.insert(original.clone());
    reserved.insert(sidecar.clone());
    reserved.extend(rendered.iter().cloned());

    if !cache.copy_original(&photo.path, &original, &photo.identity, output.conflict)? {
        importing::copy_original(&photo.path, &original, &photo.identity, output.conflict)?;
    }
    let recipe = foveonic::sidecar::encode(&photo.settings)?;
    fs::create_dir_all(sidecar.parent().ok_or("sidecar has no parent")?)?;
    importing::write_output(&sidecar, output.conflict, |temporary| {
        fs::write(temporary, &recipe)?;
        Ok(())
    })?;
    if !formats.is_empty() {
        // Reuse the source cache after copying. A miss reads the completed local copy.
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
            None => Arc::new(foveonic::source::Source::open(&original)?),
        };
        source_loaded(&photo.path, &source, Some(&original));
        let mut session = RenderSession::new(source);
        for (&format, path) in formats.iter().zip(rendered) {
            write_export(
                engine,
                &mut session,
                photo,
                &Export {
                    path,
                    format,
                    tiff_color_space: output.tiff_color_space,
                    conflict: output.conflict,
                },
            )?;
        }
    }
    let relative = original.strip_prefix(destination)?.to_owned();
    finish(&relative)?;
    Ok(relative)
}

fn source_loaded(
    path: &std::path::Path,
    source: &foveonic::source::Source,
    copied: Option<&std::path::Path>,
) {
    if crate::timing::enabled() {
        let cached = source
            .load_timings()
            .iter()
            .any(|t| t.stage == "decoded cache read");
        crate::timing::emit(
            "source_load",
            serde_json::json!({
                "frontend":"batch","source":path,"timings":source.load_timings(),
                "camera":source.metadata.properties.get("CAMMODEL"),
                "input":if cached {"decoded-cache"}else if copied.is_some(){"copied-original"}else{"original"},
                "read_from":if cached {None}else{Some(copied.unwrap_or(path))},
                "extent":[source.metadata.width,source.metadata.height],
            }),
        );
    }
}

fn write_export(
    engine: &Engine,
    session: &mut RenderSession,
    photo: &Photo,
    output: &Export,
) -> Result<()> {
    let mut profile = Profile::new(crate::timing::mode());
    let started = crate::timing::enabled().then(Instant::now);
    let result = export::write(
        engine,
        session,
        &photo.settings,
        output,
        &mut profile,
        |_| {},
    );
    if let Some(started) = started {
        crate::timing::emit(
            "export",
            serde_json::json!({
                "frontend":"batch","source":photo.path,"destination":output.path,
                "format":output.format.extension(),"settings":photo.settings,"profile":profile,
                "completed_ms":started.elapsed().as_secs_f64()*1000.,
                "status":if result.is_ok(){"complete"}else{"error"},
            }),
        );
    }
    result
}

#[cfg(test)]
#[path = "batch_test.rs"]
mod tests;
