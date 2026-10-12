//! Owns the GPU device, compute kernels, buffer uploads, and export readback.

use crate::profile::{Profile, Timing};
use crate::source::Result;
use wgpu::util::DeviceExt;

#[derive(Default)]
pub(crate) struct GpuTrace {
    query: Option<wgpu::QuerySet>,
    labels: Vec<&'static str>,
}

impl GpuTrace {
    fn writes(&mut self, kernel: Kernel) -> Option<wgpu::ComputePassTimestampWrites<'_>> {
        let query = self.query.as_ref()?;
        let index = self.labels.len() as u32 * 2;
        self.labels.push(kernel.label());
        Some(wgpu::ComputePassTimestampWrites {
            query_set: query,
            beginning_of_pass_write_index: Some(index),
            end_of_pass_write_index: Some(index + 1),
        })
    }
}

#[derive(Clone, Copy)]
pub enum Kernel {
    Histogram,
    BlackMean,
    Calibration,
    Color,
    Tone,
    Encode,
    Repair,
    Sharpen,
    ColorDq,
    View,
    Scopes,
    NoiseTile,
    NoiseBlock,
    NoiseRadius,
    NoiseBilateral,
    NoiseEdge,
    NoiseMean,
    NoiseLuminance,
    NoiseDiffuse,
    NoiseOsf,
    NoiseRatio,
    NoiseMix,
    NoiseRestore,
    NoiseSignal,
    NoiseStitch,
    SpeckleDetect,
    SpecklePrefix,
    SpeckleScatter,
    SpeckleRepair,
    CameraRepair,
    ClusterRepair,
    HighlightFlags,
    HighlightMap,
    HighlightDensity,
    HighlightDonors,
    HighlightPrefix,
    HighlightScatter,
    HighlightTable,
    HighlightApply,
    BandingFit,
    BandingResidual,
    BandingQuality,
    BandingMeans,
    BandingReduce,
    BandingApply,
    ColumnHorizontal,
    ColumnDeltas,
    ColumnApply,
    PatternMask,
    PatternCounts,
    PatternPrefix,
    PatternHorizontal,
    PatternVertical,
    PatternBlend,
    PatternMaskImage,
    SharpSignal,
    Resample,
    NoiseGlobalTile,
    NoisePatternSeed,
    NoiseGlobalScalarTile,
    HighlightInterpolation,
    HighlightBinHistogram,
    HighlightBinPrefix,
    HighlightBinScatter,
    CameraDetect,
    CameraPrefix,
    CameraScatter,
    NoiseMedian,
}

impl Kernel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Histogram => "black histogram",
            Self::BlackMean => "black mean",
            Self::Calibration => "calibration",
            Self::Color => "color matrix",
            Self::Tone => "tone",
            Self::Encode => "encode",
            Self::Repair => "repair",
            Self::Sharpen => "sharpen",
            Self::ColorDq => "ColorDQ",
            Self::View => "view texture",
            Self::Scopes => "current buffer scopes",
            Self::NoiseTile => "noise tile context",
            Self::NoiseBlock => "noise block means",
            Self::NoiseRadius => "adaptive noise radius",
            Self::NoiseBilateral => "noise bilateral base",
            Self::NoiseEdge => "noise edge weights",
            Self::NoiseMean => "directional local mean",
            Self::NoiseLuminance => "reconstruction luminance",
            Self::NoiseDiffuse => "luminance diffusion",
            Self::NoiseOsf => "luminance extrema conditioning",
            Self::NoiseRatio => "detail ratio",
            Self::NoiseMix => "residual detail mixing",
            Self::NoiseRestore => "detail reconstruction",
            Self::NoiseSignal => "signed detail signal",
            Self::NoiseStitch => "processing tile projection",
            Self::SpeckleDetect => "speckle detection",
            Self::SpecklePrefix => "speckle bank prefixes",
            Self::SpeckleScatter => "speckle record compaction",
            Self::SpeckleRepair => "ordered speckle correction",
            Self::CameraRepair => "complete camera record repair",
            Self::ClusterRepair => "masked camera cluster matching",
            Self::HighlightFlags => "connected original saturation flags",
            Self::HighlightMap => "clipped highlight neighborhood map",
            Self::HighlightDensity => "highlight neighborhood density",
            Self::HighlightDonors => "highlight donor selection",
            Self::HighlightPrefix => "highlight donor prefixes",
            Self::HighlightScatter => "ordered highlight donor compaction",
            Self::HighlightTable => "highlight blue correction table",
            Self::HighlightApply => "highlight restoration",
            Self::BandingFit => "row smooth-surface fits",
            Self::BandingResidual => "row patch residuals",
            Self::BandingQuality => "row patch correlation rejection",
            Self::BandingMeans => "row patch mean contributions",
            Self::BandingReduce => "ordered row bias reduction",
            Self::BandingApply => "row bias correction",
            Self::ColumnHorizontal => "column horizontal bilateral",
            Self::ColumnDeltas => "column block means",
            Self::ColumnApply => "column conditioning",
            Self::PatternMask => "pattern minmax mask",
            Self::PatternCounts => "pattern line counts",
            Self::PatternPrefix => "ordered pattern prefixes",
            Self::PatternHorizontal => "pattern horizontal swaps",
            Self::PatternVertical => "pattern vertical swaps",
            Self::PatternBlend => "pattern ratio blend",
            Self::PatternMaskImage => "pattern mask image",
            Self::SharpSignal => "native mono high-pass",
            Self::Resample => "native polyphase resampling",
            Self::NoiseGlobalTile => "global reduced tile context",
            Self::NoisePatternSeed => "native transposed pattern seed",
            Self::NoiseGlobalScalarTile => "global reduced edge context",
            Self::HighlightInterpolation => "highlight table interpolation",
            Self::HighlightBinHistogram => "highlight donor bin counts",
            Self::HighlightBinPrefix => "highlight donor bin offsets",
            Self::HighlightBinScatter => "stable highlight donor bin grouping",
            Self::CameraDetect => "parallel original-signal bad-red detection",
            Self::CameraPrefix => "stable capped bad-red prefixes",
            Self::CameraScatter => "ordered bad-red records and mask overrides",
            Self::NoiseMedian => "reusable noise cross medians",
        }
    }
}

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pipelines: Vec<wgpu::ComputePipeline>,
    pub adapter_name: String,
    activity: std::sync::Mutex<crate::profile::GpuActivity>,
}

impl Gpu {
    pub fn new() -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok_or("no compatible GPU adapter")?;
        let adapter_name = adapter.get_info().name;
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Foveonic image engine"),
                required_limits: adapter.limits(),
                required_features: adapter.features() & wgpu::Features::TIMESTAMP_QUERY,
                ..Default::default()
            },
            None,
        ))?;
        Self::from_device(device, queue, adapter_name)
    }

    pub fn from_device(
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter_name: String,
    ) -> Result<Self> {
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let kernels = [
            (include_str!("shaders/black.wgsl"), "histogram_pass"),
            (include_str!("shaders/black.wgsl"), "mean_pass"),
            (include_str!("shaders/calibrate.wgsl"), "main"),
            (include_str!("shaders/color.wgsl"), "main"),
            (include_str!("shaders/tone.wgsl"), "main"),
            (include_str!("shaders/encode.wgsl"), "main"),
            (include_str!("shaders/repair.wgsl"), "main"),
            (include_str!("shaders/sharpen.wgsl"), "main"),
            (include_str!("shaders/color_dq.wgsl"), "main"),
            (include_str!("shaders/view.wgsl"), "main"),
            (include_str!("shaders/scopes.wgsl"), "main"),
            (include_str!("shaders/noise_base.wgsl"), "tile_input"),
            (include_str!("shaders/noise_base.wgsl"), "block_mean"),
            (include_str!("shaders/noise_base.wgsl"), "radius_map"),
            (include_str!("shaders/noise_base.wgsl"), "bilateral"),
            (include_str!("shaders/noise_base.wgsl"), "edge_map"),
            (include_str!("shaders/noise_base.wgsl"), "local_mean"),
            (include_str!("shaders/noise_detail.wgsl"), "luminance"),
            (include_str!("shaders/noise_detail.wgsl"), "diffuse"),
            (include_str!("shaders/noise_detail.wgsl"), "osf"),
            (include_str!("shaders/noise_detail.wgsl"), "ratio"),
            (include_str!("shaders/noise_detail.wgsl"), "residual_mix"),
            (include_str!("shaders/noise_detail.wgsl"), "restore"),
            (include_str!("shaders/noise_detail.wgsl"), "signal"),
            (include_str!("shaders/noise_stitch.wgsl"), "stitch"),
            (include_str!("shaders/despeckle.wgsl"), "detect"),
            (include_str!("shaders/despeckle.wgsl"), "prefix"),
            (include_str!("shaders/despeckle.wgsl"), "scatter"),
            (include_str!("shaders/despeckle.wgsl"), "repair"),
            (include_str!("shaders/camera_repair.wgsl"), "main"),
            (include_str!("shaders/cluster_repair.wgsl"), "main"),
            (include_str!("shaders/highlight.wgsl"), "connected_flags"),
            (include_str!("shaders/highlight.wgsl"), "neighborhood_map"),
            (include_str!("shaders/highlight.wgsl"), "density_field"),
            (include_str!("shaders/highlight.wgsl"), "donor_marks"),
            (include_str!("shaders/highlight.wgsl"), "donor_prefix"),
            (include_str!("shaders/highlight.wgsl"), "donor_scatter"),
            (include_str!("shaders/highlight.wgsl"), "correction_table"),
            (include_str!("shaders/highlight.wgsl"), "restore"),
            (include_str!("shaders/banding.wgsl"), "fit"),
            (include_str!("shaders/banding.wgsl"), "residual_field"),
            (include_str!("shaders/banding.wgsl"), "quality"),
            (include_str!("shaders/banding.wgsl"), "row_means"),
            (include_str!("shaders/banding.wgsl"), "reduce_rows"),
            (include_str!("shaders/banding.wgsl"), "apply"),
            (include_str!("shaders/noise_column.wgsl"), "horizontal"),
            (include_str!("shaders/noise_column.wgsl"), "block_deltas"),
            (include_str!("shaders/noise_column.wgsl"), "correct"),
            (include_str!("shaders/pattern.wgsl"), "mask"),
            (include_str!("shaders/pattern.wgsl"), "line_counts"),
            (include_str!("shaders/pattern.wgsl"), "line_prefix"),
            (include_str!("shaders/pattern.wgsl"), "horizontal"),
            (include_str!("shaders/pattern.wgsl"), "vertical"),
            (include_str!("shaders/pattern.wgsl"), "blend"),
            (include_str!("shaders/pattern.wgsl"), "mask_image"),
            (include_str!("shaders/sharpen.wgsl"), "signal"),
            (include_str!("shaders/resample.wgsl"), "main"),
            (include_str!("shaders/noise_base.wgsl"), "global_tile"),
            (include_str!("shaders/noise_base.wgsl"), "pattern_seed"),
            (
                include_str!("shaders/noise_base.wgsl"),
                "global_tile_scalar",
            ),
            (
                include_str!("shaders/highlight.wgsl"),
                "table_interpolation",
            ),
            (include_str!("shaders/highlight.wgsl"), "bin_histogram"),
            (include_str!("shaders/highlight.wgsl"), "bin_prefix"),
            (include_str!("shaders/highlight.wgsl"), "bin_scatter"),
            (include_str!("shaders/camera_repair.wgsl"), "detect_records"),
            (
                include_str!("shaders/camera_repair.wgsl"),
                "detection_prefix",
            ),
            (
                include_str!("shaders/camera_repair.wgsl"),
                "scatter_detections",
            ),
            (include_str!("shaders/noise_base.wgsl"), "cross_medians"),
        ];
        let pipelines = kernels
            .iter()
            .enumerate()
            .map(|(index, &(code, entry))| {
                let code: std::borrow::Cow<'_, str> = if [
                    Kernel::Encode as usize,
                    Kernel::View as usize,
                    Kernel::Scopes as usize,
                ]
                .contains(&index)
                {
                    format!("{}\n{code}", include_str!("shaders/sample.wgsl")).into()
                } else if (Kernel::NoiseTile as usize..=Kernel::NoiseStitch as usize)
                    .contains(&index)
                    || (Kernel::ColumnHorizontal as usize..=Kernel::ColumnApply as usize)
                        .contains(&index)
                    || (Kernel::NoiseGlobalTile as usize..=Kernel::NoiseGlobalScalarTile as usize)
                        .contains(&index)
                    || index == Kernel::NoiseMedian as usize
                {
                    format!("{}\n{code}", include_str!("shaders/noise_common.wgsl")).into()
                } else if [
                    Kernel::Repair as usize,
                    Kernel::CameraRepair as usize,
                    Kernel::CameraDetect as usize,
                    Kernel::CameraPrefix as usize,
                    Kernel::CameraScatter as usize,
                ]
                .contains(&index)
                {
                    format!("{}\n{code}", include_str!("shaders/camera_common.wgsl")).into()
                } else {
                    code.into()
                };
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(entry),
                    source: wgpu::ShaderSource::Wgsl(code),
                });
                device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(entry),
                    layout: None,
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    cache: None,
                })
            })
            .collect();
        if let Some(error) = pollster::block_on(device.pop_error_scope()) {
            return Err(error.into());
        }
        Ok(Self {
            device,
            queue,
            pipelines,
            adapter_name,
            activity: Default::default(),
        })
    }

    pub fn activity(&self) -> crate::profile::GpuActivity {
        *self.activity.lock().expect("GPU activity counter")
    }

    fn submit(&self, encoder: wgpu::CommandEncoder) {
        self.activity
            .lock()
            .expect("GPU activity counter")
            .submissions += 1;
        self.queue.submit(Some(encoder.finish()));
    }

    pub fn upload<T: bytemuck::Pod>(&self, label: &str, values: &[T]) -> Result<wgpu::Buffer> {
        self.check_size(std::mem::size_of_val(values) as u64)?;
        {
            let mut activity = self.activity.lock().expect("GPU activity counter");
            activity.upload_bytes += std::mem::size_of_val(values) as u64;
            activity.allocations += 1;
        }
        Ok(self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(values),
                // Uploaded image inputs can later enter the execution-local
                // scratch pool, whose next owner may initialize by GPU copy.
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
            }))
    }

    pub fn uniform<T: bytemuck::Pod>(&self, value: &T) -> wgpu::Buffer {
        {
            let mut activity = self.activity.lock().expect("GPU activity counter");
            activity.uniform_bytes += std::mem::size_of::<T>() as u64;
            activity.allocations += 1;
        }
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Node parameters"),
                contents: bytemuck::bytes_of(value),
                usage: wgpu::BufferUsages::UNIFORM,
            })
    }

    pub fn storage(&self, label: &str, size: u64) -> Result<wgpu::Buffer> {
        self.check_size(size)?;
        {
            let mut activity = self.activity.lock().expect("GPU activity counter");
            activity.storage_bytes += size;
            activity.allocations += 1;
        }
        Ok(self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }))
    }

    pub fn copy(&self, source: &wgpu::Buffer, target: &wgpu::Buffer, size: u64) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_buffer_to_buffer(source, 0, target, 0, size);
        self.activity
            .lock()
            .expect("GPU activity counter")
            .device_copy_bytes += size;
        self.submit(encoder);
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
        GpuTrace {
            query: (enabled
                && self
                    .device
                    .features()
                    .contains(wgpu::Features::TIMESTAMP_QUERY))
            .then(|| {
                self.device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("Stage timestamps"),
                    ty: wgpu::QueryType::Timestamp,
                    count: maximum_passes * 2,
                })
            }),
            labels: Vec::new(),
        }
    }

    pub fn finish_trace(&self, trace: &GpuTrace, profile: &mut Profile) -> Result<()> {
        if trace.labels.is_empty() {
            return Ok(());
        }
        let query = trace.query.as_ref().expect("timestamps have a query set");
        let count = trace.labels.len() as u32 * 2;
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
        let bytes = self.readback(&resolved)?;
        let timestamps = bytemuck::cast_slice::<u8, u64>(&bytes);
        let period = f64::from(self.queue.get_timestamp_period());
        for (&stage, pair) in trace.labels.iter().zip(timestamps.as_chunks::<2>().0) {
            profile.gpu.push(Timing {
                stage,
                milliseconds: pair[1].wrapping_sub(pair[0]) as f64 * period / 1_000_000.,
            });
        }
        Ok(())
    }

    pub fn dispatch(
        &self,
        kernel: Kernel,
        buffers: &[(u32, &wgpu::Buffer)],
        groups: [u32; 2],
        trace: &mut GpuTrace,
    ) {
        let entries: Vec<_> = buffers
            .iter()
            .map(|(binding, buffer)| wgpu::BindGroupEntry {
                binding: *binding,
                resource: buffer.as_entire_binding(),
            })
            .collect();
        self.dispatch_entries(kernel, &entries, groups, trace);
    }

    pub fn view(
        &self,
        input: &wgpu::Buffer,
        params: &crate::calibration::EncodeParams,
        texture: &wgpu::TextureView,
        trace: &mut GpuTrace,
    ) {
        let uniform = self.uniform(params);
        self.dispatch_entries(
            Kernel::View,
            &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
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
        self.activity
            .lock()
            .expect("GPU activity counter")
            .dispatches += 1;
        let pipeline = &self.pipelines[kernel as usize];
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
        self.submit(encoder);
    }

    pub fn readback(&self, source: &wgpu::Buffer) -> Result<Vec<u8>> {
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
        encoder.copy_buffer_to_buffer(source, 0, &staging, 0, source.size());
        self.submit(encoder);
        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()??;
        let data = slice.get_mapped_range().to_vec();
        staging.unmap();
        Ok(data)
    }
}
