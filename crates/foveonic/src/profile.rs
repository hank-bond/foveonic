//! Collects lightweight host wall timings and optional GPU timestamp measurements.

use std::time::Instant;

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
