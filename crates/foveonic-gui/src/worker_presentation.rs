//! Reframes one immutable whole-image result without reopening sources or running photographic nodes.

use super::{Comparison, Request};
use crate::{editor::Rendition, export::resolve_camera_settings, preferences::PreviewMode};
use foveonic::{
    decode_cache::Cache as DecodeCache,
    image::{ImageTexture, RenderedImage},
    importing::Identity,
    inspection::{Artifact, Content},
    render::{PreviewQuality, Settings},
    source::{Metadata, Result},
};
use std::{path::PathBuf, sync::Arc};

/// Crop and rotation change the view. Existing workspace grid flags and range
/// names retain the same normalization used for Smart's pixel revisions.
pub(super) fn same_pixels(a: &Settings, b: &Settings) -> bool {
    let mut b = b.clone();
    b.crop = a.crop;
    b.rotation = a.rotation;
    crate::smart::same_edits(a, &b)
}
fn same_calibration(
    a: &Option<Arc<foveonic::noise_calibration::Calibration>>,
    b: &Option<Arc<foveonic::noise_calibration::Calibration>>,
) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}
pub(super) fn same_context(a: &Request, b: &Request) -> bool {
    a.path == b.path
        && a.output == b.output
        && a.rendition == b.rendition
        && a.comparison == b.comparison
        && a.preview_policy == b.preview_policy
        && a.export.is_none()
        && b.export.is_none()
        && same_calibration(&a.calibration, &b.calibration)
}
pub(super) fn presentation_change(a: &Request, b: &Request) -> bool {
    a.framing_preview != b.framing_preview
        || a.settings.crop != b.settings.crop
        || a.settings.rotation != b.settings.rotation
}

struct Image {
    name: &'static str,
    dehaze_estimate: Option<[f32; 3]>,
    metadata: Arc<Metadata>,
    image: RenderedImage,
}
impl Image {
    fn compose(&self, settings: &Settings) -> Result<Artifact> {
        let image = self
            .image
            .with_composition(settings.crop, settings.rotation)?;
        Ok(Artifact {
            name: self.name,
            dehaze_estimate: self.dehaze_estimate,
            metadata: self.metadata.clone(),
            settings: image.settings().clone(),
            content: Content::Image(Box::new(image)),
        })
    }
}
#[derive(Clone)]
pub(super) struct Display {
    pub texture: Arc<ImageTexture>,
    pub comparison: Option<Comparison>,
    framing: bool,
}
struct Whole {
    path: PathBuf,
    root: String,
    rendition: Rendition,
    comparison: bool,
    calibration: Option<Arc<foveonic::noise_calibration::Calibration>>,
    policy: PreviewMode,
    settings: Settings,
    framing: bool,
    identity: Option<Identity>,
    images: Vec<Image>,
    display: Option<Display>,
}
#[derive(Default)]
pub(super) struct Presentation {
    whole: Option<Whole>,
}
pub(super) struct Prepared {
    pub metadata: Arc<Metadata>,
    pub quality: PreviewQuality,
    pub artifacts: Vec<Artifact>,
    pub display: Option<Display>,
}
impl Presentation {
    pub fn clear(&mut self) {
        self.whole = None;
    }
    /// Pixel edits can be pending when composition changes. They still need a
    /// render, using the existing working grid rather than a new framing policy.
    pub fn quality_for_view_change(&self, request: &Request) -> Option<PreviewQuality> {
        let whole = self.whole.as_ref()?;
        let mut settings = request.settings.clone();
        resolve_camera_settings(&mut settings, &whole.images[0].metadata);
        let changed = whole.framing != request.framing_preview
            || whole.settings.crop != settings.crop
            || whole.settings.rotation != settings.rotation;
        if !changed
            && request.preview_quality == PreviewMode::Full
            && whole.images[0].image.settings().preview_quality != PreviewQuality::Full
            && same_pixels(&whole.settings, &settings)
        {
            return None;
        }
        (request.export.is_none()
            && request.settings.preview_region.is_none()
            && request.settings.output_size.is_none()
            && whole.path == request.path
            && whole.root == request.output
            && whole.rendition == request.rendition
            && whole.comparison == request.comparison
            && whole.policy == request.preview_policy
            && same_calibration(&whole.calibration, &request.calibration)
            && ((whole.framing && request.framing_preview)
                || whole.framing != request.framing_preview
                || whole.settings.crop != request.settings.crop
                || whole.settings.rotation != request.settings.rotation))
            .then_some(whole.images[0].image.settings().preview_quality)
    }
    pub fn prepare(&self, request: &Request, cache: &DecodeCache) -> Result<Option<Prepared>> {
        let Some(whole) = &self.whole else {
            return Ok(None);
        };
        if request.export.is_some()
            || request.settings.preview_region.is_some()
            || request.settings.output_size.is_some()
            || whole.path != request.path
            || whole.root != request.output
            || whole.rendition != request.rendition
            || whole.comparison != request.comparison
            || whole.policy != request.preview_policy
            || !same_calibration(&whole.calibration, &request.calibration)
        {
            return Ok(None);
        }
        if let (Some(old), Some(current)) = (&whole.identity, cache.import_identity(&request.path))
        {
            if *old != current {
                return Ok(None);
            }
        }
        let mut settings = request.settings.clone();
        resolve_camera_settings(&mut settings, &whole.images[0].metadata);
        if !same_pixels(&whole.settings, &settings) {
            return Ok(None);
        }
        let changed = whole.framing != request.framing_preview
            || whole.settings.crop != settings.crop
            || whole.settings.rotation != settings.rotation;
        // A Smart refinement or explicit zoom can ask for a new pass without
        // changing controls. Composition must retain the already rendered grid.
        if !changed
            && request.preview_quality == PreviewMode::Full
            && whole.images[0].image.settings().preview_quality != PreviewQuality::Full
        {
            return Ok(None);
        }
        let display = whole
            .display
            .as_ref()
            .filter(|display| {
                (display.framing && request.framing_preview)
                    || (display.framing == request.framing_preview
                        && whole.settings.crop.bounds == settings.crop.bounds
                        && whole.settings.rotation == settings.rotation)
            })
            .cloned();
        Ok(Some(Prepared {
            metadata: whole.images[0].metadata.clone(),
            quality: whole.images[0].image.settings().preview_quality,
            artifacts: whole
                .images
                .iter()
                .map(|image| image.compose(&settings))
                .collect::<Result<_>>()?,
            display,
        }))
    }
    /// Keep one whole result with at most its main and comparison images. A
    /// regional Smart reply never replaces coverage with a partial buffer.
    pub fn retain(
        &mut self,
        request: &Request,
        settings: &Settings,
        artifacts: &[Artifact],
        cache: &DecodeCache,
    ) -> bool {
        if request.export.is_some()
            || artifacts.is_empty()
            || artifacts.len() > 2
            || artifacts.iter().any(|artifact| {
                artifact.settings.preview_region.is_some()
                    || artifact.settings.output_size.is_some()
                    || !matches!(artifact.content, Content::Image(_))
            })
        {
            return false;
        }
        let images = artifacts
            .iter()
            .map(|artifact| {
                let Content::Image(image) = &artifact.content else {
                    unreachable!()
                };
                Image {
                    name: artifact.name,
                    dehaze_estimate: artifact.dehaze_estimate,
                    metadata: artifact.metadata.clone(),
                    image: (**image).clone(),
                }
            })
            .collect();
        self.whole = Some(Whole {
            path: request.path.clone(),
            root: request.output.clone(),
            rendition: request.rendition,
            comparison: request.comparison,
            calibration: request.calibration.clone(),
            policy: request.preview_policy,
            settings: settings.clone(),
            framing: request.framing_preview,
            identity: cache.import_identity(&request.path),
            images,
            display: None,
        });
        true
    }
    pub fn display(
        &mut self,
        texture: Arc<ImageTexture>,
        comparison: Option<Comparison>,
        framing: bool,
    ) {
        if let Some(whole) = &mut self.whole {
            whole.display = Some(Display {
                texture,
                comparison,
                framing,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::{Frame, Update, Worker};
    use eframe::{egui, wgpu};
    use foveonic::{profile::Profile, render::Engine, source::Source};
    use std::sync::atomic::Ordering;

    fn request(
        path: &std::path::Path,
        revision: u64,
        pass: PreviewMode,
        policy: PreviewMode,
        settings: &Settings,
        framing: bool,
    ) -> Request {
        Request {
            path: path.to_owned(),
            revision,
            settings: settings.clone(),
            preview_quality: pass,
            preview_policy: policy,
            framing_preview: framing,
            calibration: None,
            rendition: Default::default(),
            output: "final".into(),
            comparison: false,
            export: None,
        }
    }
    fn receive(worker: &Worker, revision: u64) -> Frame {
        loop {
            if let Update::Image(reply) = worker
                .replies
                .recv_timeout(std::time::Duration::from_secs(30))
                .unwrap()
            {
                if reply.revision == revision {
                    return reply.result.unwrap().unwrap();
                }
            }
        }
    }
    fn dispatches(worker: &Worker, revision: u64) -> u64 {
        worker
            .checks
            .recipes
            .lock()
            .unwrap()
            .iter()
            .find(|entry| entry.0 == revision)
            .unwrap()
            .1
    }
    fn pixels(engine: &Engine, image: &ImageTexture) -> Vec<u8> {
        let texture = image.texture();
        let bytes = texture.width() * 16;
        let row = bytes.div_ceil(256) * 256;
        let buffer = engine.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("Composition comparison"),
            size: u64::from(row) * u64::from(texture.height()),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = engine.device().create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(texture.height()),
                },
            },
            texture.size(),
        );
        engine.queue().submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).unwrap()
            });
        engine.device().poll(wgpu::Maintain::Wait);
        receiver.recv().unwrap().unwrap();
        let data = buffer.slice(..).get_mapped_range();
        let result = data
            .chunks(row as usize)
            .flat_map(|row| row[..bytes as usize].iter().copied())
            .collect();
        drop(data);
        buffer.unmap();
        result
    }
    fn matches_fresh(engine: &Engine, frame: &Frame, path: &std::path::Path, settings: &Settings) {
        let mut settings = settings.clone();
        settings.preview_quality = frame.artifact.settings.preview_quality;
        settings.preview_region = None;
        let fresh = engine
            .render(
                &Source::open(path).unwrap(),
                &settings,
                &mut Profile::default(),
            )
            .unwrap();
        let image = if frame.framing_preview {
            fresh.without_composition()
        } else {
            fresh
        };
        let expected = engine
            .view_texture(&image, None, &mut Profile::default())
            .unwrap();
        assert_eq!(frame.texture.description(), expected.description());
        assert_eq!(pixels(engine, &frame.texture), pixels(engine, &expected));
    }
    #[test]
    fn pixel_boundary_preserves_control_source_and_workspace_policy_guards() {
        let path = std::path::Path::new("test.x3f");
        let settings = Settings::new("Sunlight", "Standard");
        let a = request(
            path,
            1,
            PreviewMode::Full,
            PreviewMode::Smart,
            &settings,
            false,
        );
        let mut b = request(
            path,
            2,
            PreviewMode::Smart,
            PreviewMode::Smart,
            &settings,
            true,
        );
        b.settings.crop.bounds = [0.1, 0.2, 0.9, 0.8];
        b.settings.rotation.degrees = 7.;
        assert!(
            same_context(&a, &b)
                && same_pixels(&a.settings, &b.settings)
                && presentation_change(&a, &b)
        );
        b.settings.exposure = 0.5;
        assert!(!same_pixels(&a.settings, &b.settings));
        b.settings = settings;
        b.preview_policy = PreviewMode::Full;
        assert!(!same_context(&a, &b));
        b.preview_policy = PreviewMode::Smart;
        b.path = "another.x3f".into();
        assert!(!same_context(&a, &b));
    }

    #[test]
    #[ignore = "requires GPU and MERRILL_X3F"]
    fn crop_and_rotate_reuse_thumb_smart_fit_and_smart_full_without_numerical_dispatches() {
        let path = PathBuf::from(std::env::var_os("MERRILL_X3F").unwrap());
        for policy in [PreviewMode::Thumb, PreviewMode::Smart] {
            let engine = Engine::new().unwrap();
            let verifier = engine.executor();
            let worker = Worker::new(engine, egui::Context::default());
            let mut settings = Settings::new("Sunlight", "Standard");
            settings.reconstruction = Some(Default::default());
            worker.submit(request(&path, 1, policy, policy, &settings, false));
            let base = receive(&worker, 1);
            let quality = base.artifact.settings.preview_quality;
            assert_eq!(
                quality,
                if policy == PreviewMode::Thumb {
                    PreviewQuality::Thumb
                } else {
                    PreviewQuality::Fast
                }
            );
            let loads = worker.checks.source_loads.load(Ordering::Relaxed);
            if policy == PreviewMode::Smart {
                let mut region = settings.clone();
                region.preview_region = Some([100, 100, 400, 500]);
                worker.submit(request(
                    &path,
                    2,
                    PreviewMode::Smart,
                    policy,
                    &region,
                    false,
                ));
                let tile = receive(&worker, 2);
                assert!(tile.artifact.settings.preview_region.is_some());
            }
            worker.submit(request(
                &path,
                3,
                PreviewMode::Full,
                policy,
                &settings,
                true,
            ));
            let plane = receive(&worker, 3);
            assert_eq!(
                plane.artifact.settings.preview_quality, quality,
                "a regional Full tile cannot become the whole plane"
            );
            assert!(plane.artifact.settings.preview_region.is_none());
            assert_eq!(dispatches(&worker, 3), 0);
            settings.crop.bounds = [0.1, 0.15, 0.9, 0.85];
            settings.rotation = foveonic::rotation::Rotation {
                turns: 1,
                degrees: 7.,
            };
            worker.submit(request(&path, 4, policy, policy, &settings, true));
            let draft = receive(&worker, 4);
            assert!(
                Arc::ptr_eq(&plane.texture, &draft.texture),
                "drag/commit keeps the untrimmed texture"
            );
            assert_eq!(dispatches(&worker, 4), 0);
            worker.submit(request(&path, 5, policy, policy, &settings, false));
            let committed = receive(&worker, 5);
            assert_eq!(committed.artifact.settings.preview_quality, quality);
            assert_eq!(dispatches(&worker, 5), 0);
            assert_eq!(worker.checks.source_loads.load(Ordering::Relaxed), loads);
            matches_fresh(&verifier, &committed, &path, &settings);
            // A deliberate zoom asks for a new Full pass despite unchanged controls.
            worker.submit(request(
                &path,
                6,
                PreviewMode::Full,
                policy,
                &settings,
                false,
            ));
            let full = receive(&worker, 6);
            assert_eq!(full.artifact.settings.preview_quality, PreviewQuality::Full);
            assert!(dispatches(&worker, 6) > 0);
            let full_loads = worker.checks.source_loads.load(Ordering::Relaxed);
            worker.submit(request(&path, 7, policy, policy, &settings, true));
            let full_plane = receive(&worker, 7);
            assert_eq!(
                full_plane.artifact.settings.preview_quality,
                PreviewQuality::Full
            );
            assert_eq!(dispatches(&worker, 7), 0);
            settings.rotation.degrees = -4.;
            worker.submit(request(&path, 8, policy, policy, &settings, false));
            let full_composed = receive(&worker, 8);
            assert_eq!(
                full_composed.artifact.settings.preview_quality,
                PreviewQuality::Full
            );
            assert_eq!(dispatches(&worker, 8), 0);
            assert_eq!(
                worker.checks.source_loads.load(Ordering::Relaxed),
                full_loads
            );
            matches_fresh(&verifier, &full_composed, &path, &settings);
        }
    }

    #[test]
    #[ignore = "requires GPU and MERRILL_X3F"]
    fn a_new_full_zoom_pass_while_framing_is_active_is_numerical() {
        let path = PathBuf::from(std::env::var_os("MERRILL_X3F").unwrap());
        let worker = Worker::new(Engine::new().unwrap(), egui::Context::default());
        let settings = Settings::new("Sunlight", "Standard");
        worker.submit(request(
            &path,
            1,
            PreviewMode::Thumb,
            PreviewMode::Thumb,
            &settings,
            true,
        ));
        assert_eq!(
            receive(&worker, 1).artifact.settings.preview_quality,
            PreviewQuality::Thumb
        );
        worker.submit(request(
            &path,
            2,
            PreviewMode::Full,
            PreviewMode::Thumb,
            &settings,
            true,
        ));
        assert_eq!(
            receive(&worker, 2).artifact.settings.preview_quality,
            PreviewQuality::Full
        );
        assert!(dispatches(&worker, 2) > 0);
    }

    #[test]
    #[ignore = "requires GPU and MERRILL_X3F"]
    fn composition_keeps_pending_exposure_and_explicit_quality_changes_are_numerical() {
        let path = PathBuf::from(std::env::var_os("MERRILL_X3F").unwrap());
        for (pass, policy, initially_framing) in [
            (PreviewMode::Thumb, PreviewMode::Thumb, false),
            (PreviewMode::Full, PreviewMode::Smart, true),
        ] {
            let engine = Engine::new().unwrap();
            let verifier = engine.executor();
            let worker = Worker::new(engine, egui::Context::default());
            let mut settings = Settings::new("Sunlight", "Standard");
            settings.reconstruction = Some(Default::default());
            worker.submit(request(
                &path,
                1,
                pass,
                policy,
                &settings,
                initially_framing,
            ));
            let base = receive(&worker, 1);
            let quality = base.artifact.settings.preview_quality;
            let loads = worker.checks.source_loads.load(Ordering::Relaxed);
            settings.exposure = 0.5;
            worker.submit(request(
                &path,
                2,
                pass,
                policy,
                &settings,
                initially_framing,
            ));
            settings.crop.bounds = [0.2, 0.1, 0.8, 0.9];
            settings.rotation.degrees = 5.;
            worker.submit(request(
                &path,
                3,
                policy,
                policy,
                &settings,
                !initially_framing,
            ));
            let frame = receive(&worker, 3);
            assert_eq!(frame.artifact.settings.exposure, 0.5);
            assert_eq!(
                frame.artifact.settings.preview_quality, quality,
                "composition must retain the working grid of pending pixel edits"
            );
            assert_eq!(worker.checks.source_loads.load(Ordering::Relaxed), loads);
            assert_eq!(
                worker
                    .checks
                    .recipes
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(revision, dispatches)| *revision >= 2 && *dispatches > 0)
                    .count(),
                1
            );
            matches_fresh(&verifier, &frame, &path, &settings);
            worker.submit(request(
                &path,
                4,
                PreviewMode::Full,
                PreviewMode::Full,
                &settings,
                true,
            ));
            let changed = receive(&worker, 4);
            assert_eq!(
                changed.artifact.settings.preview_quality,
                PreviewQuality::Full
            );
            assert!(
                dispatches(&worker, 4) > 0,
                "explicit workspace quality is a pixel request"
            );
        }
    }
}
