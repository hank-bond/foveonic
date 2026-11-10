//! Retains edited GPU previews immediately and persists bounded photographic previews after foreground work yields.

use crate::{preview_image::PreparedPreview, rendering::worker::Frame, viewport};
use eframe::{egui, egui_wgpu, wgpu};
use foveonic::diagnostics as trace;
use foveonic::{
    decode_cache::Cache, image::ImageTexture, preview::Preview, render::Settings, source::Metadata,
};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{mpsc, Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

// Enough detail for a fitted viewport without retaining native-size JPEGs.
const PREVIEW_SIDE: u32 = foveonic::decode_cache::PREVIEW_MAX_SIDE;
// Let the main image reach the screen before preparing browser feedback.
const PRESENT_FIRST: Duration = Duration::from_millis(16);
// Read back linear RGBA16Float before encoding a monitor-independent RGB16 asset.
const RGBA_BYTES: u32 = size_of::<[u16; 4]>() as u32;
const RGB_CHANNELS: usize = 3;
// Bound completed CPU display pixels while the UI consumes its previous reply.
const READY_CAPACITY: usize = 1;

#[derive(Clone)]
struct Job {
    diagnostic_task: crate::timing::DeliveredTask,
    epoch: u64,
    calibration_signature: Option<String>,
    path: PathBuf,
    revision: u64,
    source: Arc<ImageTexture>,
    metadata: Arc<Metadata>,
    settings: Settings,
    native_extent: [u32; 2],
}
#[derive(Default)]
struct Pending {
    job: Option<Job>,
    active: Option<Job>,
    revision: u64,
    stop: bool,
}
pub struct Ready {
    pub epoch: u64,
    pub calibration_signature: Option<String>,
    pub path: PathBuf,
    pub revision: u64,
    pub image: PreparedPreview,
    pub(crate) _memory: Option<crate::display_image::StagedBytes>,
}
pub struct Previews {
    render_state: egui_wgpu::RenderState,
    cache: Arc<Cache>,
    epoch: u64,
    invalid_paths: HashSet<PathBuf>,
    pending: Arc<(Mutex<Pending>, Condvar)>,
    pub replies: mpsc::Receiver<Ready>,
    wanted: Option<(Job, Instant)>,
    sent: Option<(PathBuf, u64)>,
    staging: crate::display_image::Staging,
    resource_revision: u64,
}
impl Previews {
    pub fn invalidate(&mut self) {
        self.resource_revision = self.resource_revision.wrapping_add(1);
        self.epoch = self.epoch.wrapping_add(1);
        self.invalid_paths.clear();
        self.sent = None;
        self.wanted = None;
        self.pending.0.lock().unwrap().job = None;
    }
    pub fn new(rs: egui_wgpu::RenderState, context: egui::Context, cache: Arc<Cache>) -> Self {
        let render_state = rs.clone();
        let policy = cache.clone();
        let pending = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let worker = pending.clone();
        let (tx, replies) = mpsc::sync_channel(READY_CAPACITY);
        let staging = crate::display_image::Staging::default();
        let worker_staging = staging.clone();
        let builder = std::thread::Builder::new().name("rendered-previews".into());
        let spawned = builder.spawn(move || {
            loop {
                let job = {
                    let mut state = worker.0.lock().unwrap();
                    while state.job.is_none() && !state.stop {
                        state = worker.1.wait(state).unwrap();
                    }
                    if state.stop {
                        break;
                    }
                    let job = state.job.take().unwrap();
                    state.active = Some(job.clone());
                    state.revision = state.revision.wrapping_add(1);
                    job
                };
                let _context = trace::enabled().then(|| {
                    trace::enter(crate::timing::context_for(
                        job.diagnostic_task.id,
                        job.diagnostic_task.revision,
                        &job.path,
                        "preview_generation",
                    ))
                });
                let _job =
                    trace::scope_with(trace::Category::Presentation, "preview.job", |fields| {
                        fields
                            .str("source", &job.path.to_string_lossy())
                            .u64("presentation_revision", job.revision)
                            .u64("epoch", job.epoch);
                    });
                let result = cache.background(|| {
                    if !cache
                        .permits_preview_storage(&job.settings, &job.metadata)
                        .unwrap_or(false)
                    {
                        return None;
                    }
                    // A newer frame can replace this job while it waits for foreground work.
                    {
                        let state = worker.0.lock().unwrap();
                        if state.stop
                            || state
                                .job
                                .as_ref()
                                .is_some_and(|j| j.revision > job.revision)
                        {
                            trace::event(
                                trace::Category::Coordinator,
                                "preview.superseded",
                                |_| {},
                            );
                            return None;
                        }
                    }
                    let _generate = trace::scope(trace::Category::Cpu, "preview.generate");
                    let preview = downsample(&rs, &job.source).ok()?;
                    let prepared = PreparedPreview::new(
                        Preview {
                            rgb: preview.rgb.clone(),
                            color_space: preview.color_space,
                            extent: preview.extent,
                            rotation: 0,
                            native_extent: job.native_extent,
                        },
                        true,
                    )
                    .ok()?;
                    let _persist = trace::scope(trace::Category::Cache, "preview.persist_request");
                    let persisted = cache.save_preview(
                        &job.path,
                        &job.settings,
                        &job.metadata,
                        Preview {
                            native_extent: job.native_extent,
                            ..preview
                        },
                        job.calibration_signature.as_deref(),
                    );
                    trace::event(trace::Category::Cache, "preview.persist_result", |fields| {
                        fields.bool("api_ok", persisted.is_ok());
                    });
                    Some(prepared)
                });
                trace::event(trace::Category::Presentation, "preview.result", |fields| {
                    fields.bool("ready", result.is_some());
                });
                if let Some(image) = result {
                    let memory = worker_staging.track(image.retained_bytes());
                    let _send = trace::scope_with(
                        trace::Category::Coordinator,
                        "preview.reply_send",
                        |fields| {
                            fields
                                .str("measurement", "host_wait")
                                .u64("capacity", READY_CAPACITY as u64);
                        },
                    );
                    if tx
                        .send(Ready {
                            epoch: job.epoch,
                            calibration_signature: job.calibration_signature,
                            path: job.path,
                            revision: job.revision,
                            image,
                            _memory: memory,
                        })
                        .is_err()
                    {
                        let mut state = worker.0.lock().unwrap();
                        state.active = None;
                        state.revision = state.revision.wrapping_add(1);
                        break;
                    }
                    context.request_repaint();
                }
                let mut state = worker.0.lock().unwrap();
                state.active = None;
                state.revision = state.revision.wrapping_add(1);
            }
        });
        spawned.expect("start rendered-previews thread");
        Self {
            render_state,
            cache: policy,
            epoch: 0,
            invalid_paths: HashSet::new(),
            pending,
            replies,
            wanted: None,
            sent: None,
            staging,
            resource_revision: 0,
        }
    }
    pub fn resource_revision(&self) -> u64 {
        self.resource_revision
            .wrapping_add(self.pending.0.lock().unwrap().revision)
            .wrapping_add(self.staging.revision())
    }
    pub fn memory_resources(&self, inventory: &mut crate::memory::Inventory) {
        let account = |job: &Job, inventory: &mut crate::memory::Inventory| {
            inventory.image_texture(&job.source);
            inventory.metadata(&job.metadata);
        };
        if let Some((job, _)) = &self.wanted {
            account(job, inventory);
        }
        let state = self.pending.0.lock().unwrap();
        if let Some(job) = &state.job {
            account(job, inventory);
        }
        if let Some(job) = &state.active {
            account(job, inventory);
        }
        inventory.cpu(self.staging.bytes());
    }
    pub fn accepts(&self, reply: &Ready) -> bool {
        reply.epoch == self.epoch && !self.invalid_paths.contains(&reply.path)
    }

    /// Reject delayed preview readbacks after a presentation-only browser rotation.
    pub fn invalidate_path(&mut self, path: &std::path::Path) {
        self.invalid_paths.insert(path.to_owned());
    }

    /// Queue a bounded GPU thumbnail without waiting for readback or disk caching.
    pub fn snapshot(
        &mut self,
        frame: &Frame,
    ) -> Option<(crate::browser::Thumbnail, crate::browser::Thumbnail)> {
        if !photographic(frame) {
            return None;
        }
        self.invalid_paths.remove(&frame.path);
        let make = |side| {
            let target = scaled_texture(
                &self.render_state,
                frame.texture.texture(),
                frame.texture.description().space,
                side,
            );
            crate::browser::Thumbnail {
                texture: crate::display_image::Texture::from_native(&self.render_state, target),
                native_extent: frame.display_extent(false),
            }
        };
        Some((make(256), make(PREVIEW_SIDE)))
    }
    /// Keep one replaceable pending whole-image job. Diagnostics never become
    /// photographic thumbnails, even when they are displayed in the navigator.
    pub fn update(&mut self, frame: Option<&Frame>, busy: bool, context: &egui::Context) {
        let Some(frame) = frame.filter(|f| {
            photographic(f)
                && self
                    .cache
                    .permits_preview_storage(&f.artifact.settings, &f.artifact.metadata)
                    .unwrap_or(false)
        }) else {
            if self.wanted.is_some() {
                self.resource_revision = self.resource_revision.wrapping_add(1);
            }
            self.wanted = None;
            return;
        };
        let key = (frame.path.clone(), frame.revision);
        if self.sent.as_ref() == Some(&key) {
            return;
        }
        if self
            .wanted
            .as_ref()
            .is_none_or(|(j, _)| j.path != key.0 || j.revision != key.1)
        {
            self.resource_revision = self.resource_revision.wrapping_add(1);
            self.wanted = Some((
                Job {
                    diagnostic_task: frame.diagnostic_task,
                    epoch: self.epoch,
                    calibration_signature: frame.calibration_signature.clone(),
                    path: frame.path.clone(),
                    revision: frame.revision,
                    source: frame.texture.clone(),
                    metadata: frame.artifact.metadata.clone(),
                    settings: frame.artifact.settings.clone(),
                    native_extent: frame.display_extent(false),
                },
                Instant::now() + PRESENT_FIRST,
            ));
        }
        if busy {
            return;
        }
        let ready = self.wanted.as_ref().unwrap().1;
        if ready > Instant::now() {
            context.request_repaint_after(ready.saturating_duration_since(Instant::now()));
            return;
        }
        {
            let mut state = self.pending.0.lock().unwrap();
            state.job = self.wanted.take().map(|(j, _)| j);
            state.revision = state.revision.wrapping_add(1);
        }
        self.sent = Some(key);
        self.pending.1.notify_one();
    }
}
impl Drop for Previews {
    fn drop(&mut self) {
        let mut state = self.pending.0.lock().unwrap();
        state.stop = true;
        state.job = None;
        self.pending.1.notify_one();
    }
}

fn photographic(frame: &Frame) -> bool {
    frame.rendition == crate::editor::Rendition::Current
        && !frame.framing_preview
        && frame.output == "final"
        && frame.artifact.settings.color_range_preview.is_none()
        && frame.artifact.settings.preview_region.is_none()
}

fn scaled_texture(
    rs: &egui_wgpu::RenderState,
    source: &wgpu::Texture,
    space: foveonic::image::Space,
    side: u32,
) -> wgpu::Texture {
    let _scope = trace::scope_with(trace::Category::Gpu, "preview.render_texture", |fields| {
        fields.u64("maximum_side", u64::from(side));
    });
    let extent = [source.width(), source.height()];
    let scale = (side as f32 / extent[0].max(extent[1]) as f32).min(1.);
    let size = extent.map(|v| (v as f32 * scale).round().max(1.) as u32);
    let target = rs.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Cached photographic preview"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    viewport::prepare_preview(
        rs,
        source,
        space,
        &target,
        viewport::Mapping::normal(-1),
        None,
        None,
    );
    target
}

fn downsample(rs: &egui_wgpu::RenderState, source: &ImageTexture) -> Result<Preview, String> {
    downsample_texture(rs, source.texture(), source.description().space)
}

fn downsample_texture(
    rs: &egui_wgpu::RenderState,
    source: &wgpu::Texture,
    space: foveonic::image::Space,
) -> Result<Preview, String> {
    let _scope = trace::scope(trace::Category::Cpu, "preview.downsample_readback");
    let extent = [source.width(), source.height()];
    let target = scaled_texture(rs, source, space, PREVIEW_SIDE);
    let size = [target.width(), target.height()];
    let row = (size[0] * RGBA_BYTES).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = rs.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Display preview readback"),
        size: u64::from(row) * u64::from(size[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = rs.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(size[1]),
            },
        },
        target.size(),
    );
    rs.queue.submit(Some(encoder.finish()));
    {
        let _wait = trace::scope_with(trace::Category::Gpu, "preview.readback_wait", |fields| {
            fields.str("measurement", "host_wait");
        });
        foveonic::map_readback(&rs.device, &buffer).map_err(|e| e.to_string())?;
    }
    let color_space = foveonic::color_space::TiffColorSpace::ProPhotoRgb;
    let matrix = color_space.transform(foveonic::image::ColorSpace::Srgb);
    let rgb = {
        let bytes = buffer.slice(..).get_mapped_range();
        let mut rgb = Vec::with_capacity(size[0] as usize * size[1] as usize * RGB_CHANNELS);
        for row in bytes.chunks_exact(row as usize) {
            for pixel in row[..size[0] as usize * RGBA_BYTES as usize]
                .as_chunks::<{ RGBA_BYTES as usize }>()
                .0
            {
                let linear: [f32; 3] = std::array::from_fn(|c| {
                    half::f16::from_le_bytes([pixel[c * 2], pixel[c * 2 + 1]]).to_f32()
                });
                rgb.extend((0..RGB_CHANNELS).map(|r| {
                    foveonic::preview::encode_sample(
                        color_space,
                        (0..RGB_CHANNELS).map(|c| matrix[r][c] * linear[c]).sum(),
                    )
                }));
            }
        }
        rgb
    };
    buffer.unmap();
    Ok(Preview {
        rgb,
        color_space,
        extent: size,
        rotation: 0,
        native_extent: extent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a GPU"]
    fn cached_preview_keeps_wide_colors_while_srgb_viewing_is_selected() {
        use foveonic::image::{ColorSpace, Space};
        let rs = crate::navigator::gpu_tests::state();
        crate::viewport::set_gamut(&rs, crate::preferences::DisplayGamut::Srgb);
        let source = crate::navigator::gpu_tests::texture(
            &rs,
            [1, 1],
            wgpu::TextureFormat::Rgba32Float,
            wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        rs.queue.write_texture(
            source.as_image_copy(),
            bytemuck::cast_slice(&[0_f32, 1., 0., 1.]),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(16),
                rows_per_image: None,
            },
            source.size(),
        );
        let preview =
            downsample_texture(&rs, &source, Space::ToneMappedRgb(ColorSpace::AdobeRgb)).unwrap();
        let prepared = crate::preview_image::PreparedPreview::new(preview, true).unwrap();
        let pixel = prepared.preview.unwrap().rgba[0];
        assert!(half::f16::from_bits(pixel[0]).to_f32() < -0.3);
        assert!(half::f16::from_bits(pixel[2]).to_f32() < -0.02);
    }
    #[test]
    #[ignore = "requires a GPU"]
    fn display_readback_preserves_color_and_removes_row_padding() {
        use foveonic::image::{ColorSpace, Space};
        let rs = crate::navigator::gpu_tests::state();
        let source = crate::navigator::gpu_tests::texture(
            &rs,
            [4, 2],
            wgpu::TextureFormat::Rgba32Float,
            wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let colors = [[0_f32, 0.5, 1., 1.], [1., 0.5, 0., 1.]]
            .into_iter()
            .flat_map(|pixel| [pixel; 4])
            .collect::<Vec<_>>();
        rs.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&colors),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * 16),
                rows_per_image: Some(2),
            },
            source.size(),
        );
        let preview =
            downsample_texture(&rs, &source, Space::ToneMappedRgb(ColorSpace::Srgb)).unwrap();
        assert_eq!(preview.extent, [4, 2]);
        assert_eq!(preview.rgb.len(), 4 * 2 * 3);
        let linear = preview.linear_image().unwrap();
        let matrix =
            foveonic::color_space::TiffColorSpace::Srgb.transform(preview.color_space.into());
        for (row, expected) in linear
            .as_raw()
            .as_chunks::<12>()
            .0
            .iter()
            .zip([[0., 0.5, 1.], [1., 0.5, 0.]])
        {
            for pixel in row.as_chunks::<3>().0 {
                for c in 0..3 {
                    let actual: f32 = (0..3).map(|i| matrix[c][i] * pixel[i]).sum();
                    assert!(
                        (actual - expected[c]).abs() < 0.002,
                        "{actual} != {}",
                        expected[c]
                    );
                }
            }
        }
    }
}
