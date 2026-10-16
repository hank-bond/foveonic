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
#[path = "dng/gpu.rs"]
mod dng_gpu;
mod execute;
mod recipes;
mod settings;

use execute::Input;
pub(crate) use execute::Value;
use recipes::Recipe;
pub use settings::{
    MonochromeControls, Rendering, RepairThreshold, Settings, TiffColorSpace, ToneControls,
};

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
        let destination = image.settings.tiff_color_space.unwrap_or(space.into());
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
