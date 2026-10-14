//! Builds typed processing and export recipes around owned GPU images and execution-local scratch.

use crate::calibration::{self, ColorSpace};
use crate::gpu::Binding::{Read, Write};
use crate::gpu::{Gpu, Kernel};
use crate::graph::{DataType, Graph, Operation};
use crate::image::{Description, ImageTexture, Layout, RenderedImage, Space, View};
pub use crate::interactive::RenderSession;
use crate::interactive::{Execution, Superseded};
use crate::profile::Profile;
use crate::scratch::Scratch;
use crate::source::{Metadata, Result, Source};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

pub use crate::repair::Threshold as RepairThreshold;
pub use crate::tone::Controls as ToneControls;

/// Sharing the technical stages keeps DNG corrections in step with photo edits.
/// DNG leaves the appearance in metadata instead of applying it to pixels.
#[path = "dng/gpu.rs"]
mod dng_gpu;

enum Recipe<'a> {
    Photographic(&'a [&'a str]),
    DngBase,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub white_balance: String,
    pub color_mode: String,
    /// Stops applied to linear RGB before the camera-derived tone curve.
    pub exposure: f32,
    pub tone: ToneControls,
    /// Uniform multiplier for calibrated, black-corrected sensor layers before repair and color conversion.
    pub sensor_gain: f32,
    /// A factor after tone: zero is grayscale, one leaves saturation unchanged.
    pub saturation: f32,
    /// ColorDQ gain after camera color conversion. Zero skips this correction.
    /// None chooses the camera's automatic gain when reconstruction is on.
    /// Some keeps an explicit override, including zero for Off.
    pub color_dq_gain: Option<f32>,
    /// Ordered camera-record repair, using the current cutoff or an explicit minimum badness.
    pub repair: Option<RepairThreshold>,
    /// Detect and correct remaining channel extrema with reference-style despeckling.
    pub despeckle: bool,
    /// Restore clipped-layer color from original camera saturation maps, independently of tone highlights.
    pub highlight_restoration: bool,
    /// Camera threshold offset in stops. None selects Auto. Positive values delay neutralization.
    pub highlight_threshold_ev: Option<f32>,
    /// Estimate and subtract native additive per-row layer bias before reconstruction.
    pub row_correction: bool,
    /// Noise and detail settings. None leaves reconstruction off.
    pub reconstruction: Option<crate::noise::Controls>,
    /// Sharpening strength relative to brightness. Zero leaves sharpening off.
    pub sharpen_amount: f32,
    /// Final dimensions after orientation. None keeps the active crop's native size.
    pub output_size: Option<[u32; 2]>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            // Camera choices are resolved from the selected X3F, not a global preset.
            white_balance: String::new(),
            color_mode: String::new(),
            exposure: 0.,
            tone: ToneControls::default(),
            sensor_gain: 1.,
            saturation: 1.,
            color_dq_gain: None,
            repair: Some(RepairThreshold::Camera),
            despeckle: true,
            highlight_restoration: true,
            highlight_threshold_ev: None,
            row_correction: true,
            reconstruction: None,
            sharpen_amount: 0.,
            output_size: None,
        }
    }
}

impl Settings {
    pub fn new(white_balance: impl Into<String>, color_mode: impl Into<String>) -> Self {
        Self {
            white_balance: white_balance.into(),
            color_mode: color_mode.into(),
            ..Self::default()
        }
    }

    pub fn color_dq_selected(&self) -> bool {
        self.color_dq_gain
            .map_or(self.reconstruction.is_some(), |v| v > 0.)
    }

    pub fn validate(&self) -> Result<()> {
        if self.white_balance.is_empty() || self.color_mode.is_empty() {
            return Err("--wb and --mode are required".into());
        }
        self.validate_values()
    }

    /// Inspection can show sensor data before WB or mode is known. Check numbers
    /// here and let stages that use those camera choices resolve them later.
    pub(crate) fn validate_values(&self) -> Result<()> {
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
        if [self.sensor_gain, self.saturation, self.sharpen_amount]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.)
        {
            return Err(
                "sensor gain, saturation, and sharpening must be finite and nonnegative".into(),
            );
        }
        if self.color_dq_gain.is_some_and(|v| !v.is_finite() || v < 0.) {
            return Err("ColorDQ gain must be finite and nonnegative".into());
        }
        crate::highlight::validate_threshold(self.highlight_threshold_ev)?;
        if let Some(threshold) = self.repair {
            threshold.validate()?;
        }
        if let Some(controls) = self.reconstruction {
            controls.validate()?;
        }
        if self.output_size.is_some_and(|size| size.contains(&0)) {
            return Err("output dimensions must be nonzero".into());
        }
        Ok(())
    }
}

fn sensor_gain(dimensions: [u32; 4], factor: f32) -> calibration::ColorParams {
    calibration::ColorParams {
        source: dimensions,
        matrix: std::array::from_fn(|row| {
            std::array::from_fn(|col| if row == col { factor } else { 0. })
        }),
    }
}

fn saturation(dimensions: [u32; 4], factor: f32) -> calibration::ColorParams {
    // Reference-style brightness weights preserve signed RGB and headroom.
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

#[derive(Clone)]
pub(crate) struct Value {
    pub(crate) buffer: crate::gpu::ReadGuard,
    pub(crate) description: Option<Description>,
}

fn description(data: DataType, extent: [u32; 2]) -> Option<Description> {
    let (layout, space) = match data {
        DataType::SensorLayers => (Layout::Rgb32Float, Space::SensorLayers),
        DataType::ScalarField => (Layout::Scalar32Float, Space::ScalarField),
        DataType::LuminanceRatio => (Layout::Scalar32Float, Space::LuminanceRatio),
        DataType::SignedDetail => (Layout::Scalar32Float, Space::SignedDetail),
        DataType::CalibratedLayers => (Layout::Rgb32Float, Space::CalibratedLayers),
        DataType::LinearRgb(space) => (Layout::Rgb32Float, Space::LinearRgb(space)),
        DataType::ToneMappedRgb(space) => (Layout::Rgb32Float, Space::ToneMappedRgb(space)),
        DataType::EncodedRgb16(space) => (Layout::PackedRgb16, Space::EncodedRgb16(space)),
        DataType::BlackLevels
        | DataType::Histogram
        | DataType::RowTable
        | DataType::File
        | DataType::FilteredTiles
        | DataType::ConditionedTiles
        | DataType::PatternSeedTiles
        | DataType::UnconditionedRatioTiles
        | DataType::ResidualTiles
        | DataType::RatioTiles
        | DataType::ColorTiles(_) => return None,
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
    /// Cumulative engine-owned submissions/transfers/allocations, excluding other queue users.
    pub fn gpu_activity(&self) -> crate::profile::GpuActivity {
        self.gpu.activity()
    }
    /// Read stored samples for diagnostics, not for CPU photo processing.
    /// This transfer counts as engine activity, so keep it outside render timings.
    pub fn readback_image(&self, image: &RenderedImage) -> Result<Vec<u8>> {
        self.check_owner(image)?;
        self.gpu.readback(&image.buffer)
    }

    /// Analyze the retained buffer in its logical view.
    pub fn scopes(
        &self,
        image: &RenderedImage,
        profile: &mut Profile,
    ) -> Result<crate::scopes::Scopes> {
        profile.mode.check_gpu_features(self.device().features())?;
        self.check_owner(image)?;
        if !matches!(
            image.description.layout,
            Layout::Rgb32Float | Layout::Scalar32Float
        ) {
            return Err("scope analysis requires floating image storage".into());
        }
        crate::scopes::analyze(&self.gpu, image, profile)
    }

    pub fn adapter_name(&self) -> &str {
        &self.gpu.adapter_name
    }

    /// Returns owned full-sensor, tone-mapped floating RGB with a logical output view.
    /// Off and wall modes return after submission. GPU diagnostics wait for timestamps.
    pub fn render(
        &self,
        source: &Source,
        settings: &Settings,
        profile: &mut Profile,
    ) -> Result<RenderedImage> {
        settings.validate()?;
        let artifact = self
            .render_artifacts(source, settings, &["final"], profile)?
            .pop()
            .ok_or("missing final image")?;
        let crate::inspection::Content::Image(image) = artifact.content else {
            return Err("final output is not an image".into());
        };
        Ok(*image)
    }

    /// Request outputs by name so graph positions can change without changing callers.
    /// Only their dependencies run. Other intermediate outputs are not retained.
    pub fn render_artifacts(
        &self,
        source: &Source,
        settings: &Settings,
        names: &[&str],
        profile: &mut Profile,
    ) -> Result<Vec<crate::inspection::Artifact>> {
        self.render_recipe(
            source,
            settings,
            Recipe::Photographic(names),
            profile,
            &mut Execution::default(),
        )
    }

    /// Keep full quality during editing. None means a newer request took over safely,
    /// not that rendering failed.
    /// Checkpoints remain bound to this session's immutable source and this engine's device.
    pub fn render_session(
        &self,
        session: &mut RenderSession,
        settings: &Settings,
        names: &[&str],
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
    ) -> Result<Option<Vec<crate::inspection::Artifact>>> {
        session.cache.bind(&self.owner);
        session.last_settings = Some(settings.clone());
        let mut execution = Execution {
            cache: Some(&mut session.cache),
            cancel: Some(&cancel),
            checkpoints: Vec::new(),
        };
        let result = self.render_recipe(
            &session.source,
            settings,
            Recipe::Photographic(names),
            profile,
            &mut execution,
        );
        session.cache.finish();
        match result {
            Ok(artifacts) => Ok(Some(artifacts)),
            Err(error) if error.downcast_ref::<Superseded>().is_some() => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn render_dng_base(
        &self,
        source: &Source,
        settings: &Settings,
        profile: &mut Profile,
    ) -> Result<RenderedImage> {
        let artifact = self
            .render_recipe(
                source,
                settings,
                Recipe::DngBase,
                profile,
                &mut Execution::default(),
            )?
            .pop()
            .ok_or("missing DNG processing base")?;
        let crate::inspection::Content::Image(image) = artifact.content else {
            return Err("DNG processing base is not an image".into());
        };
        Ok(*image)
    }

    fn render_recipe(
        &self,
        source: &Source,
        settings: &Settings,
        recipe: Recipe<'_>,
        profile: &mut Profile,
        execution: &mut Execution<'_>,
    ) -> Result<Vec<crate::inspection::Artifact>> {
        let (names, dng_base) = match recipe {
            Recipe::Photographic(names) => (names, false),
            Recipe::DngBase => (&["final"][..], true),
        };
        execution.check()?;
        profile.mode.check_gpu_features(self.device().features())?;
        settings.validate_values()?;
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let space = ColorSpace::from_metadata(&source.metadata)?;
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
        macro_rules! finish {
            () => {
                execution.check()?;
                if !names.contains(&"final")
                    && names
                        .iter()
                        .all(|name| graph.outputs.iter().any(|o| o.name == *name))
                {
                    return self.collect_artifacts(
                        source, settings, &graph, input, names, params, profile, execution,
                    );
                }
            };
        }
        finish!();
        let mut highlight_params = None;
        if settings.highlight_restoration && names.contains(&"highlight.density") {
            let prepared = Arc::new(profile.measure("prepare original saturation", || {
                crate::highlight::prepare(
                    &source.metadata,
                    settings.sensor_gain,
                    settings.highlight_threshold_ev,
                )
            })?);
            graph.add(Operation::HighlightDensity(prepared.clone()), &[])?;
            highlight_params = Some(prepared);
            finish!();
        }
        let black_params =
            profile.measure("prepare black", || calibration::black(&source.metadata))?;
        let black = graph.add(Operation::Black(black_params), &[input])?;
        finish!();
        let calibration = profile.measure("prepare calibration", || {
            calibration::prepare(&source.metadata, &settings.white_balance)
        })?;
        let calibration_gains = std::array::from_fn(|c| calibration.params.gains[c]);
        let mut calibrated = graph.add(Operation::Calibrate(calibration), &[input, black])?;
        finish!();
        if settings.sensor_gain != 1. {
            calibrated = graph.add(
                Operation::SensorGain {
                    factor: settings.sensor_gain,
                },
                &[calibrated],
            )?;
            finish!();
        }
        if let Some(threshold) = settings.repair {
            let prepared = profile.measure("prepare repair records", || {
                crate::camera_repair::prepare(&source.metadata, threshold)
            })?;
            calibrated = graph.add(Operation::Repair(prepared), &[calibrated, input])?;
            finish!();
        }
        if settings.highlight_restoration {
            let prepared = match highlight_params {
                Some(prepared) => prepared,
                None => Arc::new(profile.measure("prepare highlight restoration", || {
                    crate::highlight::prepare(
                        &source.metadata,
                        settings.sensor_gain,
                        settings.highlight_threshold_ev,
                    )
                })?),
            };
            calibrated = graph.add(Operation::Highlights(prepared), &[calibrated])?;
            finish!();
        }
        let mut couplings = None;
        if settings.despeckle {
            let recipe = profile.measure("prepare noise couplings", || {
                crate::noise::couplings(
                    &source.metadata,
                    &settings.white_balance,
                    settings.reconstruction.unwrap_or_default(),
                )
            })?;
            couplings = Some(recipe);
            let prepared = profile.measure("prepare despeckle", || {
                crate::despeckle::prepare(
                    &source.metadata,
                    calibration_gains,
                    settings.sensor_gain,
                    recipe.despeckle_scale,
                )
            })?;
            calibrated = graph.add(Operation::Despeckle(prepared), &[calibrated])?;
            finish!();
        }
        if settings.row_correction {
            let prepared = profile.measure("prepare row correction", || {
                crate::banding::prepare(&source.metadata, calibration_gains, settings.sensor_gain)
            })?;
            calibrated = graph.add(Operation::Banding(prepared), &[calibrated])?;
            finish!();
        }
        let reconstruction = if let Some(controls) = settings.reconstruction {
            let params = profile.measure("prepare reference noise", || {
                crate::noise::prepare(
                    &source.metadata,
                    &settings.white_balance,
                    settings.sensor_gain,
                    controls,
                )
            })?;
            let base = graph.add(Operation::NoiseBase(params), &[calibrated])?;
            finish!();
            let column_params = profile.measure("prepare column cuts", || {
                crate::noise::column_parameters(&source.metadata, params)
            })?;
            let conditioned = graph.add(Operation::Column(column_params), &[calibrated])?;
            finish!();
            let ratio = graph.add(Operation::Ratio(params), &[conditioned, base])?;
            finish!();
            let prepared = profile.measure("prepare pattern recipe", || {
                crate::noise::pattern_parameters(&source.metadata, params, controls)
            })?;
            let pattern = graph.add(Operation::Pattern { params, prepared }, &[ratio, base + 3])?;
            finish!();
            let detail = graph.add(Operation::Detail(params), &[conditioned, base, pattern])?;
            finish!();
            Some((params, base, detail, pattern))
        } else {
            None
        };
        let mode = if dng_base {
            // Content corrections still need calibrated working RGB. Leaving out the
            // selected mode here prevents its look from becoming baked DNG data.
            crate::color_mode::Settings {
                tone_index: 0,
                contrast: 0.,
                compensation: None,
            }
        } else {
            profile.measure("prepare color mode", || {
                crate::color_mode::prepare(&source.metadata, &settings.color_mode)
            })?
        };
        let color = profile.measure("prepare color matrix", || {
            calibration::color(&source.metadata, &settings.white_balance, &mode)
        })?;
        let mut linear = if let Some((params, base, _, _)) = reconstruction {
            graph.add(
                Operation::TiledColor {
                    params,
                    color,
                    space,
                },
                &[base],
            )?
        } else {
            graph.add(
                Operation::Color {
                    params: color,
                    space,
                },
                &[calibrated],
            )?
        };
        finish!();
        if settings.color_dq_selected() {
            let gain = if let Some(gain) = settings.color_dq_gain {
                gain
            } else {
                let recipe = match couplings {
                    Some(recipe) => recipe,
                    None => profile.measure("prepare noise couplings", || {
                        crate::noise::couplings(
                            &source.metadata,
                            &settings.white_balance,
                            settings.reconstruction.unwrap_or_default(),
                        )
                    })?,
                };
                crate::noise::processing_gain(&source.metadata)?
                    .1
                    .min(recipe.color_dq_limit)
            };
            let table = profile.measure("prepare ColorDQ", || {
                crate::color_dq::prepare(&source.metadata, gain)
            })?;
            linear = if let Some((params, _, _, _)) = reconstruction {
                graph.add(
                    Operation::TiledColorDq {
                        params,
                        table,
                        space,
                    },
                    &[linear],
                )?
            } else {
                graph.add(Operation::ColorDq { table, space }, &[linear])?
            };
            finish!();
        }
        if let Some((params, _, detail, ratio)) = reconstruction {
            linear = graph.add(
                Operation::Reconstruct { params, space },
                &[linear, detail, ratio],
            )?;
            finish!();
        }
        if settings.sharpen_amount > 0. {
            let signal = if reconstruction.is_some() {
                linear + 1
            } else {
                let signal = graph.add(Operation::SharpSignal { space }, &[linear])?;
                finish!();
                signal
            };
            linear = graph.add(
                Operation::Sharpen {
                    amount: settings.sharpen_amount,
                    space,
                },
                &[linear, signal],
            )?;
            finish!();
        }
        if dng_base {
            return self.collect_artifacts(
                source, settings, &graph, linear, names, params, profile, execution,
            );
        }
        let curve = profile.measure("prepare tone curve", || {
            crate::tone::prepare(&source.metadata, &mode, settings.tone)
        })?;
        let mut toned = graph.add(
            Operation::Tone {
                curve,
                exposure: settings.exposure.exp2(),
                space,
            },
            &[linear],
        )?;
        finish!();
        if settings.saturation != 1. {
            toned = graph.add(
                Operation::Saturation {
                    params: saturation(dimensions, settings.saturation),
                    space,
                },
                &[toned],
            )?;
            finish!();
        }
        self.collect_artifacts(
            source, settings, &graph, toned, names, params, profile, execution,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_artifacts(
        &self,
        source: &Source,
        settings: &Settings,
        graph: &Graph,
        final_id: usize,
        names: &[&str],
        params: calibration::EncodeParams,
        profile: &mut Profile,
        execution: &mut Execution<'_>,
    ) -> Result<Vec<crate::inspection::Artifact>> {
        use crate::inspection::{Artifact, Content};
        let targets: Vec<_> = names
            .iter()
            .map(|name| {
                if *name == "final" {
                    Ok(final_id)
                } else {
                    graph
                        .outputs
                        .iter()
                        .position(|o| o.name == *name)
                        .ok_or_else(|| format!("unavailable output {name}"))
                }
            })
            .collect::<std::result::Result<_, _>>()?;
        if targets
            .iter()
            .enumerate()
            .any(|(i, id)| targets[..i].contains(id))
        {
            return Err("duplicate requested output".into());
        }
        execution.check()?;
        execution.plan(graph, &targets)?;
        let mut roots = targets.clone();
        for &id in &execution.checkpoints {
            if !roots.contains(&id) {
                roots.push(id);
            }
        }
        let values =
            self.execute_controlled(Input::Source(source), graph, &roots, profile, execution)?;
        values
            .into_iter()
            .zip(targets)
            .map(|(value, id)| {
                let output = &graph.outputs[id];
                let content = if let Some(description) = value.description {
                    Content::Image(Box::new(RenderedImage {
                        buffer: value.buffer,
                        description,
                        view: View {
                            crop: params.crop,
                            rotation: params.source[2],
                            extent: [params.output[0], params.output[1]],
                        },
                        metadata: source.metadata.clone(),
                        settings: settings.clone(),
                        owner: self.owner.clone(),
                    }))
                } else {
                    let bytes = self.gpu.readback(&value.buffer)?;
                    let values = match output.data {
                        DataType::Histogram => bytemuck::cast_slice::<u8, u32>(&bytes)
                            .iter()
                            .map(|v| *v as f32)
                            .collect(),
                        DataType::BlackLevels | DataType::RowTable => {
                            bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
                        }
                        _ => return Err("unsupported diagnostic output".into()),
                    };
                    Content::Table {
                        columns: match output.data {
                            DataType::Histogram => 4096,
                            DataType::RowTable => 4,
                            _ => 1,
                        },
                        values,
                    }
                };
                Ok(Artifact {
                    name: output.name,
                    metadata: source.metadata.clone(),
                    settings: settings.clone(),
                    content,
                })
            })
            .collect()
    }

    /// Encodes and exports an image created by this engine.
    pub fn write_tiff(
        &self,
        image: &RenderedImage,
        path: &Path,
        profile: &mut Profile,
    ) -> Result<[u32; 2]> {
        profile.mode.check_gpu_features(self.device().features())?;
        self.check_owner(image)?;
        if path.exists() {
            return Err("output file already exists".into());
        }
        let params = image.params();
        let Space::ToneMappedRgb(space) = image.description.space else {
            return Err("TIFF requires tone-mapped RGB".into());
        };
        let mut graph = Graph::default();
        let input = graph.add(
            Operation::RenderedInput {
                data: image.description.space.into(),
            },
            &[],
        )?;
        let encoded = graph.add(Operation::Encode { params, space }, &[input])?;
        let file = graph.add(
            Operation::Tiff {
                path: path.to_owned(),
                params,
                space,
            },
            &[encoded],
        )?;
        self.execute(Input::Rendered(image), &graph, &[file], profile)?;
        Ok(image.view.extent)
    }

    /// Materializes the logical view as an RGBA32Float texture on GPU.
    /// An optional extent changes the preview size, not the stored image or export size.
    /// Presenters use textureLoad/manual filtering or enable float32 filtering on the device.
    /// Colors stay in the working space so the presenter can apply display conversion.
    pub fn view_texture(
        &self,
        image: &RenderedImage,
        extent: Option<[u32; 2]>,
        profile: &mut Profile,
    ) -> Result<ImageTexture> {
        profile.mode.check_gpu_features(self.device().features())?;
        self.check_owner(image)?;
        if !matches!(
            image.description.layout,
            Layout::Rgb32Float | Layout::Scalar32Float
        ) {
            return Err("texture preparation requires floating image storage".into());
        }
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
        let mut trace = self.gpu.trace(profile.mode.gpu(), 1);
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
        targets: &[usize],
        profile: &mut Profile,
    ) -> Result<Vec<Value>> {
        self.execute_controlled(context, graph, targets, profile, &mut Execution::default())
    }

    fn execute_controlled(
        &self,
        context: Input<'_>,
        graph: &Graph,
        targets: &[usize],
        profile: &mut Profile,
        execution: &mut Execution<'_>,
    ) -> Result<Vec<Value>> {
        let gpu = &self.gpu;
        let mut trace = gpu.trace(profile.mode.gpu(), graph.nodes.len() as u32 * 2);
        let mut scratch = Scratch::default();
        let mut values: Vec<Option<Value>> = graph
            .outputs
            .iter()
            .map(|output| execution.seed(output.name))
            .collect();
        let cached: Vec<bool> = values.iter().map(Option::is_some).collect();
        let required = if cached.iter().any(|cached| *cached) {
            graph.required_until(targets, &cached)?
        } else {
            graph.required(targets)?
        };
        // A cached main output does not guarantee that a side output is cached.
        // Request only what is missing so a partial run cannot replace retained values.
        let required: Vec<bool> = required
            .iter()
            .zip(&cached)
            .map(|(needed, cached)| *needed && !*cached)
            .collect();
        let mut reads = vec![Vec::new(); graph.nodes.len()];
        for (id, output) in graph.outputs.iter().enumerate() {
            if required[id] && !cached[id] {
                for &input in &output.dependencies {
                    if !reads[output.producer].contains(&input) {
                        reads[output.producer].push(input);
                    }
                }
            }
        }
        let mut consumers = vec![0; graph.outputs.len()];
        for inputs in &reads {
            for &input in inputs {
                consumers[input] += 1;
            }
        }
        for &target in targets {
            consumers[target] += 1;
        }
        for (node_index, node) in graph.nodes.iter().enumerate() {
            execution.check()?;
            if !node.outputs.iter().any(|id| required[*id] && !cached[*id]) {
                continue;
            }
            let index = node.outputs[0];
            let start = Instant::now();
            let input = |slot: usize| {
                values[node.inputs[slot]].as_ref().unwrap_or_else(|| {
                    panic!(
                        "validated input {} for {} is not live",
                        graph.outputs[node.inputs[slot]].name,
                        node.operation.label()
                    )
                })
            };
            let inherited = reads[node_index]
                .iter()
                .find_map(|id| {
                    values[*id]
                        .as_ref()
                        .and_then(|value| value.description)
                        .map(|d| d.extent)
                })
                .unwrap_or_else(|| match &context {
                    Input::Source(source) => {
                        [source.metadata.width as u32, source.metadata.height as u32]
                    }
                    Input::Rendered(image) => image.description.extent,
                });
            let extent = match &node.operation {
                Operation::Encode { params, .. } => [params.output[0], params.output[1]],
                _ => inherited,
            };
            let desc = description(graph.outputs[index].data, extent);
            let groups = [inherited[0].div_ceil(8), inherited[1].div_ceil(8)];
            let dimensions = [inherited[0], inherited[1], 0, 0];
            let bytes = u64::from(inherited[0]) * u64::from(inherited[1]) * 12;
            let mut dispatch = |kernel, buffers: &mut [(u32, crate::gpu::Binding<'_>)], groups| {
                gpu.dispatch(kernel, buffers, groups, &mut trace)
            };
            let output = match &node.operation {
                Operation::Source => {
                    let Input::Source(source) = &context else {
                        return Err("sensor input context required".into());
                    };
                    Some(gpu.upload("Original sensor layers", &source.pixels)?)
                }
                Operation::RenderedInput { data } => {
                    let Input::Rendered(image) = &context else {
                        return Err("rendered input context required".into());
                    };
                    if description(*data, image.description.extent) != Some(image.description)
                        || image.buffer.size() < image.description.byte_len()
                    {
                        return Err(
                            "imported image representation does not match graph input".into()
                        );
                    }
                    Some(image.buffer.clone())
                }
                Operation::Black(params) => {
                    let mut histogram = gpu.storage("Dark-shield histogram", 3 * 4096 * 4)?;
                    let uniform = gpu.uniform(params);
                    dispatch(
                        Kernel::Histogram,
                        &mut [
                            (0, Read(&input(0).buffer)),
                            (1, Write(histogram.write())),
                            (2, Read(&uniform)),
                        ],
                        [params.source[2].div_ceil(64), 1],
                    );
                    let black = if required[index] {
                        let mut black = gpu.storage("Black levels", 12)?;
                        dispatch(
                            Kernel::BlackMean,
                            &mut [
                                (1, Read(&histogram)),
                                (2, Read(&uniform)),
                                (3, Write(black.write())),
                            ],
                            [3, 1],
                        );
                        Some(black)
                    } else {
                        None
                    };
                    let side = node.outputs[1];
                    if required[side] {
                        values[side] = Some(Value {
                            buffer: histogram,
                            description: None,
                        });
                    }
                    black
                }
                Operation::Calibrate(calibration) => {
                    let metadata = context.metadata();
                    if inherited != [metadata.width as u32, metadata.height as u32] {
                        return Err("calibration requires original sensor coordinates".into());
                    }
                    let mut output = scratch.take(gpu, bytes)?;
                    let grid = gpu.upload("Spatial gain grids", &calibration.grid_values)?;
                    let uniform = gpu.uniform(&calibration.params);
                    dispatch(
                        Kernel::Calibration,
                        &mut [
                            (0, Read(&input(0).buffer)),
                            (1, Read(&input(1).buffer)),
                            (2, Read(&grid)),
                            (3, Read(&uniform)),
                            (4, Write(output.write())),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::NoiseBase(params) => {
                    let projected = node.outputs[1];
                    let edge_id = node.outputs[2];
                    let base = crate::reconstruction::base(
                        gpu,
                        &input(0).buffer,
                        *params,
                        required[edge_id],
                        required[index] || required[projected],
                        required[node.outputs[3]],
                        profile,
                    )?;
                    if required[projected] {
                        values[projected] = Some(Value {
                            buffer: crate::reconstruction::project(
                                gpu,
                                base.tiles.as_ref().unwrap(),
                                *params,
                                3,
                                false,
                                profile,
                            )?,
                            description: description(DataType::CalibratedLayers, inherited),
                        });
                    }
                    if required[edge_id] {
                        values[edge_id] = Some(Value {
                            buffer: crate::reconstruction::project(
                                gpu,
                                base.edges.as_ref().unwrap(),
                                *params,
                                1,
                                false,
                                profile,
                            )?,
                            description: description(DataType::ScalarField, inherited),
                        });
                    }
                    if required[node.outputs[3]] {
                        values[node.outputs[3]] = Some(Value {
                            buffer: base.seed.unwrap(),
                            description: None,
                        });
                    }
                    if required[index] {
                        base.tiles
                    } else {
                        None
                    }
                }
                Operation::Column(params) => {
                    let tiles =
                        crate::reconstruction::column(gpu, &input(0).buffer, *params, profile)?;
                    let projected = node.outputs[1];
                    if required[projected] {
                        values[projected] = Some(Value {
                            buffer: crate::reconstruction::project(
                                gpu, &tiles, *params, 3, false, profile,
                            )?,
                            description: description(DataType::CalibratedLayers, inherited),
                        });
                    }
                    required[index].then_some(tiles)
                }
                Operation::Ratio(params) => {
                    let ratio = crate::reconstruction::ratio(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        *params,
                        profile,
                    )?;
                    let projected = node.outputs[1];
                    if required[projected] {
                        values[projected] = Some(Value {
                            buffer: crate::reconstruction::project(
                                gpu, &ratio, *params, 1, false, profile,
                            )?,
                            description: description(DataType::LuminanceRatio, inherited),
                        });
                    }
                    required[index].then_some(ratio)
                }
                Operation::Pattern { params, prepared } => {
                    let projected = node.outputs[1];
                    let mask = node.outputs[2];
                    let result = crate::pattern::apply(
                        gpu,
                        &input(0).buffer,
                        &input(1).buffer,
                        prepared,
                        required[index] || required[projected],
                        required[mask],
                        profile,
                    )?;
                    if required[projected] {
                        values[projected] = Some(Value {
                            buffer: crate::reconstruction::project(
                                gpu,
                                result.ratio.as_ref().unwrap(),
                                *params,
                                1,
                                false,
                                profile,
                            )?,
                            description: description(DataType::LuminanceRatio, inherited),
                        });
                    }
                    if required[mask] {
                        values[mask] = Some(Value {
                            buffer: crate::reconstruction::project(
                                gpu,
                                result.mask.as_ref().unwrap(),
                                *params,
                                1,
                                false,
                                profile,
                            )?,
                            description: description(DataType::ScalarField, inherited),
                        });
                    }
                    if required[index] {
                        result.ratio
                    } else {
                        None
                    }
                }
                Operation::Detail(params) => Some(crate::reconstruction::mix(
                    gpu,
                    &input(0).buffer,
                    &input(1).buffer,
                    &input(2).buffer,
                    *params,
                    profile,
                )?),
                Operation::TiledColor { params, color, .. } => {
                    let tiles = crate::reconstruction::color(
                        gpu,
                        &input(0).buffer,
                        *color,
                        *params,
                        profile,
                    )?;
                    let projected = node.outputs[1];
                    if required[projected] {
                        values[projected] = Some(Value {
                            buffer: crate::reconstruction::project(
                                gpu, &tiles, *params, 3, false, profile,
                            )?,
                            description: description(graph.outputs[projected].data, inherited),
                        });
                    }
                    required[index].then_some(tiles)
                }
                Operation::TiledColorDq { params, table, .. } => {
                    let tiles = crate::reconstruction::color_dq(
                        gpu,
                        &input(0).buffer,
                        table,
                        *params,
                        profile,
                    )?;
                    let projected = node.outputs[1];
                    if required[projected] {
                        values[projected] = Some(Value {
                            buffer: crate::reconstruction::project(
                                gpu, &tiles, *params, 3, false, profile,
                            )?,
                            description: description(graph.outputs[projected].data, inherited),
                        });
                    }
                    required[index].then_some(tiles)
                }
                Operation::Reconstruct { params, .. } => {
                    let signal_id = node.outputs[1];
                    let signal = if required[signal_id] {
                        Some(crate::reconstruction::signal(
                            gpu,
                            &input(0).buffer,
                            &input(2).buffer,
                            *params,
                            profile,
                        )?)
                    } else {
                        None
                    };
                    let rgb = if required[index] {
                        Some(crate::reconstruction::restore(
                            gpu,
                            &input(0).buffer,
                            &input(1).buffer,
                            *params,
                            profile,
                        )?)
                    } else {
                        None
                    };
                    if let Some(buffer) = signal {
                        values[signal_id] = Some(Value {
                            buffer,
                            description: description(DataType::SignedDetail, inherited),
                        });
                    }
                    rgb
                }
                Operation::Banding(prepared) => {
                    let result = crate::banding::apply(
                        gpu,
                        &input(0).buffer,
                        prepared,
                        required[index],
                        profile,
                    )?;
                    let rows = node.outputs[1];
                    if required[rows] {
                        values[rows] = Some(Value {
                            buffer: result.rows,
                            description: None,
                        });
                    }
                    result.image
                }
                Operation::Highlights(prepared) => {
                    crate::highlight::apply(gpu, Some(&input(0).buffer), prepared, false, profile)?
                        .pixels
                }
                Operation::HighlightDensity(prepared) => {
                    crate::highlight::apply(gpu, None, prepared, true, profile)?.density
                }
                Operation::Despeckle(prepared) => Some(crate::despeckle::apply(
                    gpu,
                    &input(0).buffer,
                    prepared,
                    profile,
                )?),
                Operation::Repair(prepared) => Some(crate::camera_repair::apply(
                    gpu,
                    &input(1).buffer,
                    &input(0).buffer,
                    prepared,
                    profile,
                )?),
                Operation::Color { .. }
                | Operation::SensorGain { .. }
                | Operation::Saturation { .. } => {
                    let mut output = scratch.take(gpu, bytes)?;
                    let mut params = match &node.operation {
                        Operation::Color { params, .. } | Operation::Saturation { params, .. } => {
                            *params
                        }
                        Operation::SensorGain { factor } => sensor_gain(dimensions, *factor),
                        _ => unreachable!(),
                    };
                    params.source[..2].copy_from_slice(&inherited);
                    let uniform = gpu.uniform(&params);
                    dispatch(
                        Kernel::Color,
                        &mut [
                            (0, Read(&input(0).buffer)),
                            (1, Read(&uniform)),
                            (2, Write(output.write())),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::ColorDq { table, .. } => {
                    let mut output = scratch.take(gpu, bytes)?;
                    let lookup = gpu.upload("ColorDQ lookup", &table.values)?;
                    let uniform = gpu.uniform(&[inherited[0], inherited[1], table.period, 0]);
                    dispatch(
                        Kernel::ColorDq,
                        &mut [
                            (0, Read(&input(0).buffer)),
                            (1, Read(&lookup)),
                            (2, Read(&uniform)),
                            (3, Write(output.write())),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::Tone {
                    curve, exposure, ..
                } => {
                    let mut output = scratch.take(gpu, bytes)?;
                    let curve = gpu.upload("Color-mode tone curve", curve)?;
                    let uniform = gpu.uniform(&ScalarParams {
                        source: dimensions,
                        value: [*exposure, 0., 0., 0.],
                    });
                    dispatch(
                        Kernel::Tone,
                        &mut [
                            (0, Read(&input(0).buffer)),
                            (1, Read(&curve)),
                            (2, Read(&uniform)),
                            (3, Write(output.write())),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::SharpSignal { .. } => {
                    let mut output = gpu.storage(
                        "Native signed mono high-pass",
                        u64::from(inherited[0]) * u64::from(inherited[1]) * 4,
                    )?;
                    let uniform = gpu.uniform(&ScalarParams {
                        source: dimensions,
                        value: [0.; 4],
                    });
                    dispatch(
                        Kernel::SharpSignal,
                        &mut [
                            (0, Read(&input(0).buffer)),
                            (2, Read(&uniform)),
                            (3, Write(output.write())),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::Sharpen { amount, .. } => {
                    if input(0).description.unwrap().extent != input(1).description.unwrap().extent
                    {
                        return Err("sharpen inputs have different image extents".into());
                    }
                    let mut output = scratch.take(gpu, bytes)?;
                    let uniform = gpu.uniform(&ScalarParams {
                        source: dimensions,
                        value: [*amount, 0., 0., 0.],
                    });
                    dispatch(
                        Kernel::Sharpen,
                        &mut [
                            (0, Read(&input(0).buffer)),
                            (1, Read(&input(1).buffer)),
                            (2, Read(&uniform)),
                            (3, Write(output.write())),
                        ],
                        groups,
                    );
                    Some(output)
                }
                Operation::Encode { params, .. } => {
                    let mut output = gpu.storage(
                        "Encoded packed RGB16",
                        desc.expect("encoded description").byte_len(),
                    )?;
                    let uniform = gpu.uniform(params);
                    dispatch(
                        Kernel::Encode,
                        &mut [
                            (0, Read(&input(0).buffer)),
                            (1, Read(&uniform)),
                            (2, Write(output.write())),
                        ],
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
            if !cached[index] {
                values[index] = output.map(|buffer| Value {
                    buffer,
                    description: desc,
                });
            }
            for &id in &node.outputs {
                if let Some(value) = values[id].as_ref() {
                    execution.store(graph.outputs[id].name, value);
                }
            }
            // Queuing a whole outdated render would delay both drawing and newer edits.
            // Finish submitted work off-thread, then check for newer work between stages.
            // Batch rendering has no interactive queue to protect and can stay asynchronous.
            if execution.cancel.is_some() {
                gpu.device.poll(wgpu::Maintain::Wait);
            }
            for &id in &reads[node_index] {
                consumers[id] -= 1;
                if consumers[id] == 0 {
                    if let Some(value) = values[id].take() {
                        // The scratch pool can reuse ordinary image buffers, not tiled or
                        // atomic workspaces. Release also checks for retained readers
                        // so a later render cannot overwrite an earlier image.
                        if value.description.is_some_and(|d| {
                            matches!(d.layout, Layout::Rgb32Float | Layout::Scalar32Float)
                        }) {
                            scratch.release(value.buffer);
                        }
                    }
                }
            }
        }
        gpu.finish_trace(&trace, profile)?;
        if profile.mode.enabled() {
            profile.scratch_allocations += scratch.allocations;
            profile.scratch_reuses += scratch.reuses;
        }
        Ok(targets.iter().filter_map(|id| values[*id].take()).collect())
    }
}

#[cfg(test)]
#[path = "interactive_test.rs"]
mod interactive_tests;
#[cfg(test)]
#[path = "render_test.rs"]
mod tests;
