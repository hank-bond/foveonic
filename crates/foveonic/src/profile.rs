//! Collects host/GPU timings and engine-owned transfer, submission, and allocation counts.

use std::time::Instant;

/// Cumulative engine-owned GPU activity; allocation bytes are totals, not peak residency.
/// Does not include wgpu's internal staging or activity submitted by another queue user.
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
    pub enabled: bool,
    pub host: Vec<Timing>,
    pub gpu: Vec<Timing>,
    pub scratch_allocations: u32,
    pub scratch_reuses: u32,
}

impl Profile {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::default()
        }
    }

    pub(crate) fn record(&mut self, stage: &'static str, start: Instant) {
        if self.enabled {
            self.host.push(Timing::elapsed(stage, start));
        }
    }

    pub fn measure<T>(&mut self, stage: &'static str, work: impl FnOnce() -> T) -> T {
        let start = Instant::now();
        let value = work();
        self.record(stage, start);
        value
    }
}
