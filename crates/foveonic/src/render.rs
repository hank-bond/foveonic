//! Shares GPU engine entry points for photographic rendering, inspection, display, and export.

use crate::gpu::Gpu;
use crate::graph::{Graph, Operation};
use crate::image::{Description, ImageTexture, Layout, RenderedImage, Space};
pub use crate::interactive::RenderSession;
use crate::interactive::{Execution, Superseded};
use crate::profile::Profile;
use crate::source::{Result, Source};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

mod artifacts;
mod capabilities;
#[path = "dng/gpu.rs"]
mod dng_gpu;
mod execute;
mod recipes;
mod settings;

pub use capabilities::ControlCapabilities;

use execute::Input;
pub(crate) use execute::Value;
use recipes::Recipe;
pub use settings::{
    ColorFieldApproach, ColorFieldControls, HighlightMode, MonochromeControls, PreviewQuality,
    Rendering, RepairThreshold, Settings, TiffColorSpace, ToneControls,
};

/// Completed recipe preparation and execution boundaries for an interactive render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderProgress {
    Prepared,
    Stage { completed: usize, total: usize },
}

/// Small image-derived statistics used while preparing calibration.
#[derive(Clone, Copy, Default)]
pub(crate) struct WhiteBalanceStats {
    pub neutral: Option<[f64; 3]>,
    pub mean: Option<[f64; 3]>,
}

pub struct Engine {
    gpu: Gpu,
    owner: Arc<()>,
    // Keep the latest photo's calibration statistics with the engine. Metadata lifetime
    // and the original sample allocation identify the immutable source.
    calibration_statistics: std::sync::Mutex<Vec<CalibrationEstimate>>,
    calibration_resources: Option<Arc<recipes::quattro::resources::ResourceDirectory>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StatisticKind {
    Neutral,
    ChannelMean,
}

struct CalibrationEstimate {
    metadata: std::sync::Weak<crate::source::Metadata>,
    samples: usize,
    kind: StatisticKind,
    values: [f64; 3],
}

impl Engine {
    pub fn new() -> Result<Self> {
        Ok(Self {
            gpu: Gpu::new()?,
            owner: Arc::new(()),
            calibration_statistics: Default::default(),
            calibration_resources: Some(Arc::new(
                recipes::quattro::resources::ResourceDirectory::bundled()?,
            )),
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
            calibration_statistics: Default::default(),
            calibration_resources: Some(Arc::new(
                recipes::quattro::resources::ResourceDirectory::bundled()?,
            )),
        })
    }
    /// Override bundled calibration maps with an immutable snapshot of supplied maps.
    /// Resources are loaded once and never resolved through the process directory.
    pub fn with_calibration_resources(mut self, directory: impl AsRef<Path>) -> Result<Self> {
        self.calibration_resources = Some(Arc::new(
            recipes::quattro::resources::ResourceDirectory::new(directory.as_ref())?,
        ));
        Ok(self)
    }

    pub(crate) fn cached_neutral(
        &self,
        source: &Source,
        calculate: impl FnOnce() -> Result<[f64; 3]>,
    ) -> Result<[f64; 3]> {
        self.cached_statistic(source, StatisticKind::Neutral, calculate)
    }

    pub(crate) fn cached_channel_mean(
        &self,
        source: &Source,
        calculate: impl FnOnce() -> Result<[f64; 3]>,
    ) -> Result<[f64; 3]> {
        self.cached_statistic(source, StatisticKind::ChannelMean, calculate)
    }

    fn cached_statistic(
        &self,
        source: &Source,
        kind: StatisticKind,
        calculate: impl FnOnce() -> Result<[f64; 3]>,
    ) -> Result<[f64; 3]> {
        let mut cache = self
            .calibration_statistics
            .lock()
            .map_err(|_| "calibration statistics cache poisoned")?;
        let samples = source.pixels.as_ptr() as usize;
        cache.retain(|cached| {
            cached.samples == samples
                && cached
                    .metadata
                    .upgrade()
                    .is_some_and(|metadata| Arc::ptr_eq(&metadata, &source.metadata))
        });
        if let Some(cached) = cache.iter().find(|cached| cached.kind == kind) {
            return Ok(cached.values);
        }
        let values = calculate()?;
        cache.push(CalibrationEstimate {
            metadata: Arc::downgrade(&source.metadata),
            samples,
            kind,
            values,
        });
        Ok(values)
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

    /// Analyze a region in cropped, rotated output pixels, or the whole view with None.
    /// Right and bottom bounds are excluded. Only counts and weights leave the GPU.
    pub fn scopes(
        &self,
        image: &RenderedImage,
        region: Option<crate::scopes::Region>,
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
        crate::scopes::analyze(&self.gpu, image, region, profile)
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
        settings.validate_for_camera(&source.metadata)?;
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
        self.render_session_observed(session, settings, names, cancel, profile, None)
    }

    /// Report completed graph stages after the interactive executor's existing GPU waits.
    #[allow(clippy::too_many_arguments)]
    pub fn render_session_with_progress(
        &self,
        session: &mut RenderSession,
        settings: &Settings,
        names: &[&str],
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: impl Fn(RenderProgress),
    ) -> Result<Option<Vec<crate::inspection::Artifact>>> {
        self.render_session_observed(session, settings, names, cancel, profile, Some(&progress))
    }

    #[allow(clippy::too_many_arguments)]
    fn render_session_observed(
        &self,
        session: &mut RenderSession,
        settings: &Settings,
        names: &[&str],
        cancel: impl Fn() -> bool,
        profile: &mut Profile,
        progress: Option<&dyn Fn(RenderProgress)>,
    ) -> Result<Option<Vec<crate::inspection::Artifact>>> {
        session.cache.bind(&self.owner);
        session.last_settings = Some(settings.clone());
        let mut execution = Execution {
            cache: Some(&mut session.cache),
            cancel: Some(&cancel),
            checkpoints: Vec::new(),
            progress,
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
        let mut params = image.params();
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
        // The camera's output preference is independent of working primaries.
        let destination = image.settings.tiff_color_space.unwrap_or_else(|| {
            crate::image::ColorSpace::from_metadata(&image.metadata)
                .unwrap_or(crate::image::ColorSpace::Srgb)
                .into()
        });
        params.output[2] = destination as u32;
        // Neutral mono has the same white in every destination. Avoid adding
        // rounding differences between its otherwise equal channels.
        params.output[3] = u32::from(
            destination != TiffColorSpace::from(space) && !image.settings.is_monochrome(),
        );
        let encoded = graph.add(
            Operation::Encode {
                params,
                space,
                destination,
            },
            &[input],
        )?;
        let file = graph.add(
            Operation::Tiff {
                path: path.to_owned(),
                params,
                space: destination,
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
                view_baked: true,
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
}

#[cfg(test)]
#[path = "render/export_test.rs"]
mod export_tests;
#[cfg(test)]
#[path = "render/interactive_test.rs"]
mod interactive_tests;
#[cfg(test)]
#[path = "render/monochrome_test.rs"]
mod monochrome_tests;
#[cfg(test)]
#[path = "render/resize_export_test.rs"]
mod resize_export_tests;
#[cfg(test)]
#[path = "render/render_test.rs"]
mod tests;

#[cfg(test)]
#[path = "render/classic_test.rs"]
mod classic_tests;

#[cfg(test)]
#[path = "render/quattro_dng_test.rs"]
mod quattro_dng_tests;

/// Resolve camera interpretation at the recipe boundary before shared DNG fitting.
pub(crate) fn dng_appearance(
    metadata: &crate::source::Metadata,
    settings: &Settings,
    statistics: WhiteBalanceStats,
) -> Result<crate::dng::appearance::Prepared> {
    recipes::prepare_dng_appearance(metadata, settings, statistics)
}

/// Resolve image-derived calibration before fitting an editable appearance.
pub(crate) fn estimated_white_balance(
    engine: &Engine,
    source: &Source,
    settings: &Settings,
    profile: &mut Profile,
) -> Result<WhiteBalanceStats> {
    recipes::estimated_white_balance(engine, source, settings, profile)
}

pub(crate) fn classic_inspection_outputs(
    metadata: &crate::source::Metadata,
    settings: &Settings,
) -> Vec<&'static str> {
    recipes::classic::inspection_outputs(metadata, settings)
}
pub(crate) fn classic_inspection_comparison(
    metadata: &crate::source::Metadata,
    settings: &Settings,
    output: &str,
) -> Option<&'static str> {
    recipes::classic::inspection_comparison(metadata, settings, output)
}

pub(crate) fn quattro_inspection_outputs(
    metadata: Option<&crate::source::Metadata>,
) -> Vec<crate::inspection::ImageOutput> {
    recipes::quattro::outputs(metadata)
}

pub(crate) fn quattro_inspection_comparison(
    metadata: &crate::source::Metadata,
    output: &str,
) -> Option<&'static str> {
    recipes::quattro::comparison_input(metadata, output)
}

#[cfg(test)]
#[path = "render/quattro_test.rs"]
mod quattro_tests;
