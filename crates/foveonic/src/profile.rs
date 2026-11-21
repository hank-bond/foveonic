//! Collects render timings, activity counts and replayable diagnostic settings.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

static NEXT_GRAPH: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_graph_id() -> u64 {
    NEXT_GRAPH.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
#[path = "profile_gpu_test.rs"]
mod gpu_tests;
#[cfg(test)]
#[path = "profile_test.rs"]
mod tests;

/// These totals count this engine's work, not current or peak GPU memory.
/// They exclude wgpu's internal staging and work from other users of the queue.
#[derive(Clone, Copy, Default, Debug, serde::Serialize)]
pub struct GpuActivity {
    pub upload_bytes: u64,
    pub uniform_bytes: u64,
    pub storage_bytes: u64,
    pub allocations: u64,
    pub dispatches: u64,
    pub submissions: u64,
    pub device_copy_bytes: u64,
    pub readback_bytes: u64,
    pub readbacks: u64,
    pub timestamp_resolves: u64,
}
impl GpuActivity {
    /// Difference between two snapshots from the same engine.
    pub fn since(self, before: Self) -> Self {
        Self {
            upload_bytes: self.upload_bytes - before.upload_bytes,
            uniform_bytes: self.uniform_bytes - before.uniform_bytes,
            storage_bytes: self.storage_bytes - before.storage_bytes,
            allocations: self.allocations - before.allocations,
            dispatches: self.dispatches - before.dispatches,
            submissions: self.submissions - before.submissions,
            device_copy_bytes: self.device_copy_bytes - before.device_copy_bytes,
            readback_bytes: self.readback_bytes - before.readback_bytes,
            readbacks: self.readbacks - before.readbacks,
            timestamp_resolves: self.timestamp_resolves - before.timestamp_resolves,
        }
    }
}

/// Wall mode collects host observations without timestamp queries or extra GPU synchronization.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Off,
    Wall,
    Gpu,
}
impl Mode {
    pub fn enabled(self) -> bool {
        self != Self::Off
    }
    pub fn gpu(self) -> bool {
        self == Self::Gpu
    }
    pub fn check_gpu_features(self, features: wgpu::Features) -> Result<(), &'static str> {
        if self.gpu() && !features.contains(wgpu::Features::TIMESTAMP_QUERY) {
            Err("GPU profiling requires timestamp support; use wall mode otherwise")
        } else {
            Ok(())
        }
    }
}
impl std::str::FromStr for Mode {
    type Err = &'static str;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "off" => Ok(Self::Off),
            "wall" => Ok(Self::Wall),
            "gpu" => Ok(Self::Gpu),
            _ => Err("profiling mode must be off, wall or gpu"),
        }
    }
}

/// Use a shared JSON marker so tools can recognize timing reports without
/// mistaking unrelated stderr messages for measurements.
pub fn report(event: &str, mode: Mode, data: impl serde::Serialize) -> serde_json::Value {
    serde_json::json!({"foveonic_profile": 1, "event": event, "mode": mode,
                      "synchronized": mode.gpu(), "data": data})
}

/// A diagnostic render request, including transient settings omitted from photo sidecars.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct RenderSettingsSnapshot {
    settings: crate::render::Settings,
    working_scale: crate::render::Scale,
    #[serde(default)]
    processing_quality: crate::render::ProcessingQuality,
    preview_region: Option<[u32; 4]>,
    color_range_preview: Option<usize>,
    dehaze_estimate: bool,
    processing_sections: crate::render::ProcessingSections,
    adaptive_tone_tuning: crate::render::AdaptiveToneTuning,
}

impl From<&crate::render::Settings> for RenderSettingsSnapshot {
    fn from(settings: &crate::render::Settings) -> Self {
        Self {
            settings: settings.clone(),
            working_scale: settings.working_scale,
            processing_quality: settings.processing_quality,
            preview_region: settings.preview_region,
            color_range_preview: settings.color_range_preview,
            dehaze_estimate: settings.dehaze_estimate,
            processing_sections: settings.processing_sections,
            adaptive_tone_tuning: settings.adaptive_tone_tuning,
        }
    }
}

impl RenderSettingsSnapshot {
    pub fn into_settings(self) -> crate::render::Settings {
        crate::render::Settings {
            working_scale: self.working_scale,
            processing_quality: self.processing_quality,
            preview_region: self.preview_region,
            color_range_preview: self.color_range_preview,
            dehaze_estimate: self.dehaze_estimate,
            processing_sections: self.processing_sections,
            adaptive_tone_tuning: self.adaptive_tone_tuning,
            ..self.settings
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Timing {
    pub stage: &'static str,
    pub milliseconds: f64,
}

impl Timing {
    pub(crate) fn elapsed(stage: &'static str, start: Instant) -> Self {
        Self {
            stage,
            milliseconds: start.elapsed().as_secs_f64() * 1000.,
        }
    }
}

/// Output reachability and cache availability observed at graph execution start.
#[derive(Debug, serde::Serialize)]
pub struct NodeOutput {
    pub id: usize,
    pub name: String,
    pub required: bool,
    pub cached_available: bool,
    /// A missing required output requested from this operation.
    pub requested_for_execution: bool,
    /// A new value was published into the executor for this output.
    pub published: bool,
    pub resource: Option<ResourceObservation>,
}

/// Scalar identities and logical sizes. These records never own GPU storage.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ResourceObservation {
    pub allocation_id: u64,
    pub generation: u64,
    pub byte_len: u64,
    pub working_scale: u32,
    pub description: Option<ImageDescription>,
    pub view: Option<ImageView>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ImageDescription {
    pub extent: [u32; 2],
    pub layout: String,
    pub space: String,
    pub view_baked: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ImageView {
    pub crop: [u32; 4],
    pub rotation: u32,
    pub extent: [u32; 2],
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct GraphOutput {
    pub id: usize,
    pub name: String,
    pub producer: usize,
    pub dependencies: Vec<usize>,
    pub data_type: String,
    pub session_hint: bool,
    pub sensor_data: bool,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct GraphObservation {
    pub id: u64,
    pub requested_roots: Vec<usize>,
    pub outputs: Vec<GraphOutput>,
    pub boundary_memory_scope: &'static str,
}

/// Unique logical bytes held at a node boundary; excludes in-node peaks and driver residency.
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct BoundaryMemory {
    pub graph_bytes: u64,
    pub scratch_bytes: u64,
    pub cache_bytes: u64,
    pub union_bytes: u64,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct CheckpointEntry {
    pub names: Vec<String>,
    pub role: &'static str,
    pub version: u64,
    pub parents: Vec<u64>,
    pub allocation_id: u64,
    pub generation: u64,
    pub byte_len: u64,
    pub working_scale: u32,
    pub selected: bool,
    pub primary: bool,
    pub restart: bool,
    pub sensor_data: bool,
    pub priority: Option<(u8, usize)>,
    pub used_frame: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct CacheSnapshot {
    pub entries: Vec<CheckpointEntry>,
    pub versions: Vec<CheckpointVersion>,
    pub cache_structure_bytes: u64,
    pub parameter_payload_bytes: u64,
    pub preparation_bytes: u64,
    pub output_parameters: Vec<crate::render::ParameterResources>,
}

/// Exact retained ancestry without parameter payloads or GPU ownership.
#[derive(Debug, serde::Serialize)]
pub struct CheckpointVersion {
    pub version: u64,
    pub role: &'static str,
    pub names: Vec<String>,
    pub parents: Vec<u64>,
}

/// Diagnostic observations for one graph node. GPU indices cover only nested
/// traces resolved during this node. Shared trace indices are separately
/// resolved at graph completion. Both ranges are end-exclusive indices into gpu.
#[derive(Debug, serde::Serialize)]
pub struct NodeObservation {
    pub graph_id: u64,
    pub ordinal: usize,
    pub operation: &'static str,
    pub outputs: Vec<NodeOutput>,
    pub executed: bool,
    /// Operation encoding and any nested work. Excludes publication and retirement.
    pub host_milliseconds: Option<f64>,
    /// Host timeline relative to this profile, including work between GPU passes.
    pub started_ms: f64,
    pub publication_ms: Option<f64>,
    /// Existing completion wait, absent when this node did not wait.
    pub completion_wait_ms: Option<f64>,
    pub retirement_ms: Option<f64>,
    /// Whole node elapsed time. Includes the phases above and progress callbacks.
    pub total_ms: Option<f64>,
    pub activity: Option<GpuActivity>,
    pub nested_gpu_start: usize,
    pub nested_gpu_end: usize,
    pub shared_gpu_start: Option<usize>,
    pub shared_gpu_end: Option<usize>,
    pub memory_before: BoundaryMemory,
    pub memory_after_publication: BoundaryMemory,
    pub memory_after_retirement: BoundaryMemory,
    /// Host time collecting resource and boundary observations, including the pre-node scan.
    /// Excludes trace emission and graph metadata collection.
    pub diagnostic_overhead_ms: f64,
}

#[derive(Default, Debug, serde::Serialize)]
pub struct Profile {
    pub mode: Mode,
    #[serde(skip)]
    origin: Option<Instant>,
    pub host: Vec<Timing>,
    pub gpu: Vec<Timing>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<NodeObservation>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub graphs: Vec<GraphObservation>,
    pub scratch_allocations: u32,
    pub scratch_reuses: u32,
}

impl Profile {
    /// Retained observation heaps remain owned even after collection is disabled.
    /// The containing owner accounts for the inline Profile value.
    pub(crate) fn account(&self, retained: &mut crate::recipe::RetainedBytes) {
        retained.add_bytes(
            self.host.capacity() * size_of::<Timing>()
                + self.gpu.capacity() * size_of::<Timing>()
                + self.nodes.capacity() * size_of::<NodeObservation>()
                + self.graphs.capacity() * size_of::<GraphObservation>(),
        );
        for node in &self.nodes {
            retained.add_bytes(node.outputs.capacity() * size_of::<NodeOutput>());
            for output in &node.outputs {
                retained.add_bytes(output.name.capacity());
                if let Some(description) = output
                    .resource
                    .as_ref()
                    .and_then(|r| r.description.as_ref())
                {
                    retained
                        .add_bytes(description.layout.capacity() + description.space.capacity());
                }
            }
        }
        for graph in &self.graphs {
            retained.add_bytes(
                graph.requested_roots.capacity() * size_of::<usize>()
                    + graph.outputs.capacity() * size_of::<GraphOutput>(),
            );
            for output in &graph.outputs {
                retained.add_bytes(
                    output.name.capacity()
                        + output.data_type.capacity()
                        + output.dependencies.capacity() * size_of::<usize>(),
                );
            }
        }
    }

    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            origin: mode.enabled().then(Instant::now),
            ..Self::default()
        }
    }

    pub(crate) fn elapsed_ms(&self) -> f64 {
        self.origin
            .map_or(0., |start| start.elapsed().as_secs_f64() * 1000.)
    }

    pub(crate) fn record(&mut self, stage: &'static str, start: Instant) {
        crate::diagnostics::interval_since(crate::diagnostics::Category::Cpu, stage, start, |_| {});
        if self.mode.enabled() {
            self.host.push(Timing::elapsed(stage, start));
        }
    }

    pub(crate) fn record_duration(&mut self, stage: &'static str, duration: std::time::Duration) {
        if self.mode.enabled() {
            self.host.push(Timing {
                stage,
                milliseconds: duration.as_secs_f64() * 1000.,
            });
        }
    }

    pub fn measure<T>(&mut self, stage: &'static str, work: impl FnOnce() -> T) -> T {
        let _scope = crate::diagnostics::scope(crate::diagnostics::Category::Cpu, stage);
        if !self.mode.enabled() {
            return work();
        }
        let start = Instant::now();
        let value = work();
        self.host.push(Timing::elapsed(stage, start));
        value
    }
}
