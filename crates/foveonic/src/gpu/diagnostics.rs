//! Resolves bounded capture-only GPU timestamps asynchronously on the device's normal polling path.

use super::{Gpu, GpuTrace, TracePass};
use crate::diagnostics::{self as diag, Category, Context, GpuInterval};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    Arc,
};

// A normal Quattro capture can enqueue 23 independent traces before the next
// application poll. Keep room for that burst without adding a profiling poll.
// Metal permits 32 counter sample buffers, one per timestamp query set:
// https://developer.apple.com/metal/limits/
// At 4096 timestamps per trace, explicit timestamp slots plus resolve/readback
// payload are bounded to 32 * 4096 * 8 * 3 = 3 MiB.
// Driver object overhead and bounded scalar pass metadata are additional.
const MAX_PENDING: usize = 32;
static NEXT_CLOCK: AtomicU64 = AtomicU64::new(1);

/// One identity and query budget for a device/queue and all its executor clones.
pub(super) struct DeviceDiagnostics {
    pub clock: u64,
    submission: AtomicU64,
    query: AtomicU64,
    pending: AtomicUsize,
}
impl Default for DeviceDiagnostics {
    fn default() -> Self {
        Self {
            clock: NEXT_CLOCK.fetch_add(1, Ordering::Relaxed),
            submission: AtomicU64::new(1),
            query: AtomicU64::new(1),
            pending: AtomicUsize::new(0),
        }
    }
}
impl DeviceDiagnostics {
    pub fn submission(&self) -> u64 {
        if diag::enabled() {
            self.submission.fetch_add(1, Ordering::Relaxed)
        } else {
            0
        }
    }
    // fetch_update remains available on the workspace minimum Rust version.
    #[allow(deprecated)]
    pub fn capture(self: &Arc<Self>, passes: u32, supported: bool) -> Option<Arc<CaptureTrace>> {
        if !diag::active_mode().gpu() || passes == 0 {
            return None;
        }
        let capture = diag::capture_id()?;
        let id = self.query.fetch_add(1, Ordering::Relaxed);
        let reason = if !supported {
            Some("timestamps-unsupported")
        } else if passes > wgpu::QUERY_SET_MAX_QUERIES / 2 {
            Some("query-capacity")
        } else if self
            .pending
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |pending| {
                (pending < MAX_PENDING).then_some(pending + 1)
            })
            .is_err()
        {
            Some("pending-budget")
        } else {
            None
        };
        if let Some(reason) = reason {
            diag::event_in_capture(capture, Category::Gpu, "gpu.query.dropped", |fields| {
                fields
                    .u64("capture_id", capture)
                    .u64("query", id)
                    .u64("clock_id", self.clock)
                    .str("reason", reason)
                    .u64("maximum_passes", passes as u64);
            });
            return None;
        }
        let trace = Arc::new(CaptureTrace {
            state: self.clone(),
            capture,
            id,
            context: diag::current_context(),
            complete: AtomicBool::new(false),
        });
        trace.event("gpu.query.started", |fields| {
            fields.u64("maximum_passes", passes as u64);
        });
        trace.pending();
        Some(trace)
    }
}

pub(super) struct CaptureTrace {
    state: Arc<DeviceDiagnostics>,
    capture: u64,
    id: u64,
    context: Option<Context>,
    complete: AtomicBool,
}
impl CaptureTrace {
    fn current(&self) -> bool {
        diag::capture_id() == Some(self.capture)
    }
    fn event(&self, name: &str, fields: impl FnOnce(&mut diag::Fields<'_>)) {
        if self.current() {
            diag::with_context(self.context, || {
                diag::event_in_capture(self.capture, Category::Gpu, name, |values| {
                    values
                        .u64("capture_id", self.capture)
                        .u64("query", self.id)
                        .u64("clock_id", self.state.clock);
                    fields(values);
                })
            });
        }
    }
    fn pending(&self) {
        self.event("gpu.query.pending", |fields| {
            fields
                .u64("pending", self.state.pending.load(Ordering::Acquire) as u64)
                .u64("limit", MAX_PENDING as u64);
        });
    }
    pub fn resolved(&self, passes: usize) {
        if !self.complete.swap(true, Ordering::AcqRel) {
            self.event("gpu.query.resolved", |fields| {
                fields.u64("passes", passes as u64);
            });
        }
    }
    fn failed(&self, reason: &str) {
        if !self.complete.swap(true, Ordering::AcqRel) {
            self.event("gpu.query.failed", |fields| {
                fields.str("reason", reason);
            });
        }
    }
}
impl Drop for CaptureTrace {
    fn drop(&mut self) {
        if !self.complete.load(Ordering::Acquire) {
            self.event("gpu.query.dropped", |fields| {
                fields.str("reason", "unfinalized");
            });
        }
        self.state.pending.fetch_sub(1, Ordering::AcqRel);
        self.pending();
    }
}
impl Drop for GpuTrace {
    fn drop(&mut self) {
        if let Some(capture) = &self.capture {
            if self.omitted > 0 {
                capture.event("gpu.query.dropped", |fields| {
                    fields
                        .str("reason", "pass-capacity")
                        .u64("passes", self.omitted as u64);
                });
            }
            if self.passes.is_empty() {
                capture.resolved(0);
            }
        }
    }
}

fn publish(capture: &CaptureTrace, passes: &[TracePass], ticks: &[u64], period: f64) {
    for (pass, pair) in passes.iter().zip(ticks.as_chunks::<2>().0) {
        diag::with_context(pass.context, || {
            diag::gpu_interval(
                pass.label,
                &GpuInterval {
                    capture_id: capture.capture,
                    submission: pass.submission,
                    clock_id: capture.state.clock,
                    start_tick: pair[0],
                    end_tick: pair[1],
                    nanoseconds_per_tick: period,
                },
                |fields| {
                    fields.u64("query", capture.id);
                },
            );
        });
    }
    capture.resolved(passes.len());
}
pub(super) fn publish_trace(trace: &GpuTrace, ticks: &[u64], period: f64) {
    if let Some(capture) = &trace.capture {
        publish(capture, &trace.passes, ticks, period);
    }
}

impl Gpu {
    /// This resolve/copy submission adds no gate wait or device poll. The map
    /// callback runs only when the application normally polls this device.
    pub(crate) fn finish_capture_trace(&self, trace: &GpuTrace) -> crate::source::Result<()> {
        if trace.finished.replace(true) {
            return Ok(());
        }
        let Some(capture) = trace.capture.as_ref().cloned() else {
            return Ok(());
        };
        if trace.passes.is_empty() {
            capture.resolved(0);
            return Ok(());
        }
        if !capture.current() {
            capture.complete.store(true, Ordering::Release);
            return Ok(());
        }
        let query = trace.query.as_ref().unwrap().clone();
        let passes = trace.passes.clone();
        let count = passes.len() as u32 * 2;
        let size = u64::from(count) * 8;
        let resolved = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Capture timestamp resolve"),
            size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Capture timestamp readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Capture timestamp resolve"),
            });
        encoder.resolve_query_set(&query, 0..count, &resolved, 0);
        encoder.copy_buffer_to_buffer(&resolved, 0, &readback, 0, size);
        {
            let mut activity = self.activity.lock().expect("GPU activity counter");
            activity.timestamp_resolves += 1;
            activity.readbacks += 1;
            activity.readback_bytes += size;
            activity.device_copy_bytes += size;
            activity.allocations += 2;
        }
        let submission = self.submit_buffer(encoder, true);
        capture.event("gpu.query.resolve_submitted", |fields| {
            fields
                .u64("submission", submission)
                .u64("bytes", size)
                .bool("diagnostic", true);
        });
        let period = f64::from(self.queue.get_timestamp_period());
        let callback_buffer = readback.clone();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _query = query;
                if let Err(error) = result {
                    capture.failed(&error.to_string());
                    return;
                }
                {
                    let bytes = callback_buffer.slice(..).get_mapped_range();
                    // Read only tiny timestamp pairs. No image/source data or
                    // borrowed mapped range escapes this callback.
                    let ticks: Vec<_> = bytes
                        .as_chunks::<8>()
                        .0
                        .iter()
                        .map(|bytes| u64::from_ne_bytes(*bytes))
                        .collect();
                    publish(&capture, &passes, &ticks, period);
                }
                callback_buffer.unmap();
            });
        Ok(())
    }
}

#[cfg(test)]
#[path = "diagnostics_test.rs"]
mod tests;
