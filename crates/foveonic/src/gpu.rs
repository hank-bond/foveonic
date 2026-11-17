//! Owns the GPU device, compute kernels, buffer uploads, and export readback.

use crate::profile::{Profile, Timing};
use crate::source::Result;
use wgpu::util::DeviceExt;

mod completion;
mod diagnostics;
mod field;
mod kernels;
mod scheduling;
pub(crate) mod scratch;
mod storage;
pub use kernels::Kernel;
#[cfg(feature = "wide-experiment")]
pub(crate) use scheduling::SubmissionWindow;
pub(crate) use storage::{Binding, ReadGuard, WriteGuard};

pub(crate) struct Retirement {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl Retirement {
    pub(crate) fn wait(&self) -> Result<()> {
        completion::submitted(&self.device, &self.queue)
    }
}

#[derive(Clone)]
struct TracePass {
    label: &'static str,
    submission: u64,
    context: Option<crate::diagnostics::Context>,
}

#[derive(Default)]
pub(crate) struct GpuTrace {
    query: Option<wgpu::QuerySet>,
    passes: Vec<TracePass>,
    capture: Option<std::sync::Arc<diagnostics::CaptureTrace>>,
    finished: std::cell::Cell<bool>,
    legacy: bool,
    omitted: u32,
    maximum_passes: u32,
}

impl GpuTrace {
    /// Timestamp owners have separate device/readback lifetimes and no audited
    /// allocation contract. An ordinary trace owns only its pass-vector heap.
    pub(crate) fn account(&self, retained: &mut crate::recipe::RetainedBytes) -> Option<()> {
        if self.query.is_some() || self.capture.is_some() {
            return None;
        }
        retained.add_bytes(self.passes.capacity() * size_of::<TracePass>());
        Some(())
    }

    pub(crate) fn len(&self) -> usize {
        self.passes.len()
    }

    fn writes(&mut self, kernel: Kernel) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        let query = self.query.as_ref()?;
        let index = self.passes.len() as u32 * 2;
        if !self.legacy && self.passes.len() >= self.maximum_passes as usize {
            self.omitted += 1;
            return None;
        }
        self.passes.push(TracePass {
            label: kernel.label(),
            submission: 0,
            context: crate::diagnostics::current_context(),
        });
        Some(wgpu::ComputePassTimestampWrites {
            query_set: query,
            beginning_of_pass_write_index: Some(index),
            end_of_pass_write_index: Some(index + 1),
        })
    }

    fn submitted(&mut self, first: usize, submission: u64) {
        for pass in &mut self.passes[first..] {
            pass.submission = submission;
        }
    }
}

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pipelines: Vec<wgpu::ComputePipeline>,
    field: std::sync::Arc<std::sync::OnceLock<std::result::Result<field::FieldKernels, String>>>,
    #[cfg(test)]
    shader_writes: Vec<u32>,
    pub adapter_name: String,
    activity: std::sync::Mutex<crate::profile::GpuActivity>,
    diagnostics: std::sync::Arc<diagnostics::DeviceDiagnostics>,
}

impl Gpu {
    pub(crate) fn retirement(&self) -> Retirement {
        Retirement {
            device: self.device.clone(),
            queue: self.queue.clone(),
        }
    }
    pub fn executor(&self) -> Self {
        Self {
            device: self.device.clone(),
            queue: self.queue.clone(),
            pipelines: self.pipelines.clone(),
            field: self.field.clone(),
            #[cfg(test)]
            shader_writes: self.shader_writes.clone(),
            adapter_name: self.adapter_name.clone(),
            activity: Default::default(),
            diagnostics: self.diagnostics.clone(),
        }
    }
    pub fn new() -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .map_err(|error| format!("no compatible GPU adapter: {error}"))?;
        let adapter_name = adapter.get_info().name;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("Foveonic image engine"),
                required_limits: adapter.limits(),
                required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
                ..Default::default()
            }))?;
        Self::from_device(device, queue, adapter_name)
    }

    pub fn activity(&self) -> crate::profile::GpuActivity {
        *self.activity.lock().expect("GPU activity counter")
    }

    /// Complete the current queue prefix without holding wgpu locks while idle.
    /// This parks the calling worker and must not run on the UI thread.
    pub fn wait_for_submitted_work(&self) -> Result<()> {
        let _wait = crate::diagnostics::scope_with(
            crate::diagnostics::Category::Gpu,
            "gpu.wait",
            |fields| {
                fields
                    .str("reason", "submitted-work")
                    .bool("blocking_poll", false);
            },
        );
        completion::submitted(&self.device, &self.queue)
    }

    fn submit(&self, encoder: wgpu::CommandEncoder) {
        self.submit_buffer(encoder, false);
    }

    fn submit_buffer(&self, encoder: wgpu::CommandEncoder, diagnostic: bool) -> u64 {
        // Import renders inherit the cache's foreground gate. Never queue an
        // entire batch ahead of interactive work on the shared device queue.
        if !diagnostic {
            crate::background::checkpoint();
        }
        let id = self.diagnostics.submission();
        let command = {
            let _scope =
                crate::diagnostics::scope(crate::diagnostics::Category::Gpu, "gpu.encode.finish");
            encoder.finish()
        };
        self.activity
            .lock()
            .expect("GPU activity counter")
            .submissions += 1;
        {
            let _scope = crate::diagnostics::scope_with(
                crate::diagnostics::Category::Gpu,
                "gpu.submit",
                |fields| {
                    fields
                        .u64("submission", id)
                        .u64("clock_id", self.diagnostics.clock)
                        .bool("diagnostic", diagnostic);
                },
            );
            self.queue.submit(Some(command));
        };
        if !diagnostic
            && crate::background::context().is_some()
            && !scheduling::stage_controls(self)
        {
            let _scope = crate::diagnostics::scope_with(
                crate::diagnostics::Category::Gpu,
                "gpu.wait",
                |fields| {
                    fields
                        .u64("submission", id)
                        .str("reason", "existing-foreground-gate")
                        .bool("blocking_poll", false);
                },
            );
            completion::submitted(&self.device, &self.queue).expect("GPU submission completion");
        }
        id
    }

    pub fn upload<T: bytemuck::Pod>(&self, label: &str, values: &[T]) -> Result<ReadGuard> {
        let _scope = crate::diagnostics::scope_with(
            crate::diagnostics::Category::Gpu,
            "gpu.upload",
            |fields| {
                fields
                    .str("label", label)
                    .u64("bytes", std::mem::size_of_val(values) as u64);
            },
        );
        crate::background::checkpoint();
        self.check_size(std::mem::size_of_val(values) as u64)?;
        {
            let mut activity = self.activity.lock().expect("GPU activity counter");
            activity.upload_bytes += std::mem::size_of_val(values) as u64;
            activity.allocations += 1;
        }
        Ok(ReadGuard::new(
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: bytemuck::cast_slice(values),
                    // A later scratch-buffer user may initialize this allocation
                    // by GPU copy, so uploaded images also need copy-destination usage.
                    usage: wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_SRC
                        | wgpu::BufferUsages::COPY_DST,
                }),
            label,
        ))
    }

    pub fn uniform<T: bytemuck::Pod>(&self, value: &T) -> ReadGuard {
        let _scope = crate::diagnostics::scope_with(
            crate::diagnostics::Category::Gpu,
            "gpu.uniform",
            |fields| {
                fields.u64("bytes", std::mem::size_of::<T>() as u64);
            },
        );
        {
            let mut activity = self.activity.lock().expect("GPU activity counter");
            activity.uniform_bytes += std::mem::size_of::<T>() as u64;
            activity.allocations += 1;
        }
        ReadGuard::new(
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Node parameters"),
                    contents: bytemuck::bytes_of(value),
                    usage: wgpu::BufferUsages::UNIFORM,
                }),
            "Node parameters",
        )
    }

    pub fn storage(&self, label: &str, size: u64) -> Result<ReadGuard> {
        let _scope = crate::diagnostics::scope_with(
            crate::diagnostics::Category::Gpu,
            "gpu.storage",
            |fields| {
                fields.str("label", label).u64("bytes", size);
            },
        );
        self.check_size(size)?;
        {
            let mut activity = self.activity.lock().expect("GPU activity counter");
            activity.storage_bytes += size;
            activity.allocations += 1;
        }
        Ok(ReadGuard::new(
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            label,
        ))
    }

    pub fn copy(&self, source: &ReadGuard, mut target: WriteGuard<'_>, size: u64) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_buffer_to_buffer(source.raw(), 0, target.raw(), 0, size);
        self.activity
            .lock()
            .expect("GPU activity counter")
            .device_copy_bytes += size;
        self.submit(encoder);
        target.submitted("copy");
    }

    /// Copy already validated byte ranges without exposing unguarded buffers.
    pub fn copy_ranges(
        &self,
        source: &ReadGuard,
        mut target: WriteGuard<'_>,
        ranges: impl IntoIterator<Item = [u64; 3]>,
    ) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let mut total = 0;
        for [from, to, bytes] in ranges {
            encoder.copy_buffer_to_buffer(source.raw(), from, target.raw(), to, bytes);
            total += bytes;
        }
        self.activity
            .lock()
            .expect("GPU activity counter")
            .device_copy_bytes += total;
        self.submit(encoder);
        target.submitted("copy region");
    }

    fn check_size(&self, size: u64) -> Result<()> {
        let limits = self.device.limits();
        if size == 0
            || size > limits.max_buffer_size
            || size > u64::from(limits.max_storage_buffer_binding_size)
        {
            return Err(format!(
                "GPU storage buffer requires {size} bytes, above the supported limit"
            )
            .into());
        }
        Ok(())
    }

    pub fn trace(&self, enabled: bool, maximum_passes: u32) -> GpuTrace {
        if !enabled && !crate::diagnostics::active_mode().gpu() {
            return GpuTrace::default();
        }
        let supported = self
            .device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY);
        let capture = self.diagnostics.capture(maximum_passes, supported);
        GpuTrace {
            query: ((enabled || capture.is_some()) && supported && maximum_passes > 0).then(|| {
                self.device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("Stage timestamps"),
                    ty: wgpu::QueryType::Timestamp,
                    count: maximum_passes * 2,
                })
            }),
            passes: Vec::new(),
            capture,
            finished: Default::default(),
            legacy: enabled,
            omitted: 0,
            maximum_passes,
        }
    }

    pub fn finish_trace(&self, trace: &GpuTrace, profile: &mut Profile) -> Result<()> {
        if !profile.mode.gpu() {
            return self.finish_capture_trace(trace);
        }
        if trace.finished.replace(true) {
            return Ok(());
        }
        if trace.passes.is_empty() {
            if let Some(capture) = &trace.capture {
                capture.resolved(0);
            }
            return Ok(());
        }
        let query = trace.query.as_ref().expect("timestamps have a query set");
        let count = trace.passes.len() as u32 * 2;
        let resolved = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Resolved timestamps"),
            size: u64::from(count) * 8,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.resolve_query_set(query, 0..count, &resolved, 0);
        self.activity
            .lock()
            .expect("GPU activity counter")
            .timestamp_resolves += 1;
        self.submit(encoder);
        let bytes = self.readback(&ReadGuard::new(resolved, "Resolved timestamps"))?;
        let timestamps = bytemuck::cast_slice::<u8, u64>(&bytes);
        let period = f64::from(self.queue.get_timestamp_period());
        for (pass, pair) in trace.passes.iter().zip(timestamps.as_chunks::<2>().0) {
            profile.gpu.push(Timing {
                stage: pass.label,
                milliseconds: pair[1].wrapping_sub(pair[0]) as f64 * period / 1_000_000.,
            });
        }
        diagnostics::publish_trace(trace, timestamps, period);
        Ok(())
    }

    pub fn dispatch(
        &self,
        kernel: Kernel,
        buffers: &mut [(u32, Binding<'_>)],
        groups: [u32; 2],
        trace: &mut GpuTrace,
    ) {
        #[cfg(test)]
        for (slot, binding) in buffers.iter() {
            assert_eq!(
                matches!(binding, Binding::Write(_)),
                self.write_mask(kernel) & (1 << slot) != 0,
                "incorrect buffer guard for {} binding {}",
                kernel.label(),
                slot
            );
        }
        let entries: Vec<_> = buffers
            .iter()
            .map(|(binding, buffer)| wgpu::BindGroupEntry {
                binding: *binding,
                resource: buffer.raw().as_entire_binding(),
            })
            .collect();
        self.dispatch_entries(kernel, &entries, groups, trace);
        // Keep every guard borrowed until submission has put the commands in
        // queue order. A later writer must not get ahead of these reads.
        for (_, binding) in buffers {
            binding.submitted(kernel.label());
        }
    }

    /// Bind the actual resources used by a color-field entry point. Reflection
    /// keeps the many solver passes' access guards tied to their WGSL accesses.
    pub(crate) fn field_dispatch(
        &self,
        kernel: Kernel,
        resources: &mut [(u32, ReadGuard)],
        groups: u32,
        trace: &mut GpuTrace,
    ) {
        let (used, writes) =
            self.field_data().bindings[kernel as usize - Kernel::FieldPrepare as usize];
        let mut bindings: Vec<_> = resources
            .iter_mut()
            .filter(|(slot, _)| used & (1 << *slot) != 0)
            .map(|(slot, buffer)| {
                (
                    *slot,
                    if writes & (1 << *slot) != 0 {
                        Binding::Write(buffer.write())
                    } else {
                        Binding::Read(buffer)
                    },
                )
            })
            .collect();
        assert_eq!(
            bindings.len(),
            used.count_ones() as usize,
            "missing color-field binding"
        );
        self.dispatch(kernel, &mut bindings, [groups.max(1), 1], trace);
    }

    /// Encode a dependent color-field pass sequence under one exclusive resource
    /// borrow and submit it once. Dispatch order preserves dependent writes.
    /// No writer or retained reader can escape before the batch is submitted.
    pub(crate) fn field_batch(
        &self,
        tasks: &[(Kernel, u32)],
        resources: &mut [(u32, ReadGuard)],
        trace: &mut GpuTrace,
    ) {
        let _scope =
            crate::diagnostics::scope(crate::diagnostics::Category::Gpu, "gpu.encode.field_batch");
        let first = trace.len();
        let access = |kernel: Kernel| {
            self.field_data().bindings[kernel as usize - Kernel::FieldPrepare as usize]
        };
        let (used, writes) = tasks.iter().fold((0, 0), |(u, w), (kernel, _)| {
            let (ku, kw) = access(*kernel);
            (u | ku, w | kw)
        });
        // Borrow once for the whole submission. Repeated kernels use the same
        // bind group, and no write guard is released before queue submission.
        let mut bindings: Vec<_> = resources
            .iter_mut()
            .filter(|(slot, _)| used & (1 << *slot) != 0)
            .map(|(slot, buffer)| {
                (
                    *slot,
                    if writes & (1 << *slot) != 0 {
                        Binding::Write(buffer.write())
                    } else {
                        Binding::Read(buffer)
                    },
                )
            })
            .collect();
        assert_eq!(bindings.len(), used.count_ones() as usize);
        let mut groups: Vec<(usize, wgpu::BindGroup)> = Vec::new();
        for &(kernel, _) in tasks {
            if groups.iter().any(|(key, _)| *key == kernel as usize) {
                continue;
            }
            let (used, _) = access(kernel);
            let entries: Vec<_> = bindings
                .iter()
                .filter(|(slot, _)| used & (1 << *slot) != 0)
                .map(|(slot, buffer)| wgpu::BindGroupEntry {
                    binding: *slot,
                    resource: buffer.raw().as_entire_binding(),
                })
                .collect();
            let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(kernel.label()),
                layout: &self.pipeline(kernel).get_bind_group_layout(0),
                entries: &entries,
            });
            groups.push((kernel as usize, group));
        }
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Color-field batch"),
            });
        let group_for = |kernel: Kernel| {
            &groups
                .iter()
                .find(|(key, _)| *key == kernel as usize)
                .unwrap()
                .1
        };
        if trace.query.is_none() {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Color-field batch"),
                timestamp_writes: None,
            });
            for &(kernel, count) in tasks {
                pass.set_pipeline(self.pipeline(kernel));
                pass.set_bind_group(0, group_for(kernel), &[]);
                pass.dispatch_workgroups(count.max(1), 1, 1);
            }
        } else {
            // Per-stage diagnostic timestamps need pass boundaries. They are
            // deliberately separate samples, not normal completed render times.
            for &(kernel, count) in tasks {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some(kernel.label()),
                    timestamp_writes: trace.writes(kernel),
                });
                pass.set_pipeline(self.pipeline(kernel));
                pass.set_bind_group(0, group_for(kernel), &[]);
                pass.dispatch_workgroups(count.max(1), 1, 1);
            }
        }
        self.activity
            .lock()
            .expect("GPU activity counter")
            .dispatches += tasks.len() as u64;
        let submission = self.submit_buffer(encoder, false);
        trace.submitted(first, submission);
        for (_, binding) in &mut bindings {
            binding.submitted("color-field batch");
        }
    }

    pub fn view(
        &self,
        input: &ReadGuard,
        params: &crate::nodes::sensor::calibration::EncodeParams,
        texture: &wgpu::TextureView,
        trace: &mut GpuTrace,
    ) {
        let uniform = self.uniform(params);
        self.dispatch_entries(
            Kernel::View,
            &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.raw().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.raw().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(texture),
                },
            ],
            [params.output[0].div_ceil(8), params.output[1].div_ceil(8)],
            trace,
        );
    }

    fn dispatch_entries(
        &self,
        kernel: Kernel,
        entries: &[wgpu::BindGroupEntry<'_>],
        groups: [u32; 2],
        trace: &mut GpuTrace,
    ) {
        let _scope = crate::diagnostics::scope_with(
            crate::diagnostics::Category::Gpu,
            "gpu.encode.dispatch",
            |fields| {
                fields
                    .str("kernel", kernel.label())
                    .u64("groups_x", groups[0] as u64)
                    .u64("groups_y", groups[1] as u64);
            },
        );
        let first = trace.len();
        self.activity
            .lock()
            .expect("GPU activity counter")
            .dispatches += 1;
        let pipeline = self.pipeline(kernel);
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Node inputs"),
            layout: &pipeline.get_bind_group_layout(0),
            entries,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Image node"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Image node"),
                timestamp_writes: trace.writes(kernel),
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(groups[0], groups[1], 1);
        }
        let submission = self.submit_buffer(encoder, false);
        trace.submitted(first, submission);
    }

    #[cfg(test)]
    pub fn readback_texture(
        &self,
        texture: &wgpu::Texture,
        extent: [u32; 2],
        bytes_per_row: u32,
    ) -> Result<Vec<u8>> {
        let mut target = self.storage(
            "Texture test readback",
            u64::from(bytes_per_row) * u64::from(extent[1]),
        )?;
        let mut guard = target.write();
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: guard.raw(),
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(extent[1]),
                },
            },
            wgpu::Extent3d {
                width: extent[0],
                height: extent[1],
                depth_or_array_layers: 1,
            },
        );
        self.submit(encoder);
        guard.submitted("texture test readback");
        self.readback(&target)
    }

    pub fn readback(&self, source: &ReadGuard) -> Result<Vec<u8>> {
        let _scope = crate::diagnostics::scope_with(
            crate::diagnostics::Category::Gpu,
            "gpu.readback",
            |fields| {
                fields.u64("bytes", source.size());
            },
        );
        {
            let mut activity = self.activity.lock().expect("GPU activity counter");
            activity.readbacks += 1;
            activity.readback_bytes += source.size();
        }
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Export readback"),
            size: source.size(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Export readback"),
            });
        encoder.copy_buffer_to_buffer(source.raw(), 0, &staging, 0, source.size());
        self.submit(encoder);
        map_readback(&self.device, &staging)?;
        let slice = staging.slice(..);
        let data = slice.get_mapped_range().to_vec();
        staging.unmap();
        Ok(data)
    }
}

/// Map a submitted staging buffer for CPU reading. The caller unmaps it after use.
/// Shared nonblocking polling keeps completion waits outside device locks.
/// Background readback retains its existing priority and interruption policy.
pub fn map_readback(device: &wgpu::Device, buffer: &wgpu::Buffer) -> Result<()> {
    let background = crate::background::context().is_some();
    let _wait =
        crate::diagnostics::scope_with(crate::diagnostics::Category::Gpu, "gpu.wait", |fields| {
            fields
                .str("reason", "readback")
                .bool("blocking_poll", false);
        });
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    completion::wait(device, rx, || {
        if background {
            crate::background::checkpoint_or_cancel()?;
        }
        Ok(())
    })??;
    Ok(())
}

#[cfg(test)]
mod scheduling_tests {
    use super::*;
    use std::{
        sync::{mpsc, Arc},
        time::Duration,
    };

    #[test]
    #[ignore = "requires a working GPU adapter"]
    fn background_readback_yields_to_foreground_and_preserves_samples() {
        let gpu = Gpu::new().unwrap();
        let expected = [0x12345678_u32, 0xfedcba98, 7, 0];
        let staging = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Cooperative readback fixture"),
                contents: bytemuck::cast_slice(&expected),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            });
        let gate = Arc::new(crate::background::Gate::default());
        let active = gate.foreground();
        let background_gate = gate.clone();
        let device = gpu.device.clone();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            crate::background::run(Some(background_gate), || {
                entered_tx.send(()).unwrap();
                map_readback(&device, &staging).unwrap();
                let actual = staging.slice(..).get_mapped_range().to_vec();
                staging.unmap();
                done_tx.send(actual).unwrap();
            })
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
        let input = gpu.upload("Foreground fixture", &expected).unwrap();
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<u32, u8>(&expected)
        );
        drop(active);
        assert_eq!(
            done_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            bytemuck::cast_slice::<u32, u8>(&expected)
        );
        thread.join().unwrap();
    }

    #[test]
    #[ignore = "requires a GPU"]
    fn background_executor_yields_submissions_and_keeps_activity_separate() {
        let foreground = Gpu::new().unwrap();
        let background = foreground.executor();
        let gate = Arc::new(crate::background::Gate::default());
        let (entered_tx, entered_rx) = mpsc::channel();
        let (advance_tx, advance_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let background_gate = gate.clone();
        let thread = std::thread::spawn(move || {
            crate::background::run(Some(background_gate), || {
                background.submit(
                    background
                        .device
                        .create_command_encoder(&Default::default()),
                );
                entered_tx.send(()).unwrap();
                advance_rx.recv().unwrap();
                background.submit(
                    background
                        .device
                        .create_command_encoder(&Default::default()),
                );
                done_tx.send(background.activity()).unwrap();
            })
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // Interrupt an executor that has already submitted work, rather than
        // only holding a job before its first render begins.
        let active = gate.foreground();
        advance_tx.send(()).unwrap();
        assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
        foreground.submit(
            foreground
                .device
                .create_command_encoder(&Default::default()),
        );
        foreground
            .device
            .poll(wgpu::PollType::Wait)
            .expect("GPU polling failed");
        assert_eq!(foreground.activity().submissions, 1);
        drop(active);
        assert_eq!(
            done_rx
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .submissions,
            2
        );
        assert_eq!(foreground.activity().submissions, 1);
        thread.join().unwrap();
    }
}

#[cfg(test)]
#[path = "../examples/chroma_investigation/gpu.rs"]
pub(crate) mod chroma_investigation;

#[cfg(test)]
mod accounting_tests {
    use super::*;

    #[test]
    fn ordinary_trace_counts_reserved_pass_capacity_without_a_device() {
        let mut trace = GpuTrace::default();
        trace.passes.reserve_exact(3);
        trace.passes.push(TracePass {
            label: "example",
            submission: 1,
            context: Some(Default::default()),
        });
        let expected = trace.passes.capacity() * size_of::<TracePass>();
        for clear in [false, true] {
            if clear {
                trace.passes.clear();
            }
            let mut retained = crate::recipe::RetainedBytes::default();
            assert_eq!(trace.account(&mut retained), Some(()));
            assert_eq!(retained.bytes(), expected);
        }
    }
}
