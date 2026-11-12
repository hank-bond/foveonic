//! Collects render timings, activity counts and replayable diagnostic settings.

use std::time::Instant;

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
    pub name: String,
    pub required: bool,
    pub cached_available: bool,
    /// A missing required output requested from this operation.
    pub requested_for_execution: bool,
    /// A new value was published into the executor for this output.
    pub published: bool,
}

/// Diagnostic observations for one graph node. GPU indices cover only nested
/// traces resolved during this node. Shared trace indices are separately
/// resolved at graph completion. Both ranges are end-exclusive indices into gpu.
#[derive(Debug, serde::Serialize)]
pub struct NodeObservation {
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
    pub scratch_allocations: u32,
    pub scratch_reuses: u32,
}

impl Profile {
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
