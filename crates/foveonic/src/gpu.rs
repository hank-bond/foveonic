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
    Blur,
    Sharpen,
    ColorDq,
    View,
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
            Self::Blur => "blur",
            Self::Sharpen => "sharpen",
            Self::ColorDq => "ColorDQ",
            Self::View => "view texture",
        }
    }
}

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pipelines: [wgpu::ComputePipeline; 11],
    pub adapter_name: String,
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
            (include_str!("shaders/blur.wgsl"), "main"),
            (include_str!("shaders/sharpen.wgsl"), "main"),
            (include_str!("shaders/color_dq.wgsl"), "main"),
            (include_str!("shaders/view.wgsl"), "main"),
        ];
        let pipelines = std::array::from_fn(|index| {
            let (code, entry) = kernels[index];
            let code: std::borrow::Cow<'_, str> =
                if index == Kernel::Encode as usize || index == Kernel::View as usize {
                    format!("{}\n{code}", include_str!("shaders/sample.wgsl")).into()
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
        });
        if let Some(error) = pollster::block_on(device.pop_error_scope()) {
            return Err(error.into());
        }
        Ok(Self {
            device,
            queue,
            pipelines,
            adapter_name,
        })
    }

    pub fn upload<T: bytemuck::Pod>(&self, label: &str, values: &[T]) -> Result<wgpu::Buffer> {
        self.check_size(std::mem::size_of_val(values) as u64)?;
        Ok(self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(values),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            }))
    }

    pub fn uniform<T: bytemuck::Pod>(&self, value: &T) -> wgpu::Buffer {
        self.device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Node parameters"),
                contents: bytemuck::bytes_of(value),
                usage: wgpu::BufferUsages::UNIFORM,
            })
    }

    pub fn storage(&self, label: &str, size: u64) -> Result<wgpu::Buffer> {
        self.check_size(size)?;
        Ok(self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        }))
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
        self.queue.submit(Some(encoder.finish()));
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
        self.queue.submit(Some(encoder.finish()));
    }

    pub fn readback(&self, source: &wgpu::Buffer) -> Result<Vec<u8>> {
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
        self.queue.submit(Some(encoder.finish()));
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
