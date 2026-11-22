//! Runs frozen processing batches, source scans and destination probes off-thread while yielding to interactive rendering; import-history matching never waits for rendering.

use crate::export::{self, Export, ExportFormat};
use eframe::egui;
use foveonic::diagnostics as trace;
use foveonic::priority::Priority;
use foveonic::support::{self, Level, OperationContext};
use foveonic::{
    decode_cache::Cache,
    importing::{self, ConflictPolicy, Disposition, History, Identity, Record},
    profile::Profile,
    render::{Engine, NativePhoto, Settings},
    source::Result,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    time::Instant,
};

#[derive(Clone)]
pub struct Photo {
    pub path: PathBuf,
    pub identity: Identity,
    pub settings: Settings,
    pub rating: foveonic::review::Rating,
}
/// Identity/history remains usable even when a photo's editing recipe is unreadable.
pub struct SourceInfo {
    pub identity: Identity,
    pub settings: std::result::Result<Settings, String>,
    pub rating: foveonic::review::Rating,
}
pub struct Scan {
    pub defaults: foveonic::render::RecipeDefaults,
    pub generation: u64,
    pub destination: Option<PathBuf>,
    pub files: Vec<PathBuf>,
}
/// Import history for a scan, published before its settings so handled files hide at once.
pub struct Recorded {
    pub generation: u64,
    // Counted before reading history, and imports save history before counting,
    // so these completions are already in it.
    pub completed_imports: usize,
    pub dispositions: HashMap<PathBuf, Disposition>,
    pub errors: Vec<String>,
}
pub struct Scanned {
    pub generation: u64,
    pub sources: HashMap<PathBuf, SourceInfo>,
    pub errors: Vec<String>,
}
struct Lookup {
    generation: u64,
    destination: PathBuf,
    files: Vec<PathBuf>,
}
#[derive(Default)]
struct Lookups {
    next: Option<Lookup>,
    stop: bool,
}
pub struct Batch {
    pub calibration: Option<Arc<foveonic::noise_calibration::Calibration>>,
    pub destination: PathBuf,
    pub photos: Vec<Photo>,
    pub operation: Operation,
    pub output: Output,
}
pub struct Output {
    pub formats: Vec<ExportFormat>,
    pub encoding: foveonic::output::EncodingOptions,
    pub tiff_color_space: Option<foveonic::render::TiffColorSpace>,
    pub conflict: ConflictPolicy,
    pub sections: foveonic::render::ProcessingSections,
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
    pub decisions_saved: bool,
    pub errors: Vec<String>,
    pub operations: Vec<String>,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Summary {
    pub running: bool,
    pub total: usize,
    pub completed: usize,
}
impl Progress {
    fn summary(&self) -> Summary {
        Summary {
            running: self.running,
            total: self.total,
            completed: self.completed,
        }
    }
}
#[derive(Default)]
struct Pending {
    scan: Option<Scan>,
    batch: Option<Batch>,
    directory: Option<PathBuf>,
    stop: bool,
}
pub struct Worker {
    pending: Arc<(Mutex<Pending>, Condvar)>,
    progress: Arc<Mutex<Progress>>,
    lookups: Arc<(Mutex<Lookups>, Condvar)>,
    recorded: Arc<Mutex<Option<Recorded>>>,
    scans: Arc<Mutex<Option<Scanned>>>,
    directory: Arc<Mutex<Option<(PathBuf, bool)>>>,
}
impl Worker {
    pub fn new(engine: Engine, cache: Arc<Cache>, context: egui::Context) -> Self {
        let pending = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let progress = Arc::new(Mutex::new(Progress::default()));
        let slot = pending.clone();
        let status = progress.clone();
        let scans = Arc::new(Mutex::new(None));
        let scanned = scans.clone();
        let lookups = Arc::new((Mutex::new(Lookups::default()), Condvar::new()));
        let recorded = Arc::new(Mutex::new(None));
        start_history_lookups(
            lookups.clone(),
            recorded.clone(),
            progress.clone(),
            cache.clone(),
            context.clone(),
        );
        let directory = Arc::new(Mutex::new(None));
        let checked_directory = directory.clone();
        let mut engine = engine;
        let builder = std::thread::Builder::new().name("batch".into());
        let spawned = builder.spawn(move || {
            let mut unavailable_directory = None;
            loop {
                let (scan, batch, directory) = {
                    let mut state = slot.0.lock().unwrap();
                    while !state.stop
                        && state.scan.is_none()
                        && state.batch.is_none()
                        && state.directory.is_none()
                    {
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
                    let directory = if batch.is_none() && scan.is_none() {
                        state.directory.take()
                    } else {
                        None
                    };
                    (scan, batch, directory)
                };
                if let Some(path) = directory {
                    let _context = trace::enabled().then(|| {
                        trace::enter(trace::Context {
                            task: 0,
                            revision: 0,
                            photo: 0,
                            purpose: "destination_probe",
                            node: None,
                        })
                    });
                    let _probe = trace::scope_with(
                        trace::Category::Io,
                        "batch.destination_probe",
                        |fields| {
                            fields.str("path", &path.to_string_lossy());
                        },
                    );
                    let available = match fs::metadata(&path) {
                        Ok(metadata) => {
                            if metadata.is_dir() {
                                unavailable_directory = None;
                                true
                            } else {
                                if unavailable_directory.as_ref() != Some(&(path.clone(), None)) {
                                    support::event(
                                        Level::Warning,
                                        "import.destination_unavailable",
                                        "Destination is not a folder",
                                        serde_json::json!({"destination":path.to_string_lossy()}),
                                    );
                                }
                                unavailable_directory = Some((path.clone(), None));
                                false
                            }
                        }
                        Err(error) => {
                            let failure = (path.clone(), Some(error.kind()));
                            if unavailable_directory.as_ref() != Some(&failure) {
                                if error.kind() == std::io::ErrorKind::NotFound {
                                    support::event(
                                        Level::Warning,
                                        "import.destination_unavailable",
                                        "Destination folder does not exist",
                                        serde_json::json!({"destination":path.to_string_lossy()}),
                                    );
                                } else {
                                    support::report_error(
                                        "import.destination_probe",
                                        &error,
                                        OperationContext {
                                            component: Some("import".into()),
                                            phase: Some("inspect-destination".into()),
                                            destination: Some(path.to_string_lossy().into_owned()),
                                            ..Default::default()
                                        },
                                    );
                                }
                            }
                            unavailable_directory = Some(failure);
                            false
                        }
                    };
                    *checked_directory.lock().unwrap() = Some((path, available));
                }
                if let Some(scan) = scan {
                    let result = cache.background(|| {
                        read_settings(&scan, &cache, || {
                            cache.background(|| ());
                            slot.0.lock().unwrap().scan.is_some()
                        })
                    });
                    // A stale scan can be discarded. Batch completion never waits for UI receipt.
                    if result.is_some() {
                        *scanned.lock().unwrap() = result;
                    }
                }
                if let Some(batch) = batch {
                    engine.set_noise_calibration(batch.calibration.clone());
                    let _demand = cache.priority().demand(Priority::Export);
                    cache.priority().run(Priority::Export, || {
                        run_batch(&engine, &cache, batch, &status, &context, || {
                            let (stop, scan) = {
                                let mut state = slot.0.lock().unwrap();
                                (state.stop, state.scan.take())
                            };
                            if let Some(scan) = scan {
                                let result = read_settings(&scan, &cache, || {
                                    cache.priority().run(Priority::Export, || ());
                                    slot.0.lock().unwrap().scan.is_some()
                                });
                                if result.is_some() {
                                    *scanned.lock().unwrap() = result;
                                }
                                context.request_repaint();
                            }
                            stop
                        })
                    });
                    engine.clear_preparation();
                    engine.set_noise_calibration(None);
                    status.lock().unwrap().running = false;
                }
                context.request_repaint();
            }
        });
        spawned
            .inspect_err(|error| {
                support::report_error(
                    "batch.spawn",
                    error,
                    OperationContext {
                        component: Some("batch".into()),
                        phase: Some("spawn-worker".into()),
                        ..Default::default()
                    },
                );
            })
            .expect("start batch thread");
        Self {
            pending,
            progress,
            lookups,
            recorded,
            scans,
            directory,
        }
    }
    pub fn scan(&self, scan: Scan) {
        if let Some(destination) = &scan.destination {
            self.lookups.0.lock().unwrap().next = Some(Lookup {
                generation: scan.generation,
                destination: destination.clone(),
                files: scan.files.clone(),
            });
            self.lookups.1.notify_one();
        }
        self.pending.0.lock().unwrap().scan = Some(scan);
        self.pending.1.notify_one();
    }
    pub fn receive_history(&self) -> Option<Recorded> {
        self.recorded.lock().unwrap().take()
    }
    pub fn receive_scan(&self) -> Option<Scanned> {
        self.scans.lock().unwrap().take()
    }
    pub fn check_directory(&self, path: PathBuf) {
        self.pending.0.lock().unwrap().directory = Some(path);
        self.pending.1.notify_one();
    }
    pub fn receive_directory(&self) -> Option<(PathBuf, bool)> {
        self.directory.lock().unwrap().take()
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
    pub fn summary(&self) -> Summary {
        self.progress.lock().unwrap().summary()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let mut state = self.pending.0.lock().unwrap();
        state.stop = true;
        state.scan = None;
        state.batch = None;
        state.directory = None;
        self.pending.1.notify_one();
        self.lookups.0.lock().unwrap().stop = true;
        self.lookups.1.notify_one();
    }
}

/// History matching gets its own thread so it never queues behind a settings pass
/// paused by rendering.
fn start_history_lookups(
    lookups: Arc<(Mutex<Lookups>, Condvar)>,
    recorded: Arc<Mutex<Option<Recorded>>>,
    progress: Arc<Mutex<Progress>>,
    cache: Arc<Cache>,
    context: egui::Context,
) {
    std::thread::Builder::new()
        .name("import-history".into())
        .spawn(move || loop {
            let lookup = {
                let mut state = lookups.0.lock().unwrap();
                while !state.stop && state.next.is_none() {
                    state = lookups.1.wait(state).unwrap();
                }
                if state.stop {
                    break;
                }
                state.next.take().unwrap()
            };
            let completed = progress.lock().unwrap().imported.len();
            *recorded.lock().unwrap() = Some(read_history(&lookup, completed, &cache));
            context.request_repaint();
        })
        .expect("start import history thread");
}

/// Matching uses filesystem metadata only, so it never waits for rendering.
fn read_history(lookup: &Lookup, completed_imports: usize, cache: &Cache) -> Recorded {
    let _context = trace::enabled().then(|| {
        trace::enter(trace::Context {
            task: 0,
            revision: lookup.generation,
            photo: 0,
            purpose: "import_scan",
            node: None,
        })
    });
    let _scan = trace::scope_with(trace::Category::Io, "batch.scan_history", |fields| {
        fields
            .u64("generation", lookup.generation)
            .u64("files", lookup.files.len() as u64)
            .str("destination", &lookup.destination.to_string_lossy());
    });
    let mut recorded = Recorded {
        generation: lookup.generation,
        completed_imports,
        dispositions: HashMap::new(),
        errors: Vec::new(),
    };
    let history = match History::load(&lookup.destination) {
        Ok(history) => history,
        Err(error) => {
            support::report_error(
                "import.scan_history",
                error.as_ref(),
                OperationContext {
                    component: Some("import-scan".into()),
                    phase: Some("load-history".into()),
                    destination: Some(
                        History::path(&lookup.destination)
                            .to_string_lossy()
                            .into_owned(),
                    ),
                    revision: Some(lookup.generation.to_string()),
                    ..Default::default()
                },
            );
            recorded.errors.push(error.to_string());
            return recorded;
        }
    };
    for path in &lookup.files {
        // The settings pass reports unreadable files.
        let Ok(identity) = cache
            .import_identity(path)
            .map(Ok)
            .unwrap_or_else(|| Identity::read(path))
        else {
            continue;
        };
        if let Some(disposition) = history.disposition(&lookup.destination, &identity) {
            recorded
                .dispositions
                .insert(path.clone(), disposition.clone());
        }
    }
    recorded
}

/// Settings read the card and may decode, so this pass yields at each photo and
/// returns `None` once a newer scan is waiting.
fn read_settings(scan: &Scan, cache: &Cache, superseded: impl Fn() -> bool) -> Option<Scanned> {
    let _context = trace::enabled().then(|| {
        trace::enter(trace::Context {
            task: 0,
            revision: scan.generation,
            photo: 0,
            purpose: "import_scan",
            node: None,
        })
    });
    let _scan = trace::scope_with(trace::Category::Cpu, "batch.scan", |fields| {
        fields
            .u64("generation", scan.generation)
            .u64("files", scan.files.len() as u64);
    });
    let mut result = Scanned {
        generation: scan.generation,
        sources: HashMap::new(),
        errors: Vec::new(),
    };
    let mut ratings = HashMap::new();
    for path in scan.files.iter().cloned() {
        let _context = trace::enabled().then(|| {
            trace::enter(crate::timing::context_for(
                0,
                scan.generation,
                &path,
                "import_scan",
            ))
        });
        let _photo = trace::scope_with(trace::Category::Cpu, "batch.scan_photo", |fields| {
            fields.str("source", &path.to_string_lossy());
        });
        if superseded() {
            return None;
        }
        let identity = match cache
            .import_identity(&path)
            .map(Ok)
            .unwrap_or_else(|| Identity::read(&path))
        {
            Ok(identity) => identity,
            Err(error) => {
                scan_failure(
                    "read-source-identity",
                    &path,
                    scan.generation,
                    error.as_ref(),
                );
                result.errors.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        let settings = (|| -> Result<Settings> {
            if let Some(selection) = cache.selection(&path) {
                selection
                    .settings
                    .map(Ok)
                    .unwrap_or_else(|| scan.defaults.from_properties(&selection.properties))
            } else {
                let saved = foveonic::sidecar::load(&path)?;
                if let foveonic::sidecar::Saved::Malformed(error) = &saved {
                    scan_failure(
                        "load-editing-settings-using-defaults",
                        &path,
                        scan.generation,
                        error.as_ref(),
                    );
                    result.errors.push(format!("{}: {error}", path.display()));
                }
                saved.settings().map(Ok).unwrap_or_else(|| {
                    scan.defaults
                        .from_properties(&cache.metadata_summary(&path)?.properties)
                })
            }
        })()
        .map_err(|error| {
            scan_failure(
                "load-editing-settings",
                &path,
                scan.generation,
                error.as_ref(),
            );
            error.to_string()
        });
        if let Err(error) = &settings {
            result.errors.push(format!("{}: {error}", path.display()));
        }
        let folder = path.parent().unwrap_or(std::path::Path::new(""));
        let stored =
            ratings.entry(folder.to_owned()).or_insert_with(|| {
                match foveonic::review::load(folder) {
                    Ok(ratings) => ratings,
                    Err(error) => {
                        scan_failure(
                            "load-ratings-using-defaults",
                            &foveonic::review::path(folder),
                            scan.generation,
                            error.as_ref(),
                        );
                        result.errors.push(format!(
                            "{}: {error}",
                            foveonic::review::path(folder).display()
                        ));
                        Default::default()
                    }
                }
            });
        let rating = stored
            .get(std::path::Path::new(path.file_name().unwrap_or_default()))
            .copied()
            .unwrap_or_default();
        result.sources.insert(
            path,
            SourceInfo {
                identity,
                settings,
                rating,
            },
        );
    }
    Some(result)
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
    let _batch = trace::scope_with(trace::Category::Cpu, "batch.run", |fields| {
        fields
            .u64("photos", batch.photos.len() as u64)
            .str("destination", &batch.destination.to_string_lossy());
    });
    let setup_context = OperationContext {
        component: Some("batch".into()),
        phase: Some("prepare-batch".into()),
        destination: Some(batch.destination.to_string_lossy().into_owned()),
        settings: Some(
            serde_json::json!({"photos":batch.photos.len(),"formats":batch.output.formats.iter().map(|f|f.extension()).collect::<Vec<_>>(),"conflict":batch.output.conflict}),
        ),
        ..Default::default()
    };
    let result = (|| -> Result<()> {
        fs::create_dir_all(&batch.destination)?;
        let mut history = match batch.operation {
            Operation::Browse => None,
            Operation::Import { decisions } => {
                let mut history = History::load(&batch.destination)?;
                apply_decisions(&mut history, &batch.destination, decisions);
                history.save(&batch.destination)?;
                progress.lock().unwrap().decisions_saved = true;
                Some(history)
            }
        };
        let mut reserved: HashSet<_> = batch
            .photos
            .iter()
            .map(|photo| photo.path.clone())
            .collect();
        for photo in &batch.photos {
            let purpose = if history.is_some() {
                "batch_import"
            } else {
                "batch_export"
            };
            let _context = trace::enabled()
                .then(|| trace::enter(crate::timing::context_for(0, 0, &photo.path, purpose)));
            let _photo = trace::scope_with(trace::Category::Cpu, "batch.photo", |fields| {
                fields
                    .str("source", &photo.path.to_string_lossy())
                    .str("operation", purpose)
                    .str(
                        "settings",
                        &serde_json::to_string(&photo.settings).unwrap_or_default(),
                    );
            });
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
                .map(|original| {
                    let mut status = progress.lock().unwrap();
                    status.operations.push(format!(
                        "Imported: {}",
                        batch.destination.join(&original).display()
                    ));
                    for format in &batch.output.formats {
                        let path = batch
                            .destination
                            .join(original.file_name().unwrap())
                            .with_extension(format.extension());
                        status.operations.push(format!(
                            "Saved {}: {}",
                            format.label(),
                            path.display()
                        ));
                    }
                })
            } else {
                export_photo(
                    engine,
                    cache,
                    photo,
                    &batch.destination,
                    &batch.output,
                    &mut reserved,
                    |path| {
                        progress
                            .lock()
                            .unwrap()
                            .operations
                            .push(format!("Saved: {}", path.display()))
                    },
                )
            };
            if let Some(started) = started {
                crate::timing::emit("batch_photo", || {
                    serde_json::json!({
                        "source":photo.path,"destination":batch.destination,
                        "operation":if history.is_some(){"import"}else{"browse"},
                        "completed_ms":started.elapsed().as_secs_f64()*1000.,
                        "status":if result.is_ok(){"complete"}else{"error"},
                        "error":result.as_ref().err().map(ToString::to_string),
                    })
                });
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
        support::report_error("batch.prepare", error.as_ref(), setup_context);
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
    record: impl Fn(&std::path::Path),
) -> Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let operation_id = format!("export_photo-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let mut phase = "open-source";
    let mut published = Vec::new();
    support::event(
        Level::Info,
        "operation.started",
        "Photo operation started",
        serde_json::json!({"operation":photo_context(&operation_id, photo, destination, output, phase, &published)}),
    );
    let result = (|| -> Result<()> {
        // Exports consume cache hits without filling the browsing cache on a miss.
        // Writing a whole decode/preview bundle here competes with the requested output.
        let source = match cache.open_cached(&photo.path, |_| {}) {
            Some(source) => source,
            None => Arc::new(foveonic::source::Source::open(&photo.path)?),
        };
        source_loaded(&photo.path, &source, None);
        let mut session = NativePhoto::new(source);
        for &format in &output.formats {
            phase = "choose-output-path";
            let path =
                export::next_path(&photo.path, destination, format, output.conflict, reserved)?;
            reserved.insert(path.clone());
            phase = "write-output";
            write_export(
                engine,
                &mut session,
                photo,
                &Export {
                    path: path.clone(),
                    format,
                    tiff_color_space: output.tiff_color_space,
                    encoding: output.encoding,
                    sections: output.sections,
                    conflict: output.conflict,
                },
            )?;
            published.push(path.clone());
            record(&path);
        }
        Ok(())
    })();
    let operation = photo_context(&operation_id, photo, destination, output, phase, &published);
    match result {
        Ok(value) => {
            support::event(
                Level::Info,
                "operation.completed",
                "Photo operation completed",
                serde_json::json!({"operation":operation}),
            );
            Ok(value)
        }
        Err(error) => Err(support::reported_error("batch.photo", error, operation)),
    }
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
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let operation_id = format!("import_photo-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let mut phase = "create-original-directory";
    let mut published = Vec::new();
    support::event(
        Level::Info,
        "operation.started",
        "Photo operation started",
        serde_json::json!({"operation":photo_context(&operation_id, photo, destination, output, phase, &published)}),
    );
    let result = (|| -> Result<PathBuf> {
        let formats = &output.formats;
        let originals = if formats.is_empty() {
            destination.to_owned()
        } else {
            destination.join("x3f")
        };
        fs::create_dir_all(&originals)?;
        let extensions: Vec<_> = formats.iter().map(|format| format.extension()).collect();
        phase = "choose-import-paths";
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

        phase = "copy-original";
        if !cache.copy_original(&photo.path, &original, &photo.identity, output.conflict)? {
            importing::copy_original(&photo.path, &original, &photo.identity, output.conflict)?;
        }
        published.push(original.clone());
        published_artifact(&operation_id, &photo.path, &original, "original");
        phase = "write-sidecar";
        let recipe = foveonic::sidecar::encode(&photo.settings)?;
        fs::create_dir_all(sidecar.parent().ok_or("sidecar has no parent")?)?;
        importing::write_output(&sidecar, output.conflict, |temporary| {
            fs::write(temporary, &recipe)?;
            Ok(())
        })?;
        published.push(sidecar.clone());
        published_artifact(&operation_id, &photo.path, &sidecar, "sidecar");
        phase = "save-rating";
        foveonic::review::save(&original, photo.rating)?;
        let ratings = foveonic::review::path(&originals);
        support::event(
            Level::Info,
            "import.rating_saved",
            "Imported photo rating saved",
            serde_json::json!({"operation_id":operation_id,"source":photo.path.to_string_lossy(),
                "rating_file":ratings.to_string_lossy(),"rating":photo.rating}),
        );
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
            phase = "open-imported-source";
            let source = match cached {
                Some(source) => source,
                None => Arc::new(foveonic::source::Source::open(&original)?),
            };
            source_loaded(&photo.path, &source, Some(&original));
            let mut session = NativePhoto::new(source);
            for (&format, path) in formats.iter().zip(rendered) {
                phase = "write-output";
                write_export(
                    engine,
                    &mut session,
                    photo,
                    &Export {
                        path: path.clone(),
                        format,
                        tiff_color_space: output.tiff_color_space,
                        encoding: output.encoding,
                        sections: output.sections,
                        conflict: output.conflict,
                    },
                )?;
                published.push(path);
            }
        }
        let relative = original.strip_prefix(destination)?.to_owned();
        phase = "save-import-history";
        finish(&relative)?;
        published.push(History::path(destination));
        Ok(relative)
    })();
    let operation = photo_context(&operation_id, photo, destination, output, phase, &published);
    match result {
        Ok(value) => {
            support::event(
                Level::Info,
                "operation.completed",
                "Photo operation completed",
                serde_json::json!({"operation":operation}),
            );
            Ok(value)
        }
        Err(error) => Err(support::reported_error("batch.photo", error, operation)),
    }
}

fn photo_context(
    id: &str,
    photo: &Photo,
    destination: &std::path::Path,
    output: &Output,
    phase: &str,
    published: &[PathBuf],
) -> OperationContext {
    OperationContext {
        operation_id: Some(id.into()),
        component: Some("batch".into()),
        phase: Some(phase.into()),
        source: Some(photo.path.to_string_lossy().into_owned()),
        destination: Some(destination.to_string_lossy().into_owned()),
        settings: Some(
            serde_json::json!({"frozen_recipe":support::settings_snapshot(&photo.settings),"source_identity":photo.identity,"rating":photo.rating,
            "formats":output.formats.iter().map(|format|format.extension()).collect::<Vec<_>>(),
            "encoding":output.encoding,"conflict":output.conflict,"sections":output.sections,"tiff_color_space":output.tiff_color_space,
            "published":published.iter().map(|path|path.to_string_lossy()).collect::<Vec<_>>(),
            "history_committed":phase=="save-import-history" && published.last().is_some_and(|p|*p==History::path(destination))}),
        ),
        ..Default::default()
    }
}

fn published_artifact(id: &str, source: &std::path::Path, path: &std::path::Path, kind: &str) {
    support::event(
        Level::Info,
        "artifact.published",
        "Import artifact saved",
        serde_json::json!({"operation_id":id,"source":source.to_string_lossy(),"destination":path.to_string_lossy(),"kind":kind}),
    );
}

fn scan_failure(
    phase: &str,
    path: &std::path::Path,
    generation: u64,
    error: &(dyn std::error::Error + 'static),
) {
    support::report_error(
        "import.scan",
        error,
        OperationContext {
            component: Some("import-scan".into()),
            phase: Some(phase.into()),
            source: Some(path.to_string_lossy().into_owned()),
            revision: Some(generation.to_string()),
            ..Default::default()
        },
    );
}

fn source_loaded(
    path: &std::path::Path,
    source: &foveonic::source::Source,
    copied: Option<&std::path::Path>,
) {
    let cached = source
        .load_timings()
        .iter()
        .any(|t| t.stage == "decoded cache read");
    support::event(
        Level::Info,
        "batch.source_ready",
        "Batch source opened",
        serde_json::json!({
            "source":path.to_string_lossy(),
            "camera":source.metadata.properties.get("CAMMODEL"),
            "input":if cached {"decoded-cache"}else if copied.is_some(){"copied-original"}else{"original"},
            "read_from":if cached {None}else{Some(copied.unwrap_or(path).to_string_lossy())},
            "extent":[source.metadata.width,source.metadata.height],
        }),
    );
    if crate::timing::enabled() {
        crate::timing::emit("source_load", || {
            serde_json::json!({
                "frontend":"batch","source":path,"timings":source.load_timings(),
                "camera":source.metadata.properties.get("CAMMODEL"),
                "input":if cached {"decoded-cache"}else if copied.is_some(){"copied-original"}else{"original"},
                "read_from":if cached {None}else{Some(copied.unwrap_or(path))},
                "extent":[source.metadata.width,source.metadata.height],
            })
        });
    }
}

fn write_export(
    engine: &Engine,
    session: &mut NativePhoto,
    photo: &Photo,
    output: &Export,
) -> Result<()> {
    let mut profile = Profile::new(crate::timing::mode());
    let started = crate::timing::enabled().then(Instant::now);
    let result = export::write(
        engine,
        session,
        &photo.settings,
        &photo.path,
        output,
        &mut profile,
        |_| {},
    );
    if let Some(started) = started {
        crate::timing::emit("export", || {
            serde_json::json!({
                "frontend":"batch","source":photo.path,"destination":output.path,
                "format":output.format.extension(),"settings":photo.settings,
                "completed_ms":started.elapsed().as_secs_f64()*1000.,
                "status":if result.is_ok(){"complete"}else{"error"},
            })
        });
    }
    result
}

#[cfg(test)]
#[path = "batch_test.rs"]
mod tests;
