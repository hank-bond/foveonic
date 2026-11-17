//! Records bounded application traces through Perfetto with shared clocks and scalar correlation.

pub use crate::profile::Mode;

use perfetto_sdk::track_event::{
    EventContext, TrackEventCounter, TrackEventDebugArg, TrackEventFlow, TrackEventProtoField,
    TrackEventProtoTrack, TrackEventTimestamp, TrackEventTrack, TrackEventType,
};
pub(crate) mod io;

use std::{
    cell::Cell,
    ffi::CString,
    hash::{DefaultHasher, Hash, Hasher},
    marker::PhantomData,
    rc::Rc,
    sync::{
        atomic::{AtomicU64, AtomicU8, Ordering},
        RwLock,
    },
    time::Instant,
};

#[path = "diagnostics_capture.rs"]
mod capture;
pub use capture::{CaptureConfig, Recorder, SavedCapture};

perfetto_sdk::track_event_categories! {
    pub(crate) mod categories {
        ("foveonic.ui", "Native interaction", []),
        ("foveonic.coordinator", "Desired work and delivery", []),
        ("foveonic.cache", "Cache ownership and decisions", []),
        ("foveonic.io", "Actual input/output access", []),
        ("foveonic.cpu", "Host processing", []),
        ("foveonic.graph", "Dependency graph execution", []),
        ("foveonic.gpu", "Device submissions and observations", []),
        ("foveonic.presentation", "Image use by native drawing", []),
        ("foveonic.scenario", "Controlled application actions", []),
        ("foveonic.memory", "Unique retained resources", []),
    }
}

/// Stable categories share the order of the SDK registrations above.
#[derive(Clone, Copy, Debug)]
#[repr(usize)]
pub enum Category {
    Ui,
    Coordinator,
    Cache,
    Io,
    Cpu,
    Graph,
    Gpu,
    Presentation,
    Scenario,
    Memory,
}

static MODE: AtomicU8 = AtomicU8::new(0);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static EMISSION: RwLock<()> = RwLock::new(());
static TRANSITION_DROPS: AtomicU64 = AtomicU64::new(0);

pub fn active_mode() -> Mode {
    match MODE.load(Ordering::Acquire) {
        1 => Mode::Wall,
        2 => Mode::Gpu,
        _ => Mode::Off,
    }
}
#[inline]
pub fn enabled() -> bool {
    MODE.load(Ordering::Relaxed) != 0
}
pub fn capture_id() -> Option<u64> {
    enabled().then(|| GENERATION.load(Ordering::Acquire))
}

/// Scalar correlation only. Keeping context never keeps pixels or a Source alive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Context {
    /// Foreground coordinator task. Zero denotes work outside that coordinator.
    pub task: u64,
    pub revision: u64,
    pub photo: u64,
    pub purpose: &'static str,
    pub node: Option<u64>,
}

/// Identify a source in a capture without opening, canonicalizing or retaining it.
pub fn photo_id(path: &std::path::Path) -> u64 {
    let mut hash = DefaultHasher::new();
    path.hash(&mut hash);
    hash.finish()
}
thread_local! {
    static CONTEXT: Cell<Option<Context>> = const { Cell::new(None) };
}
pub fn current_context() -> Option<Context> {
    enabled().then(|| CONTEXT.get()).flatten()
}
/// A thread-local context must be restored on the thread that entered it.
pub struct ContextGuard {
    previous: Option<Option<Context>>,
    thread: PhantomData<Rc<()>>,
}
impl Drop for ContextGuard {
    fn drop(&mut self) {
        if let Some(previous) = self.previous {
            CONTEXT.set(previous);
        }
    }
}
pub fn enter(context: Context) -> ContextGuard {
    replace_context(Some(context))
}
fn replace_context(context: Option<Context>) -> ContextGuard {
    ContextGuard {
        previous: enabled().then(|| CONTEXT.replace(context)),
        thread: PhantomData,
    }
}
pub fn with_context<T>(context: Option<Context>, work: impl FnOnce() -> T) -> T {
    let _context = replace_context(context);
    work()
}

/// A timestamp in Perfetto's CPU clock, belonging to one capture generation.
#[derive(Clone, Copy, Debug)]
pub struct Timestamp {
    clock: u32,
    nanoseconds: u64,
    capture: u64,
}
impl Timestamp {
    pub fn nanoseconds(self) -> u64 {
        self.nanoseconds
    }
}
pub fn timestamp() -> Option<Timestamp> {
    let capture = capture_id()?;
    let now = TrackEventTimestamp::now();
    Some(Timestamp {
        clock: now.clock_id(),
        nanoseconds: now.timestamp(),
        capture,
    })
}
/// Anchor an already measured host interval to the backend clock. No second
/// origin or independently placed timeline is introduced by legacy measurements.
pub fn timestamp_at(instant: Instant) -> Option<Timestamp> {
    if !enabled() {
        return None;
    }
    let host = Instant::now();
    let mut timestamp = timestamp()?;
    timestamp.nanoseconds = timestamp.nanoseconds.saturating_sub(
        host.saturating_duration_since(instant)
            .as_nanos()
            .min(u64::MAX as u128) as u64,
    );
    Some(timestamp)
}

/// Typed attributes are constructed only inside an enabled recording callback.
pub struct Fields<'a>(&'a mut EventContext);
impl Fields<'_> {
    pub fn u64(&mut self, key: &str, value: u64) -> &mut Self {
        self.0.add_debug_arg(key, TrackEventDebugArg::Uint64(value));
        self
    }
    pub fn i64(&mut self, key: &str, value: i64) -> &mut Self {
        self.0.add_debug_arg(key, TrackEventDebugArg::Int64(value));
        self
    }
    pub fn f64(&mut self, key: &str, value: f64) -> &mut Self {
        self.0.add_debug_arg(key, TrackEventDebugArg::Double(value));
        self
    }
    pub fn bool(&mut self, key: &str, value: bool) -> &mut Self {
        self.0.add_debug_arg(key, TrackEventDebugArg::Bool(value));
        self
    }
    pub fn str(&mut self, key: &str, value: &str) -> &mut Self {
        if value.contains('\0') {
            self.0
                .add_debug_arg(key, TrackEventDebugArg::String(&value.replace('\0', "�")));
        } else {
            self.0.add_debug_arg(key, TrackEventDebugArg::String(value));
        }
        self
    }
    pub fn flow(&mut self, id: u64, terminating: bool) -> &mut Self {
        let flow = TrackEventFlow::process_scoped_flow(id);
        if terminating {
            self.0.set_terminating_flow(&flow);
        } else {
            self.0.set_flow(&flow);
        }
        self
    }
    fn context(&mut self) {
        if let Some(context) = current_context() {
            self.u64("task", context.task)
                .u64("revision", context.revision)
                .u64("photo", context.photo)
                .str("purpose", context.purpose);
            if let Some(node) = context.node {
                self.u64("node", node);
            }
        }
    }
}

fn recording(category: Category) -> bool {
    enabled() && categories::is_category_enabled(category as usize)
}
fn emit(
    category: Category,
    variant: TrackEventType,
    expected: Option<u64>,
    fields: impl FnOnce(&mut Fields<'_>),
) -> bool {
    if !recording(category) {
        return false;
    }
    let Ok(_emission) = EMISSION.try_read() else {
        if recording(category) && (expected.is_none() || expected == capture_id()) {
            TRANSITION_DROPS.fetch_add(1, Ordering::Relaxed);
        }
        return false;
    };
    if !recording(category) || expected.is_some_and(|expected| capture_id() != Some(expected)) {
        return false;
    }
    let mut context = EventContext::default();
    // Event names can come from existing stage tables. Copy their contents
    // rather than treating a temporary CString address as a persistent name.
    context.set_no_intern();
    let mut values = Fields(&mut context);
    values.context();
    fields(&mut values);
    categories::emit(category as usize, variant, &mut context);
    true
}
pub fn event(category: Category, name: &str, fields: impl FnOnce(&mut Fields<'_>)) {
    if !recording(category) {
        return;
    }
    let name = CString::new(name).unwrap_or_default();
    emit(
        category,
        TrackEventType::Instant(name.as_ptr()),
        None,
        fields,
    );
}
/// Asynchronous observations belong to their originating capture, even if a
/// stop/start transition races their arrival. Emission never waits for that transition.
pub(crate) fn event_in_capture(
    capture: u64,
    category: Category,
    name: &str,
    fields: impl FnOnce(&mut Fields<'_>),
) -> bool {
    if !recording(category) {
        return false;
    }
    let name = CString::new(name).unwrap_or_default();
    emit(
        category,
        TrackEventType::Instant(name.as_ptr()),
        Some(capture),
        fields,
    )
}
pub fn flow(
    category: Category,
    name: &str,
    id: u64,
    terminating: bool,
    fields: impl FnOnce(&mut Fields<'_>),
) {
    event(category, name, |values| {
        values.flow(id, terminating);
        fields(values);
    });
}

/// CPU scopes enter and leave on one actual thread. Task lifetime uses flows.
pub struct Scope {
    category: Category,
    capture: Option<u64>,
    thread: PhantomData<Rc<()>>,
}
impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(capture) = self.capture {
            emit(
                self.category,
                TrackEventType::SliceEnd,
                Some(capture),
                |_| {},
            );
        }
    }
}
#[inline]
pub fn scope(category: Category, name: &str) -> Scope {
    scope_with(category, name, |_| {})
}
pub fn scope_with(category: Category, name: &str, fields: impl FnOnce(&mut Fields<'_>)) -> Scope {
    let capture = if let Some(capture) = capture_id().filter(|_| recording(category)) {
        let name = CString::new(name).unwrap_or_default();
        emit(
            category,
            TrackEventType::SliceBegin(name.as_ptr()),
            Some(capture),
            fields,
        )
        .then_some(capture)
    } else {
        None
    };
    Scope {
        category,
        capture,
        thread: PhantomData,
    }
}

/// Retrospective elapsed intervals are observations with explicit endpoints.
/// Actual CPU execution uses scoped slices. Overlapping observations neither
/// invent thread nesting nor grow a catalogue of one-use synthetic tracks.
pub fn interval(
    category: Category,
    name: &str,
    start: Timestamp,
    end: Timestamp,
    fields: impl FnOnce(&mut Fields<'_>),
) {
    if Some(start.capture) != capture_id()
        || start.capture != end.capture
        || start.clock != end.clock
        || start.nanoseconds > end.nanoseconds
    {
        return;
    }
    event_in_capture(start.capture, category, name, |values| {
        values
            .u64("start_ns", start.nanoseconds)
            .u64("end_ns", end.nanoseconds)
            .u64("duration_ns", end.nanoseconds - start.nanoseconds)
            .u64("cpu_clock_id", start.clock as u64)
            .str("interval_kind", "elapsed_cpu_clock");
        fields(values);
    });
}
pub fn interval_since(
    category: Category,
    name: &str,
    start: Instant,
    fields: impl FnOnce(&mut Fields<'_>),
) {
    if let (Some(start), Some(end)) = (timestamp_at(start), timestamp()) {
        interval(category, name, start, end, fields);
    }
}

pub fn counter(category: Category, name: &str, value: f64) {
    use perfetto_sdk::protos::trace::track_event::track_descriptor::TrackDescriptorFieldNumber as F;
    emit(category, TrackEventType::Counter, None, |values| {
        let parent = TrackEventTrack::process_track_uuid();
        values
            .0
            .set_proto_track(&TrackEventProtoTrack {
                uuid: TrackEventTrack::counter_track_uuid(name, parent),
                fields: &[
                    TrackEventProtoField::VarInt(F::ParentUuid as u32, parent),
                    TrackEventProtoField::Cstr(F::Name as u32, name),
                    TrackEventProtoField::Bytes(F::Counter as u32, &[]),
                ],
            })
            .set_counter(TrackEventCounter::Double(value));
    });
}

/// Device clock values stay associated with their submission. They are never
/// positioned on the CPU axis as though device and CPU clocks were calibrated.
pub struct GpuInterval {
    pub capture_id: u64,
    pub submission: u64,
    pub clock_id: u64,
    pub start_tick: u64,
    pub end_tick: u64,
    pub nanoseconds_per_tick: f64,
}
pub fn gpu_interval(
    name: &str,
    interval: &GpuInterval,
    fields: impl FnOnce(&mut Fields<'_>),
) -> bool {
    if active_mode() != Mode::Gpu || capture_id() != Some(interval.capture_id) {
        return false;
    }
    event_in_capture(interval.capture_id, Category::Gpu, name, |values| {
        values
            .u64("capture_id", interval.capture_id)
            .u64("submission", interval.submission)
            .u64("clock_id", interval.clock_id)
            .u64("start_tick", interval.start_tick)
            .u64("end_tick", interval.end_tick)
            .f64("nanoseconds_per_tick", interval.nanoseconds_per_tick)
            .bool("calibrated", false);
        if let Some(ticks) = interval.end_tick.checked_sub(interval.start_tick) {
            if interval.nanoseconds_per_tick.is_finite() && interval.nanoseconds_per_tick > 0. {
                values.f64("duration_ns", ticks as f64 * interval.nanoseconds_per_tick);
            }
        }
        fields(values);
    })
}

#[cfg(test)]
#[path = "diagnostics_test.rs"]
mod tests;
