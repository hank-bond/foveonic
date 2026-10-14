//! Collects host/GPU timings and engine-owned transfer, submission, and allocation counts.

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

#[derive(Default, Debug, serde::Serialize)]
pub struct Profile {
    pub mode: Mode,
    pub host: Vec<Timing>,
    pub gpu: Vec<Timing>,
    pub scratch_allocations: u32,
    pub scratch_reuses: u32,
}

impl Profile {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            ..Self::default()
        }
    }

    pub(crate) fn record(&mut self, stage: &'static str, start: Instant) {
        if self.mode.enabled() {
            self.host.push(Timing::elapsed(stage, start));
        }
    }

    pub fn measure<T>(&mut self, stage: &'static str, work: impl FnOnce() -> T) -> T {
        if !self.mode.enabled() {
            return work();
        }
        let start = Instant::now();
        let value = work();
        self.record(stage, start);
        value
    }
}
