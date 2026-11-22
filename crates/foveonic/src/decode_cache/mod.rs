//! Reuses decoded sources, original bytes, selection metadata and previews in a bounded temporary disk LRU.

use crate::priority::{Gate, Priority};
mod lru;
mod window;
use lru::prune;
mod original;
mod previews;
mod samples;
mod selection;
pub use selection::Selection;
mod worker;
use crate::source::{
    FileIdentity as Identity, LoadStage, Metadata, ProcessingFamily, Result, SensorSamples, Source,
    SourceOrigin, SourceStorage,
};
pub use previews::MAX_SIDE as PREVIEW_MAX_SIDE;
use serde::{Deserialize, Serialize};
use std::{
    collections::{hash_map::DefaultHasher, BTreeMap, HashMap, HashSet, VecDeque},
    fs::{self, File, OpenOptions},
    hash::{Hash, Hasher},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Instant, SystemTime},
};

// Bump when decoder semantics or the stored source representation change.
const FORMAT_VERSION: u32 = 3;
const NAMESPACE: &str = "foveonic-decoded-v1";
pub const DEFAULT_BUDGET: u64 = 10_000_000_000;
const DEFAULT_PREFETCH_BEHIND: usize = 3;
const DEFAULT_PREFETCH_AHEAD: usize = 7;
const MAGIC: &[u8; 8] = b"FOVDEC02";
// Signature, JSON length and JSON checksum precede the versioned header.
const PREFIX_BYTES: u64 = (MAGIC.len() + size_of::<u64>() + size_of::<u32>()) as u64;
const MAX_HEADER: u64 = 1024 * 1024;
const MAX_CAMF: u64 = 64 * 1024 * 1024;
const SAMPLES_PER_CHUNK: usize = crate::priority::IO_CHUNK_BYTES / size_of::<u16>();
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Recency is best effort. A readable cache entry remains useful if it cannot be updated.
fn mark_used(path: &Path) {
    let _ = File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(SystemTime::now()));
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub max_bytes: u64,
    /// Browser neighbors to decode around the current photo, independently of CPU concurrency.
    pub prefetch_behind: usize,
    pub prefetch_ahead: usize,
    pub classic: bool,
    pub merrill: bool,
    pub quattro: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            max_bytes: DEFAULT_BUDGET,
            prefetch_behind: DEFAULT_PREFETCH_BEHIND,
            prefetch_ahead: DEFAULT_PREFETCH_AHEAD,
            classic: false,
            merrill: true,
            quattro: true,
        }
    }
}
impl Settings {
    pub fn permits(&self, family: ProcessingFamily) -> bool {
        self.enabled
            && self.max_bytes > 0
            && match family {
                ProcessingFamily::Classic => self.classic,
                ProcessingFamily::Merrill => self.merrill,
                ProcessingFamily::Quattro => self.quattro,
            }
    }
}

/// Directory readiness is reported independently of the active render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryState {
    Disabled,
    Unsupported,
    Unavailable,
    Queued,
    Ready,
    Uncached,
}

#[derive(Clone, Default, Debug)]
pub struct Status {
    pub bytes: u64,
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub writing: bool,
    pub prefetching: Vec<PathBuf>,
    pub ready: HashSet<PathBuf>,
    pub files: Arc<HashMap<PathBuf, EntryState>>,
}

impl Identity {
    fn name(&self) -> String {
        // The full identity is checked in the header. Hash collisions cause a
        // disposable miss/replacement and can never select another photograph.
        let mut hash = DefaultHasher::new();
        self.hash(&mut hash);
        format!("{:016x}.decode", hash.finish())
    }
}

#[derive(Serialize, Deserialize)]
struct Header {
    version: u32,
    identity: Identity,
    little_endian: bool,
    width: usize,
    height: usize,
    rotation: u32,
    processing_version: Option<[u16; 4]>,
    properties: BTreeMap<String, String>,
    packed_phases: bool,
    sample_encoding: samples::Encoding,
    samples: u64,
    camf_bytes: u64,
    checksum: u32,
}

struct Control {
    settings: Settings,
    photo_defaults: crate::render::RecipeDefaults,
    noise_calibration: Option<String>,
    clear: bool,
    watched: Vec<PathBuf>,
    selected: Option<PathBuf>,
    nearby: Vec<PathBuf>,
    queue: VecDeque<PathBuf>,
    stores: HashSet<PathBuf>,
    generation: u64,
    preview: Option<previews::PendingPreview>,
    identities: HashMap<PathBuf, Identity>,
    capture_metadata: HashMap<PathBuf, (Identity, BTreeMap<String, String>)>,
    rejected: HashSet<Identity>,
    recipes: HashMap<PathBuf, crate::render::Settings>,
    recipe_updates: BTreeMap<PathBuf, crate::render::Settings>,
}
impl Control {
    fn restart_prefetch(&mut self) {
        self.nearby = window::around(
            &self.watched,
            self.selected.as_deref(),
            self.settings.prefetch_behind,
            self.settings.prefetch_ahead,
        );
        self.queue = self.nearby.clone().into();
        self.generation += 1;
    }
}
enum Command {
    Maintain,
    Store {
        path: PathBuf,
        identity: Identity,
        source: Arc<Source>,
        original: Vec<u8>,
    },
}

/// Prefetch admits at most three photos independently of CPU worker count. Foreground insertion
/// allows one source write and one pending source. A full queue drops cache
/// insertion, never an image request. Sources are shared, not copied for writing.
pub struct Cache {
    root: PathBuf,
    control: Arc<Mutex<Control>>,
    status: Arc<Mutex<Status>>,
    writer: mpsc::SyncSender<Command>,
    gate: Arc<Gate>,
}
impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}
impl Cache {
    /// Start inactive. The application supplies its persistent policy with `configure`.
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(NAMESPACE).join(boot_key());
        let namespace = std::env::var_os("FOVEONIC_DIAGNOSTICS_CACHE_NAMESPACE");
        Self::at(
            diagnostic_root(root, namespace.as_deref())
                .expect("invalid diagnostic cache namespace"),
        )
    }
    fn at(root: PathBuf) -> Self {
        let control = Arc::new(Mutex::new(Control {
            photo_defaults: Default::default(),
            noise_calibration: None,
            settings: Settings {
                enabled: false,
                ..Settings::default()
            },
            clear: false,
            watched: Vec::new(),
            selected: None,
            nearby: Vec::new(),
            queue: VecDeque::new(),
            stores: HashSet::new(),
            generation: 0,
            preview: None,
            identities: HashMap::new(),
            capture_metadata: HashMap::new(),
            rejected: HashSet::new(),
            recipes: HashMap::new(),
            recipe_updates: BTreeMap::new(),
        }));
        let status = Arc::new(Mutex::new(Status::default()));
        let gate = Arc::new(Gate::default());
        let writer = worker::start(root.clone(), control.clone(), status.clone(), gate.clone());
        Self {
            root,
            control,
            status,
            writer,
            gate,
        }
    }
    pub fn configure(&self, settings: Settings) {
        let mut state = self.control.lock().unwrap();
        state.settings = settings;
        state.identities.clear();
        state.restart_prefetch();
        self.gate.interrupt_speculative();
        drop(state);
        let mut status = self.status.lock().unwrap();
        status.ready.clear();
        status.files = Arc::new(HashMap::new());
        drop(status);
        let _ = self.writer.try_send(Command::Maintain);
    }
    pub fn clear(&self) {
        let mut state = self.control.lock().unwrap();
        state.clear = true;
        state.identities.clear();
        state.recipes.clear();
        state.rejected.clear();
        state.recipe_updates.clear();
        state.preview = None;
        state.queue.clear();
        state.stores.clear();
        state.generation += 1;
        self.gate.interrupt_speculative();
        drop(state);
        let mut status = self.status.lock().unwrap();
        status.ready.clear();
        for state in Arc::make_mut(&mut status.files).values_mut() {
            if matches!(state, EntryState::Ready | EntryState::Queued) {
                *state = EntryState::Uncached;
            }
        }
        drop(status);
        let _ = self.writer.try_send(Command::Maintain);
    }

    /// Unedited previews follow the application recipe without changing saved edits or RAW identities.
    pub fn configure_photo_defaults(&self, defaults: &crate::render::RecipeDefaults) {
        self.control.lock().unwrap().photo_defaults = defaults.clone();
    }
    /// Preview samples are specific to the selected calibration. RAW bundles remain shared.
    pub fn configure_noise_calibration(&self, signature: Option<String>) {
        let mut control = self.control.lock().unwrap();
        control.noise_calibration = signature;
        control.preview = None;
    }

    /// Replace speculative work with a bounded neighborhood of the selected photo.
    /// Paths follow visible browser order and may include an active concealed photo.
    /// Repeating the same request must not restart a completed or cleared pass.
    pub fn prefetch_window(&self, paths: Vec<PathBuf>, selected: Option<PathBuf>) {
        let mut state = self.control.lock().unwrap();
        if state.watched == paths && state.selected == selected {
            return;
        }
        let order_changed = state.watched != paths;
        state.watched = paths;
        let watched: HashSet<_> = state.watched.iter().cloned().collect();
        state
            .capture_metadata
            .retain(|path, _| watched.contains(path));
        state.selected = selected;
        state.restart_prefetch();
        self.gate.interrupt_speculative();
        drop(state);
        if order_changed {
            let mut status = self.status.lock().unwrap();
            status.ready.clear();
            status.files = Arc::new(HashMap::new());
        }
        let _ = self.writer.try_send(Command::Maintain);
    }
    /// The app-wide priority gate shared by decoding, file I/O and rendering.
    pub fn priority(&self) -> &Arc<Gate> {
        &self.gate
    }
    /// Run optional work at Speculative priority on a dedicated thread with lowered OS priority.
    pub fn background<T>(&self, work: impl FnOnce() -> T) -> T {
        crate::priority::lower_priority();
        self.gate.run(Priority::Speculative, work)
    }
    /// Background-verified ready paths in the current bounded prefetch order.
    /// This snapshot never reads or stats an original file.
    pub fn preparation_window(&self) -> (u64, Vec<PathBuf>) {
        let control = self.control.lock().unwrap();
        let paths = if control.settings.enabled && control.settings.max_bytes > 0 && !control.clear
        {
            control
                .nearby
                .iter()
                .filter(|path| control.identities.contains_key(*path))
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        (control.generation, paths)
    }
    pub fn preparation_is_current(&self, generation: u64) -> bool {
        let control = self.control.lock().unwrap();
        control.generation == generation
            && control.settings.enabled
            && control.settings.max_bytes > 0
            && !control.clear
    }
    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
    pub fn directory(&self) -> &Path {
        &self.root
    }

    /// Size from the background-verified identity, without statting the source card.
    pub fn source_bytes(&self, path: &Path) -> Option<u64> {
        self.control
            .lock()
            .unwrap()
            .identities
            .get(path)
            .map(|identity| identity.length)
    }

    /// Reuse verified header metadata before loading the much larger pixel payload.
    /// Selection and editing defaults must not seek through the card again on a cache hit.
    pub fn capture_properties(&self, path: &Path) -> Result<BTreeMap<String, String>> {
        let identity = self
            .control
            .lock()
            .unwrap()
            .identities
            .get(path)
            .cloned()
            .or_else(|| Identity::read(path).ok());
        if let Some(identity) = identity.as_ref() {
            if let Some(cached) = self.memoized_capture(path, identity) {
                return Ok(cached);
            }
        }
        let settings = self.control.lock().unwrap().settings.clone();
        let header = || {
            let identity = identity.as_ref()?;
            let mut file = File::open(self.root.join(identity.name())).ok()?;
            read_header(&mut file, identity, &settings).ok()
        };
        let cached = self
            .selection(path)
            .map(|s| s.properties)
            .or_else(|| header().map(|header| header.properties))
            .map(|mut properties| {
                // Entries written before capture properties recorded the
                // orientation take it from the decoded entry, which still
                // holds it while the original is offline.
                if !properties.contains_key("X3F_ROTATION") {
                    if let Some(header) = header() {
                        properties.insert("X3F_ROTATION".into(), header.rotation.to_string());
                    }
                }
                properties
            });
        let properties = match cached {
            // Older entries lack the embedded EXIF check or the orientation.
            Some(properties)
                if properties.contains_key("EXIF_CAPTURE_CHECKED")
                    && properties.contains_key("X3F_ROTATION") =>
            {
                properties
            }
            Some(properties) => identity
                .as_ref()
                .and_then(|id| self.refresh_capture(path, id).ok())
                .unwrap_or(properties),
            None => match identity.as_ref() {
                Some(id) => self.refresh_capture(path, id)?,
                None => crate::source::read_capture_properties(path)?,
            },
        };
        if let Some(identity) = identity {
            self.remember_capture(path, identity, &properties);
        }
        Ok(properties)
    }

    fn memoized_capture(
        &self,
        path: &Path,
        identity: &Identity,
    ) -> Option<BTreeMap<String, String>> {
        self.control
            .lock()
            .unwrap()
            .capture_metadata
            .get(path)
            .filter(|(old, _)| old == identity)
            .map(|(_, properties)| properties.clone())
    }

    fn remember_capture(
        &self,
        path: &Path,
        identity: Identity,
        properties: &BTreeMap<String, String>,
    ) {
        if properties.contains_key("EXIF_CAPTURE_CHECKED") {
            self.control
                .lock()
                .unwrap()
                .capture_metadata
                .insert(path.to_owned(), (identity, properties.clone()));
        }
    }

    fn refresh_capture(
        &self,
        path: &Path,
        identity: &Identity,
    ) -> Result<BTreeMap<String, String>> {
        if Identity::read(path)? != *identity {
            return Err("source changed before reading capture metadata".into());
        }
        let properties = crate::source::read_capture_properties(path)?;
        if Identity::read(path)? != *identity {
            return Err("source changed while reading capture metadata".into());
        }
        self.remember_capture(path, identity.clone(), &properties);
        Ok(properties)
    }

    pub fn open(&self, path: &Path, progress: impl Fn(LoadStage)) -> Result<Arc<Source>> {
        if let Some(source) = self.open_cached(path, &progress) {
            return Ok(source);
        }
        let settings = self.control.lock().unwrap().settings.clone();
        let identity = settings
            .enabled
            .then(|| Identity::read(path).ok())
            .flatten();
        let (source, original) = Source::open_with_original(path, progress)?;
        let source = Arc::new(source);
        if let Some(identity) = identity {
            let mut control = self.control.lock().unwrap();
            if source
                .metadata
                .processing_family()
                .is_ok_and(|f| control.settings.permits(f))
            {
                control.stores.insert(path.to_owned());
                if self
                    .writer
                    .try_send(Command::Store {
                        path: path.to_owned(),
                        identity,
                        source: source.clone(),
                        original,
                    })
                    .is_ok()
                {
                    let mut status = self.status.lock().unwrap();
                    if !status.ready.contains(path) {
                        Arc::make_mut(&mut status.files)
                            .insert(path.to_owned(), EntryState::Queued);
                    }
                } else {
                    control.stores.remove(path);
                }
            }
        }
        Ok(source)
    }

    /// Reuse a verified decode without decoding the original again.
    /// Older entries may refresh capture metadata using a small source read.
    pub fn open_cached(&self, path: &Path, progress: impl Fn(LoadStage)) -> Option<Arc<Source>> {
        let _scope = crate::diagnostics::scope_with(
            crate::diagnostics::Category::Cache,
            "cache.decoded.lookup",
            |fields| {
                fields.str("source", &path.to_string_lossy());
            },
        );
        crate::priority::decode_priority();
        let settings = self.control.lock().unwrap().settings.clone();
        if !settings.enabled {
            crate::diagnostics::event(
                crate::diagnostics::Category::Cache,
                "cache.decoded.miss",
                |fields| {
                    fields.str("reason", "disabled");
                    fields.str("source", &path.to_string_lossy());
                },
            );
            return None;
        }
        let lookup_started = Instant::now();
        let identity_scope = crate::diagnostics::scope(
            crate::diagnostics::Category::Cache,
            "cache.decoded.identity",
        );
        let known = self.control.lock().unwrap().identities.get(path).cloned();
        let Some(identity) = known
            .clone()
            .map_or_else(|| Identity::read(path).ok(), Some)
        else {
            crate::diagnostics::event(
                crate::diagnostics::Category::Cache,
                "cache.decoded.miss",
                |fields| {
                    fields.str("reason", "source identity unavailable");
                    fields.str("source", &path.to_string_lossy());
                },
            );
            return None;
        };
        let location = self.root.join(identity.name());
        let lookup =
            crate::profile::Timing::elapsed("decoded cache identity lookup", lookup_started);
        drop(identity_scope);
        match load(&location, &identity, &settings, &progress) {
            Ok(mut source)
                if {
                    let _scope = crate::diagnostics::scope(
                        crate::diagnostics::Category::Cache,
                        "cache.decoded.identity.verify",
                    );
                    if known.is_some() {
                        self.control.lock().unwrap().identities.get(path) == Some(&identity)
                    } else {
                        Identity::read(path).ok().as_ref() == Some(&identity)
                    }
                } =>
            {
                crate::diagnostics::event(
                    crate::diagnostics::Category::Cache,
                    "cache.decoded.hit",
                    |fields| {
                        fields.str("source", &path.to_string_lossy());
                        fields.u64("bytes", source.retained_source_bytes());
                    },
                );
                if source
                    .metadata
                    .properties
                    .contains_key("FOVEONIC_LEGACY_CAPTURE_METADATA")
                {
                    let properties = self
                        .memoized_capture(path, &identity)
                        .or_else(|| self.refresh_capture(path, &identity).ok());
                    let target = &mut Arc::make_mut(&mut source.metadata).properties;
                    target.remove("FOVEONIC_LEGACY_CAPTURE_METADATA");
                    if let Some(properties) = properties {
                        target.extend(
                            properties
                                .into_iter()
                                .filter(|(name, _)| name.starts_with("EXIF_")),
                        );
                    }
                }
                self.control.lock().unwrap().rejected.remove(&identity);
                source.timings.insert(0, lookup);
                self.status.lock().unwrap().hits += 1;
                Some(Arc::new(source))
            }
            result => {
                crate::diagnostics::event(
                    crate::diagnostics::Category::Cache,
                    "cache.decoded.miss",
                    |fields| {
                        fields.str("source", &path.to_string_lossy());
                        fields.str(
                            "reason",
                            &result.as_ref().err().map_or_else(
                                || "source identity changed".to_owned(),
                                |error| error.to_string(),
                            ),
                        );
                    },
                );
                let rejected = result.as_ref().is_err_and(|error| {
                    !error.is::<crate::priority::Interrupted>() && !error.is::<PolicyMiss>()
                });
                if let Err(error) = &result {
                    if reportable_lookup_error(error.as_ref()) {
                        static OCCURRENCES: std::sync::atomic::AtomicUsize =
                            std::sync::atomic::AtomicUsize::new(0);
                        let count =
                            OCCURRENCES.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                        if count.is_power_of_two() {
                            crate::support::warning_error(
                                "decode_cache.read",
                                error.as_ref(),
                                crate::support::OperationContext {
                                    source: Some(path.display().to_string()),
                                    settings: Some(serde_json::json!({"occurrences":count})),
                                    ..Default::default()
                                },
                            );
                        }
                    }
                }
                if rejected || result.is_ok() {
                    let mut control = self.control.lock().unwrap();
                    if control.settings == settings
                        && control
                            .identities
                            .get(path)
                            .is_none_or(|current| current == &identity)
                    {
                        if rejected {
                            control.rejected.insert(identity);
                        }
                        control.identities.remove(path);
                        let mut status = self.status.lock().unwrap();
                        status.ready.remove(path);
                        if !matches!(
                            status.files.get(path),
                            Some(EntryState::Disabled | EntryState::Unsupported)
                        ) {
                            Arc::make_mut(&mut status.files)
                                .insert(path.to_owned(), EntryState::Uncached);
                        }
                    }
                }
                self.status.lock().unwrap().misses += 1;
                None
            }
        }
    }
}

/// Diagnostic suites may isolate disposable entries while retaining the ordinary
/// cache implementation and its temporary-storage policy.
fn diagnostic_root(root: PathBuf, namespace: Option<&std::ffi::OsStr>) -> Result<PathBuf> {
    let Some(namespace) = namespace else {
        return Ok(root);
    };
    let name = namespace
        .to_str()
        .ok_or("diagnostic cache namespace must be ASCII")?;
    if name.is_empty()
        || name.len() > 96
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(
            "diagnostic cache namespace must contain 1..96 ASCII letters, digits, '-' or '_'"
                .into(),
        );
    }
    Ok(root.join("diagnostics").join(name))
}

#[derive(Debug)]
struct PolicyMiss(&'static str);

fn reportable_lookup_error(error: &(dyn std::error::Error + 'static)) -> bool {
    !error.is::<crate::priority::Interrupted>()
        && !error.is::<PolicyMiss>()
        && !error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
}

#[cfg(test)]
#[test]
fn cache_warning_excludes_misses_and_cooperative_cancellation() {
    assert!(!reportable_lookup_error(&crate::priority::Interrupted));
    assert!(!reportable_lookup_error(&PolicyMiss("disabled")));
    assert!(!reportable_lookup_error(&std::io::Error::from(
        std::io::ErrorKind::NotFound
    )));
    assert!(reportable_lookup_error(&std::io::Error::from(
        std::io::ErrorKind::PermissionDenied
    )));
    assert!(reportable_lookup_error(&std::io::Error::other(
        "checksum mismatch"
    )));
}
impl std::fmt::Display for PolicyMiss {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}
impl std::error::Error for PolicyMiss {}

fn load(
    path: &Path,
    identity: &Identity,
    settings: &Settings,
    progress: &impl Fn(LoadStage),
) -> Result<Source> {
    use crate::diagnostics::{scope, scope_with, Category};

    let start = Instant::now();
    let mut file = {
        let _scope = scope(Category::Cache, "cache.decoded.file.open");
        File::open(path)?
    };
    let header = {
        let _scope = scope(Category::Cache, "cache.decoded.header");
        read_header(&mut file, identity, settings)?
    };
    let header_timing = crate::profile::Timing::elapsed("decoded cache header", start);
    let location = String::from_utf8_lossy(&identity.location);
    let mut file = crate::diagnostics::io::ObservedReader::new(
        file,
        "decoded-cache",
        Path::new(location.as_ref()),
    );
    let payload_started = Instant::now();
    let (pixels, camf) = {
        let _scope = scope_with(Category::Cache, "cache.decoded.payload", |fields| {
            fields
                .u64("samples", header.samples)
                .u64("camf_bytes", header.camf_bytes)
                .u64("chunk_bytes", crate::priority::IO_CHUNK_BYTES as u64);
        });
        let compact = matches!(header.sample_encoding, samples::Encoding::Unsigned16)
            && header
                .properties
                .get("CAMMODEL")
                .and_then(|name| ProcessingFamily::from_camera_model(name))
                == Some(ProcessingFamily::Merrill);
        let (mut pixels, mut camf) = {
            let _scope = scope(Category::Cache, "cache.decoded.payload.allocate");
            let len = usize::try_from(header.samples)?;
            let pixels = {
                let _scope = scope(Category::Cache, "cache.decoded.samples.allocate");
                if compact {
                    SensorSamples::Unsigned(vec![0; len])
                } else {
                    SensorSamples::Float(vec![0.; len])
                }
            };
            let camf = {
                let _scope = scope(Category::Cache, "cache.decoded.camf.allocate");
                vec![0; usize::try_from(header.camf_bytes)?]
            };
            (pixels, camf)
        };
        let mut checksum = crc32fast::Hasher::new();
        match &mut pixels {
            SensorSamples::Unsigned(pixels) => {
                for output in pixels.chunks_mut(SAMPLES_PER_CHUNK) {
                    crate::priority::checkpoint_or_cancel()?;
                    let bytes = bytemuck::cast_slice_mut::<u16, u8>(output);
                    {
                        let _scope = scope(Category::Cache, "cache.decoded.samples.read");
                        file.read_exact(bytes)?;
                    }
                    {
                        let _scope = scope(Category::Cache, "cache.decoded.samples.checksum");
                        checksum.update(bytes);
                    }
                }
            }
            SensorSamples::Float(pixels) => {
                let mut codes = vec![0_u16; SAMPLES_PER_CHUNK.min(pixels.len())];
                for output in pixels.chunks_mut(SAMPLES_PER_CHUNK) {
                    crate::priority::checkpoint_or_cancel()?;
                    let codes = &mut codes[..output.len()];
                    let bytes = bytemuck::cast_slice_mut::<u16, u8>(codes);
                    {
                        let _scope = scope(Category::Cache, "cache.decoded.samples.read");
                        file.read_exact(bytes)?;
                    }
                    {
                        let _scope = scope(Category::Cache, "cache.decoded.samples.checksum");
                        checksum.update(bytes);
                    }
                    {
                        let _scope = scope(Category::Cache, "cache.decoded.samples.convert");
                        header.sample_encoding.decode(codes, output);
                    }
                }
            }
            SensorSamples::UnsignedPlanes(_) => unreachable!("cache reads use interleaved storage"),
        }
        for chunk in camf.chunks_mut(crate::priority::IO_CHUNK_BYTES) {
            crate::priority::checkpoint_or_cancel()?;
            {
                let _scope = scope(Category::Cache, "cache.decoded.camf.read");
                file.read_exact(chunk)?;
            }
            {
                let _scope = scope(Category::Cache, "cache.decoded.camf.checksum");
                checksum.update(chunk);
            }
        }
        {
            let _scope = scope(Category::Cache, "cache.decoded.payload.verify");
            if checksum.finalize() != header.checksum {
                return Err("decoded cache checksum mismatch".into());
            }
        }
        (pixels, camf)
    };
    file.complete();
    let payload_timing = crate::profile::Timing::elapsed(
        if matches!(pixels, SensorSamples::Unsigned(_)) {
            "decoded cache payload read and checksum"
        } else {
            "decoded cache payload read, checksum and conversion"
        },
        payload_started,
    );
    progress(LoadStage::FileRead);
    let metadata_started = Instant::now();
    let (matrices, lists) = {
        let _scope = scope(Category::Cache, "cache.decoded.camf.parse");
        crate::x3f::cached_camera_metadata(&camf)?
    };
    let mut properties = header.properties;
    if header.version == 2 {
        properties.insert("FOVEONIC_LEGACY_CAPTURE_METADATA".into(), "1".into());
    }
    let source = Source {
        origin: SourceOrigin::File(identity.clone()),
        metadata: Arc::new(Metadata {
            width: header.width,
            height: header.height,
            rotation: header.rotation,
            processing_version: header.processing_version,
            properties,
            matrices,
            lists,
        }),
        pixels,
        storage: if header.packed_phases {
            SourceStorage::PackedRgbBluePhases
        } else {
            SourceStorage::InterleavedRgb
        },
        camf,
        timings: vec![
            header_timing,
            payload_timing,
            crate::profile::Timing::elapsed("cached camera metadata", metadata_started),
            crate::profile::Timing::elapsed("decoded cache read", start),
        ],
    };
    {
        let _scope = scope(Category::Cache, "cache.decoded.source.validate");
        source.validate()?;
    }
    {
        let _scope = scope(Category::Cache, "cache.decoded.recency");
        mark_used(path);
    }
    progress(LoadStage::Decoded);
    Ok(source)
}

fn read_header(file: &mut File, identity: &Identity, settings: &Settings) -> Result<Header> {
    let total = file.metadata()?.len();
    if total > settings.max_bytes {
        return Err(Box::new(PolicyMiss("decoded cache entry exceeds budget")));
    }
    let mut magic = [0; 8];
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err("invalid decoded cache signature".into());
    }
    let mut length = [0; 8];
    file.read_exact(&mut length)?;
    let length = u64::from_le_bytes(length);
    if length > MAX_HEADER {
        return Err("decoded cache header too large".into());
    }
    let mut header_checksum = [0; 4];
    file.read_exact(&mut header_checksum)?;
    let mut json = vec![0; usize::try_from(length)?];
    file.read_exact(&mut json)?;
    if crc32fast::hash(&json) != u32::from_le_bytes(header_checksum) {
        return Err("decoded cache metadata checksum mismatch".into());
    }
    let header: Header = serde_json::from_slice(&json)?;
    if !matches!(header.version, 2 | FORMAT_VERSION)
        || &header.identity != identity
        || header.little_endian != cfg!(target_endian = "little")
    {
        return Err("obsolete decoded cache entry".into());
    }
    let family = header
        .properties
        .get("CAMMODEL")
        .and_then(|m| ProcessingFamily::from_camera_model(m))
        .ok_or("unknown cached source family")?;
    if !settings.permits(family) {
        return Err(Box::new(PolicyMiss("source family cache disabled")));
    }
    let payload = header
        .samples
        .checked_mul(size_of::<u16>() as u64)
        .and_then(|n| n.checked_add(header.camf_bytes))
        .ok_or("decoded cache size overflow")?;
    if header.camf_bytes > MAX_CAMF || payload.checked_add(length + PREFIX_BYTES) != Some(total) {
        return Err("truncated decoded cache entry".into());
    }
    let cells = header
        .width
        .checked_mul(header.height)
        .ok_or("cached source geometry overflow")?;
    let expected_samples = if header.packed_phases {
        if !header.width.is_multiple_of(2) || !header.height.is_multiple_of(2) {
            return Err("invalid cached cell geometry".into());
        }
        // Each 2×2 sensor cell stores R, G, and four blue phases.
        cells.checked_div(4).and_then(|v| v.checked_mul(6))
    } else {
        cells.checked_mul(3)
    };
    if expected_samples.and_then(|n| u64::try_from(n).ok()) != Some(header.samples) {
        return Err("cached sample count mismatch".into());
    }
    Ok(header)
}

fn ready(root: &Path, path: &Path, settings: &Settings) -> bool {
    (|| -> Result<()> {
        let identity = Identity::read(path)?;
        let mut file = File::open(root.join(identity.name()))?;
        read_header(&mut file, &identity, settings)?;
        Ok(())
    })()
    .is_ok()
}

#[cfg(test)]
fn store(root: &Path, path: &Path, identity: Identity, source: &Source, budget: u64) -> Result<()> {
    store_checked(root, path, identity, source, budget, &[], || true).map(|_| ())
}
fn store_checked(
    root: &Path,
    path: &Path,
    identity: Identity,
    source: &Source,
    budget: u64,
    nearby: &[PathBuf],
    current: impl Fn() -> bool,
) -> Result<bool> {
    if !current() {
        return Ok(false);
    }
    if Identity::read(path)? != identity {
        return Err("source changed before caching".into());
    }
    let prepared = crate::decode_pool::cpu(|| prepare_decoded(identity, source))?;
    publish_decoded(root, path, source, prepared, budget, nearby, current)
}

struct PreparedDecoded {
    identity: Identity,
    sample_encoding: samples::Encoding,
    json: Vec<u8>,
    header_checksum: u32,
    bytes: u64,
}

fn for_code_chunks(
    values: &SensorSamples,
    encoding: samples::Encoding,
    mut consume: impl FnMut(&[u16]) -> Result<()>,
) -> Result<()> {
    match values {
        SensorSamples::Unsigned(values) => {
            for chunk in values.chunks(SAMPLES_PER_CHUNK) {
                crate::priority::checkpoint_or_cancel()?;
                consume(chunk)?;
            }
        }
        SensorSamples::UnsignedPlanes(planes) => {
            let pixels_per_chunk = SAMPLES_PER_CHUNK / 3;
            let mut codes = Vec::with_capacity(pixels_per_chunk.min(planes[0].len()) * 3);
            for ((red, green), blue) in planes[0]
                .chunks(pixels_per_chunk)
                .zip(planes[1].chunks(pixels_per_chunk))
                .zip(planes[2].chunks(pixels_per_chunk))
            {
                crate::priority::checkpoint_or_cancel()?;
                codes.clear();
                codes.extend(
                    red.iter()
                        .zip(green)
                        .zip(blue)
                        .flat_map(|((&r, &g), &b)| [r, g, b]),
                );
                consume(&codes)?;
            }
        }
        SensorSamples::Float(values) => {
            let mut codes = vec![0_u16; SAMPLES_PER_CHUNK.min(values.len())];
            for chunk in values.chunks(SAMPLES_PER_CHUNK) {
                crate::priority::checkpoint_or_cancel()?;
                let codes = &mut codes[..chunk.len()];
                encoding.encode(chunk, codes)?;
                consume(codes)?;
            }
        }
    }
    Ok(())
}

/// Check compact sample encoding without allocating another full sensor buffer.
fn prepare_decoded(identity: Identity, source: &Source) -> Result<PreparedDecoded> {
    let sample_encoding = match &source.pixels {
        SensorSamples::Unsigned(_) | SensorSamples::UnsignedPlanes(_) => {
            samples::Encoding::Unsigned16
        }
        SensorSamples::Float(values) => samples::Encoding::for_samples(values),
    };
    let mut checksum = crc32fast::Hasher::new();
    for_code_chunks(&source.pixels, sample_encoding, |codes| {
        checksum.update(bytemuck::cast_slice::<u16, u8>(codes));
        Ok(())
    })?;
    checksum.update(&source.camf);
    let header = Header {
        version: FORMAT_VERSION,
        identity: identity.clone(),
        little_endian: cfg!(target_endian = "little"),
        width: source.metadata.width,
        height: source.metadata.height,
        rotation: source.metadata.rotation,
        processing_version: source.metadata.processing_version,
        properties: source.metadata.properties.clone(),
        packed_phases: source.storage() == SourceStorage::PackedRgbBluePhases,
        sample_encoding,
        samples: source.pixels.len() as u64,
        camf_bytes: source.camf.len() as u64,
        checksum: checksum.finalize(),
    };
    let json = serde_json::to_vec(&header)?;
    let bytes = PREFIX_BYTES
        + json.len() as u64
        + source.pixels.len() as u64 * size_of::<u16>() as u64
        + source.camf.len() as u64;
    Ok(PreparedDecoded {
        identity,
        sample_encoding,
        header_checksum: crc32fast::hash(&json),
        json,
        bytes,
    })
}

fn publish_decoded(
    root: &Path,
    path: &Path,
    source: &Source,
    prepared: PreparedDecoded,
    budget: u64,
    nearby: &[PathBuf],
    current: impl Fn() -> bool,
) -> Result<bool> {
    let PreparedDecoded {
        identity,
        sample_encoding,
        json,
        header_checksum,
        bytes,
    } = prepared;
    if bytes > budget || json.len() as u64 > MAX_HEADER {
        return Ok(false);
    }
    if !current() {
        return Ok(false);
    }
    if Identity::read(path)? != identity {
        return Err("source changed before caching".into());
    }
    fs::create_dir_all(root)?;
    if !lru::reserve(root, &root.join(identity.name()), bytes, budget, nearby)? {
        return Ok(false);
    }
    let temporary = root.join(format!(
        "{}.{}.writing",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(MAGIC)?;
        file.write_all(&(json.len() as u64).to_le_bytes())?;
        file.write_all(&header_checksum.to_le_bytes())?;
        file.write_all(&json)?;
        for_code_chunks(&source.pixels, sample_encoding, |codes| {
            file.write_all(bytemuck::cast_slice::<u16, u8>(codes))?;
            Ok(())
        })?;
        file.write_all(&source.camf)?;
        file.flush()?;
        drop(file);
        if !current() || Identity::read(path)? != identity {
            return Err("source or cache policy changed during caching".into());
        }
        fs::rename(&temporary, root.join(identity.name()))?;
        Ok(())
    })();
    let _ = fs::remove_file(temporary);
    result.map(|()| true)
}

fn boot_key() -> String {
    #[cfg(target_os = "macos")]
    let boot = std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", "kern.boottime"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| out.stdout);
    #[cfg(target_os = "linux")]
    let boot = fs::read("/proc/sys/kernel/random/boot_id").ok();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let boot = format!("{}-{:?}", std::process::id(), SystemTime::now()).into_bytes();
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let boot = boot
        .unwrap_or_else(|| format!("{}-{:?}", std::process::id(), SystemTime::now()).into_bytes());
    let mut key = DefaultHasher::new();
    boot.hash(&mut key);
    format!("{:016x}", key.finish())
}

#[cfg(test)]
pub(crate) mod tests;
