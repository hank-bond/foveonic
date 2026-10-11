//! Builds typed processing and export recipes around owned GPU images and execution-local scratch.

use crate::calibration::{self, ColorSpace};
use crate::gpu::{Gpu, Kernel};
use crate::graph::{DataType, Graph, Operation};
use crate::image::{Description, ImageTexture, Layout, RenderedImage, Space, View};
use crate::profile::Profile;
use crate::scratch::Scratch;
use crate::source::{Metadata, Result, Source};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

pub use crate::tone::Controls as ToneControls;

#[derive(Clone, Debug)]
pub struct Settings {
    pub white_balance: String,
    pub color_mode: String,
    /// Stops applied to linear RGB before the camera-derived tone curve.
    pub exposure: f32,
    pub tone: ToneControls,
    /// RGB multipliers in the file's working color space, before tone.
    pub channel_gains: [f32; 3],
    /// A factor after tone: zero is grayscale, one leaves saturation unchanged.
    pub saturation: f32,
    /// Multiplies camera ColorDQ amplitudes after camera color conversion; zero omits conditioning.
    pub color_dq_gain: f32,
    /// Select isolated camera-record repairs with at least this badness (0..15).
    pub repair_badness: Option<u32>,
    /// Unsharp-mask strength; zero omits sharpening.
    pub sharpen_amount: f32,
    /// Gaussian sigma in full-sensor pixels, before output resizing.
    pub sharpen_radius: f32,
    /// Exact final oriented dimensions; None keeps the active crop's native size.
    pub output_size: Option<[u32; 2]>,
}

impl Settings {
    pub fn new(white_balance: impl Into<String>, color_mode: impl Into<String>) -> Self {
        Self {
            white_balance: white_balance.into(),
            color_mode: color_mode.into(),
            exposure: 0.,
            tone: ToneControls::default(),
            channel_gains: [1.; 3],
            saturation: 1.,
            color_dq_gain: 0.,
            repair_badness: None,
            sharpen_amount: 0.,
            sharpen_radius: 1.,
            output_size: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.white_balance.is_empty() || self.color_mode.is_empty() {
            return Err("--wb and --mode are required".into());
        }
        if !self.exposure.is_finite()
            || !self.exposure.exp2().is_finite()
            || self.exposure.exp2() <= 0.
        {
            return Err("invalid exposure".into());
        }
        if [self.tone.contrast, self.tone.shadows, self.tone.highlights]
            .iter()
            .any(|v| !(-2. ..=2.).contains(v))
        {
            return Err("contrast, shadows, and highlights must be in -2..2".into());
        }
        if self
            .channel_gains
            .iter()
            .chain([&self.saturation, &self.sharpen_amount])
            .any(|v| !v.is_finite() || *v < 0.)
        {
            return Err(
                "channel gains, saturation, and sharpening must be finite and nonnegative".into(),
            );
        }
        if !self.color_dq_gain.is_finite() || self.color_dq_gain < 0. {
            return Err("ColorDQ gain must be finite and nonnegative".into());
        }
        if !self.sharpen_radius.is_finite()
            || self.sharpen_radius <= 0.
            || self.sharpen_radius > 16.
        {
            return Err("sharpen radius must be in 0..16 pixels, excluding zero".into());
        }
        if self.repair_badness.is_some_and(|v| v > 15) {
            return Err("repair badness must be in 0..15".into());
        }
        if self.output_size.is_some_and(|size| size.contains(&0)) {
            return Err("output dimensions must be nonzero".into());
        }
        Ok(())
    }
}

fn gaussian_weights(sigma: f32) -> Vec<f32> {
    let radius = (sigma * 3.).ceil() as i32;
    let mut weights: Vec<_> = (-radius..=radius)
        .map(|x| (-0.5 * (x as f32 / sigma).powi(2)).exp())
        .collect();
    let sum: f32 = weights.iter().sum();
    for weight in &mut weights {
        *weight /= sum;
    }
    weights
}

fn gains(dimensions: [u32; 4], values: [f32; 3]) -> calibration::ColorParams {
    calibration::ColorParams {
        source: dimensions,
        matrix: std::array::from_fn(|row| {
            std::array::from_fn(|col| if row == col { values[row] } else { 0. })
        }),
    }
}

fn saturation(dimensions: [u32; 4], factor: f32) -> calibration::ColorParams {
    // Reference-style brightness weights; do not clamp RGB until output encoding.
    let luma = [0.299, 0.587, 0.114];
    calibration::ColorParams {
        source: dimensions,
        matrix: std::array::from_fn(|row| {
            std::array::from_fn(|col| {
                if col == 3 {
                    0.
                } else {
                    (1. - factor) * luma[col] + if row == col { factor } else { 0. }
                }
            })
        }),
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ScalarParams {
    source: [u32; 4],
    value: [f32; 4],
}

enum Input<'a> {
    Source(&'a Source),
    Rendered(&'a RenderedImage),
}

impl Input<'_> {
    fn metadata(&self) -> &Metadata {
        match self {
            Self::Source(source) => &source.metadata,
            Self::Rendered(image) => image.source_metadata(),
        }
    }
}

struct Value {
    buffer: wgpu::Buffer,
    description: Option<Description>,
    recyclable: bool,
}

fn description(data: DataType, extent: [u32; 2]) -> Option<Description> {
    let (layout, space) = match data {
        DataType::SensorLayers => (Layout::Rgb32Float, Space::SensorLayers),
        DataType::CalibratedLayers => (Layout::Rgb32Float, Space::CalibratedLayers),
        DataType::LinearRgb(space) => (Layout::Rgb32Float, Space::LinearRgb(space)),
        DataType::ToneMappedRgb(space) => (Layout::Rgb32Float, Space::ToneMappedRgb(space)),
        DataType::EncodedRgb16(space) => (Layout::PackedRgb16, Space::EncodedRgb16(space)),
        DataType::BlackLevels | DataType::File => return None,
    };
    Some(Description {
        extent,
        layout,
        space,
    })
}

pub struct Engine {
    gpu: Gpu,
    owner: Arc<()>,
}

impl Engine {
    pub fn new() -> Result<Self> {
        Ok(Self {
            gpu: Gpu::new()?,
            owner: Arc::new(()),
        })
    }

    /// The queue must belong to this device. GUI callers select a surface-compatible adapter
    /// themselves and request limits sufficient for full-sensor Merrill storage bindings
    /// (roughly 184 MiB). Timestamp profiling requires TIMESTAMP_QUERY.
    pub fn from_device(
        device: wgpu::Device,
        queue: wgpu::Queue,
        adapter_name: String,
    ) -> Result<Self> {
        Ok(Self {
            gpu: Gpu::from_device(device, queue, adapter_name)?,
            owner: Arc::new(()),
        })
    }
    pub fn device(&self) -> &wgpu::Device {
        &self.gpu.device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.gpu.queue
    }
    pub fn adapter_name(&self) -> &str {
        &self.gpu.adapter_name
    }

    /// Returns owned full-sensor, tone-mapped floating RGB with a separate logical output view.
    /// No pixel readback or export is required. Disabled profiling does not wait for GPU completion.
    pub fn render(
        &self,
        source: &Source,
        settings: &Settings,
        profile: &mut Profile,
    ) -> Result<RenderedImage> {
        settings.validate()?;
        if profile.enabled {
            profile.host.extend_from_slice(source.load_timings());
        }
        let space = ColorSpace::from_metadata(&source.metadata)?;
        let mode = profile.measure("prepare color mode", || {
            crate::color_mode::prepare(&source.metadata, &settings.color_mode)
        })?;
        let calibration = profile.measure("prepare calibration", || {
            calibration::prepare(&source.metadata, &settings.white_balance)
        })?;
        let color = profile.measure("prepare color matrix", || {
            calibration::color(&source.metadata, &settings.white_balance, &mode)
        })?;
        let curve = profile.measure("prepare tone curve", || {
            crate::tone::prepare(&source.metadata, &mode, settings.tone)
        })?;
        let mut params = calibration::encode(&source.metadata, space)?;
        if let Some(size) = settings.output_size {
            params.output[..2].copy_from_slice(&size);
        }
        self.check_extent([params.output[0], params.output[1]])?;
        let dimensions = [
            source.metadata.width as u32,
            source.metadata.height as u32,
            0,
            0,
        ];
        let mut graph = Graph::default();
        let input = graph.add(Operation::Source, &[])?;
        let black = graph.add(Operation::Black(calibration.params), &[input])?;
        let mut calibrated = graph.add(Operation::Calibrate(calibration), &[input, black])?;
        if let Some(minimum) = settings.repair_badness {
            let flags = profile.measure("prepare repair map", || {
                crate::repair::flags(&source.metadata, minimum)
            })?;
            calibrated = graph.add(Operation::Repair { flags }, &[calibrated])?;
        }
        let mut linear = graph.add(
            Operation::Color {
                params: color,
                space,
            },
            &[calibrated],
        )?;
        if settings.color_dq_gain > 0. {
            let table = profile.measure("prepare ColorDQ", || {
                crate::color_dq::prepare(&source.metadata, settings.color_dq_gain)
            })?;
            linear = graph.add(Operation::ColorDq { table, space }, &[linear])?;
        }
        if settings.channel_gains != [1.; 3] {
            linear = graph.add(
                Operation::Gains {
                    params: gains(dimensions, settings.channel_gains),
                    space,
                },
                &[linear],
            )?;
        }
        let mut toned = graph.add(
            Operation::Tone {
                curve,
                exposure: settings.exposure.exp2(),
                space,
            },
            &[linear],
        )?;
        if settings.saturation != 1. {
            toned = graph.add(
                Operation::Saturation {
                    params: saturation(dimensions, settings.saturation),
                    space,
                },
                &[toned],
            )?;
        }
        if settings.sharpen_amount > 0. {
            let weights = gaussian_weights(settings.sharpen_radius);
            let horizontal = graph.add(
                Operation::Blur {
                    axis: 0,
                    weights: weights.clone(),
                    space,
                },
                &[toned],
            )?;
            let blurred = graph.add(
                Operation::Blur {
                    axis: 1,
                    weights,
                    space,
                },
                &[horizontal],
            )?;
            toned = graph.add(
                Operation::Sharpen {
                    amount: settings.sharpen_amount,
                    space,
                },
                &[toned, blurred],
            )?;
        }
        let value = self
            .execute(Input::Source(source), &graph, Some(toned), profile)?
            .expect("requested graph result");
        Ok(RenderedImage {
            buffer: value.buffer,
            description: value.description.expect("rendered image description"),
            view: View {
                crop: params.crop,
                rotation: params.source[2],
                extent: [params.output[0], params.output[1]],
            },
            metadata: source.metadata.clone(),
            settings: settings.clone(),
            owner: self.owner.clone(),
        })
    }

    /// Exports an image created by this engine, without rerunning its processing recipe.
    pub fn write_tiff(
        &self,
        image: &RenderedImage,
        path: &Path,
        profile: &mut Profile,
    ) -> Result<[u32; 2]> {
        self.check_owner(image)?;
        if path.exists() {
            return Err("output file already exists".into());
        }
        let params = image.params();
        let Space::ToneMappedRgb(space) = image.description.space else {
            return Err("TIFF requires tone-mapped RGB".into());
        };
        let mut graph = Graph::default();
        let input = graph.add(Operation::RenderedInput { space }, &[])?;
        let encoded = graph.add(Operation::Encode { params, space }, &[input])?;
        graph.add(
            Operation::Tiff {
                path: path.to_owned(),
                params,
                space,
            },
            &[encoded],
        )?;
        self.execute(Input::Rendered(image), &graph, None, profile)?;
        Ok(image.view.extent)
    }

    /// Materializes the logical view on GPU. Optional extent changes only presentation size,
    /// not the working image or export size. RGBA32Float is unfilterable without additional
    /// device features: presenters can use textureLoad/manual filtering. Color management is
    /// the presentation consumer's responsibility; this texture retains its working primaries.
    pub fn view_texture(
        &self,
        image: &RenderedImage,
        extent: Option<[u32; 2]>,
        profile: &mut Profile,
    ) -> Result<ImageTexture> {
        self.check_owner(image)?;
        let mut params = image.params();
        if let Some(size) = extent {
            params.output[..2].copy_from_slice(&size);
        }
        let size = [params.output[0], params.output[1]];
        self.check_extent(size)?;
        if size
            .iter()
            .any(|v| *v > self.device().limits().max_texture_dimension_2d)
        {
            return Err("view exceeds GPU texture dimensions".into());
        }
        let start = Instant::now();
        let texture = self.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("Floating image view"),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut trace = self.gpu.trace(profile.enabled, 1);
        self.gpu.view(
            &image.buffer,
            &params,
            &texture.create_view(&wgpu::TextureViewDescriptor::default()),
            &mut trace,
        );
        profile.record("submit view texture", start);
        self.gpu.finish_trace(&trace, profile)?;
        Ok(ImageTexture {
            texture,
            description: Description {
                extent: size,
                layout: Layout::Rgba32FloatTexture,
                space: image.description.space,
            },
        })
    }

    fn check_owner(&self, image: &RenderedImage) -> Result<()> {
        if !Arc::ptr_eq(&self.owner, &image.owner) {
            return Err("image belongs to another engine".into());
        }
        Ok(())
    }
    fn check_extent(&self, size: [u32; 2]) -> Result<()> {
        if size.contains(&0)
            || size.iter().any(|v| {
                v.div_ceil(8) > self.device().limits().max_compute_workgroups_per_dimension
            })
        {
            return Err("invalid or unsupported output dimensions".into());
        }
        Ok(())
    }

    fn execute(
        &self,
        context: Input<'_>,
        graph: &Graph,
        target: Option<usize>,
        profile: &mut Profile,
    ) -> Result<Option<Value>> {
        let gpu = &self.gpu;
        let mut trace = gpu.trace(profile.enabled, graph.nodes.len() as u32 * 2);
        let mut scratch = Scratch::default();
        let mut values: Vec<Option<Value>> = (0..graph.nodes.len()).map(|_| None).collect();
        let mut consumers = vec![0; graph.nodes.len()];
        for node in &graph.nodes {
            for &input in &node.inputs {
                consumers[input] += 1;
            }
        }
        // A retained root is an extra consumer. Its buffer never enters execution scratch.
        if let Some(target) = target {
            consumers[target] += 1;
        }
        for (index, node) in graph.nodes.iter().enumerate() {
            let start = Instant::now();
            let input = |slot: usize| {
                values[node.inputs[slot]]
                    .as_ref()
                    .expect("validated graph input is live")
            };
            let inherited = if node.inputs.is_empty() {
                match &context {
                    Input::Source(source) => {
                        [source.metadata.width as u32, source.metadata.height as u32]
                    }
                    Input::Rendered(image) => image.description.extent,
                }
            } else {
                input(0)
                    .description
                    .expect("image input description")
                    .extent
            };
            let extent = match &node.operation {
                Operation::Encode { params, .. } => [params.output[0], params.output[1]],
                _ => inherited,
            };
            let desc = description(node.output, extent);
            let groups = [inherited[0].div_ceil(8), inherited[1].div_ceil(8)];
            let dimensions = [inherited[0], inherited[1], 0, 0];
            let bytes = u64::from(inherited[0]) * u64::from(inherited[1]) * 12;
            let mut dispatch = |kernel, buffers: &[(u32, &wgpu::Buffer)], groups| {
                gpu.dispatch(kernel, buffers, groups, &mut trace)
            };
            let mut recyclable = desc.is_some_and(|d| d.layout == Layout::Rgb32Float);
            let output = match &node.operation {
                Operation::Source => {
                    let Input::Source(source) = &context else {
                        return Err("sensor input context required".into());
                    };
                    Some(gpu.upload("Original sensor layers", &source.pixels)?)
                }
                Operation::RenderedInput { .. } => {
                    let Input::Rendered(image) = &context else {
                        return Err("rendered input context required".into());
                    };
                    recyclable = false;
                    Some(image.buffer.clone())
                }
                Operation::Black(params) => {
                    let histogram = gpu.storage("Dark-shield histogram", 3 * 4096 * 4)?;
                    let black = gpu.storage("Black levels", 12)?;
                    let uniform = gpu.uniform(params);
                    dispatch(
                        Kernel::Histogram,
                        &[(0, &input(0).buffer), (1, &histogram), (2, &uniform)],
                        [params.source[2].div_ceil(64), 1],
                    );
                    dispatch(
                        Kernel::BlackMean,
                        &[(1, &histogram), (2, &uniform), (3, &black)],
                        [3, 1],
                    );
                    Some(black)
                }
                Operation::Calibrate(calibration) => {
                    let metadata = context.metadata();
                    if inherited != [metadata.width as u32, metadata.height as u32] {
                        return Err("calibration requires original sensor coordinates".into());
                    }
                    let output = scratch.take(gpu, bytes)?;
                    let grid = gpu.upload("Spatial gain grids", &calibration.grid_values)?;
                    let uniform = gpu.uniform(&calibration.params);
                    dispatch(
                        Kernel::Calibration,
                        &[
                            (0, &input(0).buffer),
                            (1, &input(1).buffer),
                            (2, &grid),
                            (3, &uniform),
                            (4, &output),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::Repair { flags } => {
                    let output = scratch.take(gpu, bytes)?;
                    let flags = gpu.upload("Camera repair map", flags)?;
                    let uniform = gpu.uniform(&dimensions);
                    dispatch(
                        Kernel::Repair,
                        &[
                            (0, &input(0).buffer),
                            (1, &flags),
                            (2, &uniform),
                            (3, &output),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::Color { params, .. }
                | Operation::Gains { params, .. }
                | Operation::Saturation { params, .. } => {
                    let output = scratch.take(gpu, bytes)?;
                    let mut params = *params;
                    params.source[..2].copy_from_slice(&inherited);
                    let uniform = gpu.uniform(&params);
                    dispatch(
                        Kernel::Color,
                        &[(0, &input(0).buffer), (1, &uniform), (2, &output)],
                        groups,
                    );
                    Some(output)
                }
                Operation::ColorDq { table, .. } => {
                    let output = scratch.take(gpu, bytes)?;
                    let lookup = gpu.upload("ColorDQ lookup", &table.values)?;
                    let uniform = gpu.uniform(&[inherited[0], inherited[1], table.period, 0]);
                    dispatch(
                        Kernel::ColorDq,
                        &[
                            (0, &input(0).buffer),
                            (1, &lookup),
                            (2, &uniform),
                            (3, &output),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::Tone {
                    curve, exposure, ..
                } => {
                    let output = scratch.take(gpu, bytes)?;
                    let curve = gpu.upload("Color-mode tone curve", curve)?;
                    let uniform = gpu.uniform(&ScalarParams {
                        source: dimensions,
                        value: [*exposure, 0., 0., 0.],
                    });
                    dispatch(
                        Kernel::Tone,
                        &[
                            (0, &input(0).buffer),
                            (1, &curve),
                            (2, &uniform),
                            (3, &output),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::Blur { axis, weights, .. } => {
                    let output = scratch.take(gpu, bytes)?;
                    let uniform = gpu.uniform(&[
                        inherited[0],
                        inherited[1],
                        *axis,
                        (weights.len() / 2) as u32,
                    ]);
                    let weights = gpu.upload("Gaussian weights", weights)?;
                    dispatch(
                        Kernel::Blur,
                        &[
                            (0, &input(0).buffer),
                            (1, &weights),
                            (2, &uniform),
                            (3, &output),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::Sharpen { amount, .. } => {
                    if input(0).description != input(1).description {
                        return Err("sharpen inputs have different image descriptions".into());
                    }
                    let output = scratch.take(gpu, bytes)?;
                    let uniform = gpu.uniform(&ScalarParams {
                        source: dimensions,
                        value: [*amount, 0., 0., 0.],
                    });
                    dispatch(
                        Kernel::Sharpen,
                        &[
                            (0, &input(0).buffer),
                            (1, &input(1).buffer),
                            (2, &uniform),
                            (3, &output),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::Encode { params, .. } => {
                    let output = gpu.storage(
                        "Encoded packed RGB16",
                        desc.expect("encoded description").byte_len(),
                    )?;
                    let uniform = gpu.uniform(params);
                    dispatch(
                        Kernel::Encode,
                        &[(0, &input(0).buffer), (1, &uniform), (2, &output)],
                        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
                    );
                    Some(output)
                }
                Operation::Tiff {
                    path,
                    params,
                    space,
                } => {
                    let bytes = profile
                        .measure("readback wait and copy", || gpu.readback(&input(0).buffer))?;
                    let file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(path)?;
                    crate::output::write_tiff(
                        std::io::BufWriter::new(file),
                        &bytes,
                        params,
                        *space,
                        profile,
                    )?;
                    None
                }
            };
            if !matches!(node.operation, Operation::Tiff { .. }) {
                profile.record(node.operation.label(), start);
            }
            values[index] = output.map(|buffer| Value {
                buffer,
                description: desc,
                recyclable,
            });
            for &id in &node.inputs {
                consumers[id] -= 1;
                if consumers[id] == 0 {
                    if let Some(value) = values[id].take() {
                        if value.recyclable {
                            scratch.release(value.buffer);
                        }
                    }
                }
            }
        }
        gpu.finish_trace(&trace, profile)?;
        if profile.enabled {
            profile.scratch_allocations += scratch.allocations;
            profile.scratch_reuses += scratch.reuses;
        }
        Ok(target.and_then(|id| values[id].take()))
    }
}

#[cfg(test)]
#[path = "render_test.rs"]
mod tests;
