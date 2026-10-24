//! Reuses exact decoded sources and display previews in a bounded temporary disk LRU.

pub use crate::background::Activity;
#[path = "decode_cache_lru.rs"]
mod lru;
#[path = "decode_cache_window.rs"]
mod window;
use lru::prune;
#[path = "decode_cache_preview.rs"]
mod previews;
#[path = "decode_cache_worker.rs"]
mod worker;
use crate::source::{LoadStage, Metadata, ProcessingFamily, Result, Source, SourceStorage};
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
    time::{Instant, SystemTime, UNIX_EPOCH},
};

// Bump when decoder semantics or the stored source representation change.
const FORMAT_VERSION: u32 = 1;
const NAMESPACE: &str = "foveonic-decoded-v1";
pub const DEFAULT_BUDGET: u64 = 10_000_000_000;
// Start all three sensor layers of the nearest photo together by default.
const DEFAULT_DECODE_THREADS: usize = 3;
const DEFAULT_PREFETCH_BEHIND: usize = 3;
const DEFAULT_PREFETCH_AHEAD: usize = 7;
const MAGIC: &[u8; 8] = b"FOVDEC01";
// Signature, JSON length and JSON checksum precede the versioned header.
const PREFIX_BYTES: u64 = (MAGIC.len() + size_of::<u64>() + size_of::<u32>()) as u64;
const MAX_HEADER: u64 = 1024 * 1024;
const MAX_CAMF: u64 = 64 * 1024 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    pub max_bytes: u64,
    /// Shared CPU workers for speculative sensor layers, across nearby photos.
    pub decode_threads: usize,
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
            decode_threads: available_decode_threads().min(DEFAULT_DECODE_THREADS),
            prefetch_behind: DEFAULT_PREFETCH_BEHIND,
            prefetch_ahead: DEFAULT_PREFETCH_AHEAD,
            classic: false,
            merrill: true,
            quattro: true,
        }
    }
}
/// Available logical CPU cores, including when the platform cannot report a count.
pub fn available_decode_threads() -> usize {
    std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
}
impl Settings {
    pub fn normalize(&mut self) {
        self.decode_threads = self.decode_threads.clamp(1, available_decode_threads());
    }
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
struct Identity {
    location: Vec<u8>,
    length: u64,
    modified_seconds: u64,
    modified_nanos: u32,
}
impl Identity {
    fn read(path: &Path) -> Result<Self> {
        let canonical = path.canonicalize()?;
        let metadata = fs::metadata(&canonical)?;
        let modified = metadata.modified()?.duration_since(UNIX_EPOCH)?;
        Ok(Self {
            location: canonical.as_os_str().as_encoded_bytes().to_vec(),
            length: metadata.len(),
            modified_seconds: modified.as_secs(),
            modified_nanos: modified.subsec_nanos(),
        })
    }
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
    samples: u64,
    camf_bytes: u64,
    checksum: u32,
}

struct Control {
    settings: Settings,
    clear: bool,
    watched: Vec<PathBuf>,
    selected: Option<PathBuf>,
    nearby: Vec<PathBuf>,
    queue: VecDeque<PathBuf>,
    stores: HashSet<PathBuf>,
    generation: u64,
    preview: Option<previews::PendingPreview>,
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
    },
}

/// Prefetch retains at most one source per decode worker. Foreground insertion
/// allows one source write and one pending source. A full queue drops cache
/// insertion, never an image request. Sources are shared, not copied for writing.
pub struct Cache {
    root: PathBuf,
    control: Arc<Mutex<Control>>,
    status: Arc<Mutex<Status>>,
    writer: mpsc::SyncSender<Command>,
    gate: Arc<crate::background::Gate>,
}
impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}
impl Cache {
    /// Start inactive. The application supplies its persistent policy with `configure`.
    pub fn new() -> Self {
        Self::at(std::env::temp_dir().join(NAMESPACE).join(boot_key()))
    }
    fn at(root: PathBuf) -> Self {
        let control = Arc::new(Mutex::new(Control {
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
        }));
        let status = Arc::new(Mutex::new(Status::default()));
        let gate = Arc::new(crate::background::Gate::default());
        let writer = worker::start(root.clone(), control.clone(), status.clone(), gate.clone());
        Self {
            root,
            control,
            status,
            writer,
            gate,
        }
    }
    pub fn configure(&self, mut settings: Settings) {
        settings.normalize();
        let mut state = self.control.lock().unwrap();
        state.settings = settings;
        state.restart_prefetch();
        self.gate.interrupt_prefetch();
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
        state.preview = None;
        state.queue.clear();
        state.stores.clear();
        state.generation += 1;
        self.gate.interrupt_prefetch();
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
        state.selected = selected;
        state.restart_prefetch();
        self.gate.interrupt_prefetch();
        drop(state);
        if order_changed {
            let mut status = self.status.lock().unwrap();
            status.ready.clear();
            status.files = Arc::new(HashMap::new());
        }
        let _ = self.writer.try_send(Command::Maintain);
    }
    pub fn foreground(&self) -> Activity {
        self.gate.foreground()
    }
    /// Run optional presentation work on a dedicated background thread.
    /// This lowers that thread's scheduling priority and shares prefetch's foreground gate.
    pub fn background<T>(&self, work: impl FnOnce() -> T) -> T {
        crate::background::lower_priority();
        crate::background::run(Some(self.gate.clone()), || {
            crate::background::checkpoint();
            work()
        })
    }
    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
    pub fn directory(&self) -> &Path {
        &self.root
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
        let source = Arc::new(Source::open_with_progress(path, progress)?);
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

    /// Reuse a verified decode without falling back to reading the original.
    /// Import can decode its completed local copy when the card cache is missing.
    pub fn open_cached(&self, path: &Path, progress: impl Fn(LoadStage)) -> Option<Arc<Source>> {
        crate::background::decode_priority();
        let settings = self.control.lock().unwrap().settings.clone();
        if !settings.enabled {
            return None;
        }
        let identity = Identity::read(path).ok()?;
        let location = self.root.join(identity.name());
        match load(&location, &identity, &settings, &progress) {
            Ok(source) if Identity::read(path).ok().as_ref() == Some(&identity) => {
                self.status.lock().unwrap().hits += 1;
                Some(Arc::new(source))
            }
            _ => {
                self.status.lock().unwrap().misses += 1;
                None
            }
        }
    }
}

fn load(
    path: &Path,
    identity: &Identity,
    settings: &Settings,
    progress: &impl Fn(LoadStage),
) -> Result<Source> {
    let start = Instant::now();
    let mut file = File::open(path)?;
    let header = read_header(&mut file, identity, settings)?;
    let mut pixels = vec![0_f32; usize::try_from(header.samples)?];
    let mut camf = vec![0; usize::try_from(header.camf_bytes)?];
    let mut checksum = crc32fast::Hasher::new();
    for output in [
        bytemuck::cast_slice_mut::<f32, u8>(&mut pixels),
        camf.as_mut_slice(),
    ] {
        for chunk in output.chunks_mut(crate::background::IO_CHUNK_BYTES) {
            crate::background::checkpoint_or_cancel()?;
            file.read_exact(chunk)?;
            checksum.update(chunk);
        }
    }
    if checksum.finalize() != header.checksum {
        return Err("decoded cache checksum mismatch".into());
    }
    progress(LoadStage::FileRead);
    let (matrices, lists) = crate::x3f::cached_camera_metadata(&camf)?;
    let source = Source {
        metadata: Arc::new(Metadata {
            width: header.width,
            height: header.height,
            rotation: header.rotation,
            processing_version: header.processing_version,
            properties: header.properties,
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
        timings: vec![crate::profile::Timing::elapsed("decoded cache read", start)],
    };
    source.validate()?;
    let _ = file.set_modified(SystemTime::now());
    progress(LoadStage::Decoded);
    Ok(source)
}

fn read_header(file: &mut File, identity: &Identity, settings: &Settings) -> Result<Header> {
    let total = file.metadata()?.len();
    if total > settings.max_bytes {
        return Err("decoded cache entry exceeds budget".into());
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
    if header.version != FORMAT_VERSION
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
        return Err("source family cache disabled".into());
    }
    let payload = header
        .samples
        .checked_mul(std::mem::size_of::<f32>() as u64)
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
    store_checked(root, path, identity, source, budget, &[], || true)
}
fn store_checked(
    root: &Path,
    path: &Path,
    identity: Identity,
    source: &Source,
    budget: u64,
    nearby: &[PathBuf],
    current: impl Fn() -> bool,
) -> Result<()> {
    if !current() {
        return Ok(());
    }
    if Identity::read(path)? != identity {
        return Err("source changed before caching".into());
    }
    let mut checksum = crc32fast::Hasher::new();
    for bytes in
        bytemuck::cast_slice::<f32, u8>(&source.pixels).chunks(crate::background::IO_CHUNK_BYTES)
    {
        crate::background::checkpoint_or_cancel()?;
        checksum.update(bytes);
    }
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
        samples: source.pixels.len() as u64,
        camf_bytes: source.camf.len() as u64,
        checksum: checksum.finalize(),
    };
    let json = serde_json::to_vec(&header)?;
    let bytes = PREFIX_BYTES
        + json.len() as u64
        + std::mem::size_of_val(source.pixels.as_slice()) as u64
        + source.camf.len() as u64;
    if bytes > budget || json.len() as u64 > MAX_HEADER {
        return Ok(());
    }
    if !current() {
        return Ok(());
    }
    fs::create_dir_all(root)?;
    if !lru::reserve(root, &root.join(identity.name()), bytes, budget, nearby)? {
        return Ok(());
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
        file.write_all(&crc32fast::hash(&json).to_le_bytes())?;
        file.write_all(&json)?;
        for bytes in bytemuck::cast_slice::<f32, u8>(&source.pixels)
            .chunks(crate::background::IO_CHUNK_BYTES)
        {
            crate::background::checkpoint_or_cancel()?;
            file.write_all(bytes)?;
        }
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
    result
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
    let boot: Option<Vec<u8>> = None;
    let boot = boot
        .unwrap_or_else(|| format!("{}-{:?}", std::process::id(), SystemTime::now()).into_bytes());
    let mut key = DefaultHasher::new();
    boot.hash(&mut key);
    format!("{:016x}", key.finish())
}

#[cfg(test)]
#[path = "decode_cache_test.rs"]
mod tests;
