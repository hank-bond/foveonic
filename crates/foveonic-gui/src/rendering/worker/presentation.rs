//! Reframes one immutable whole-image result without reopening sources or running photographic nodes.

use super::Comparison;
use crate::rendering::request::RenderRequest;
#[cfg(test)]
use crate::rendering::request::RequestId;
use crate::{
    editor::Rendition,
    export::resolve_camera_settings,
    rendering::matching::{same_calibration_snapshot, same_pixel_settings},
};
use foveonic::render::ProcessingQuality;
use foveonic::{
    decode_cache::Cache as DecodeCache,
    image::{ImageTexture, RenderedImage},
    inspection::{Artifact, Content},
    render::{Scale, Settings},
    source::{Metadata, Result, SourceOrigin},
};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone)]
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
#[derive(Clone)]
struct Whole {
    path: PathBuf,
    root: String,
    rendition: Rendition,
    comparison: bool,
    calibration: Option<Arc<foveonic::noise_calibration::Calibration>>,
    policy: ProcessingQuality,
    settings: Settings,
    framing: bool,
    identity: SourceOrigin,
    images: Vec<Image>,
    display: Option<Display>,
}
impl Whole {
    fn memory_resources(&self, inventory: &mut crate::memory::Inventory) {
        for image in &self.images {
            inventory.image(&image.image);
            inventory.metadata(&image.metadata);
        }
        if let Some(display) = &self.display {
            inventory.image_texture(&display.texture);
            if let Some(comparison) = &display.comparison {
                inventory.image_texture(&comparison.texture);
            }
        }
    }
}
#[derive(Clone)]
pub(crate) struct Snapshot(Whole);
impl Snapshot {
    pub fn origin(&self) -> &SourceOrigin {
        &self.0.identity
    }
    pub fn from_frame(frame: &super::RenderResult, request: &RenderRequest) -> Option<Self> {
        if request.detail
            || request.comparison
            || frame.artifact.settings.preview_region.is_some()
            || frame.artifact.settings.output_size.is_some()
        {
            return None;
        }
        let Content::Image(image) = &frame.artifact.content else {
            return None;
        };
        Some(Self(Whole {
            path: frame.request.path.clone(),
            root: frame.request.output.clone(),
            rendition: frame.request.rendition,
            comparison: false,
            calibration: request.calibration.clone(),
            policy: request.processing_quality,
            settings: frame.artifact.settings.clone(),
            framing: frame.request.framing_preview,
            identity: frame.origin.clone(),
            images: vec![Image {
                name: frame.artifact.name,
                dehaze_estimate: frame.artifact.dehaze_estimate,
                metadata: frame.artifact.metadata.clone(),
                image: (**image).clone(),
            }],
            display: Some(Display {
                texture: frame.texture.clone(),
                comparison: None,
                framing: frame.request.framing_preview,
            }),
        }))
    }
    pub fn memory_resources(&self, inventory: &mut crate::memory::Inventory) {
        self.0.memory_resources(inventory);
    }
}
#[derive(Default)]
pub(super) struct Presentation {
    whole: Option<Whole>,
}
pub(super) struct Prepared {
    pub metadata: Arc<Metadata>,
    pub origin: SourceOrigin,
    pub working_scale: Scale,
    pub artifacts: Vec<Artifact>,
    pub display: Option<Display>,
}
impl Presentation {
    pub fn seed(&mut self, snapshot: &Snapshot) {
        self.whole = Some(snapshot.0.clone());
    }
    pub fn memory_resources(&self, inventory: &mut crate::memory::Inventory) {
        if let Some(whole) = &self.whole {
            whole.memory_resources(inventory);
        }
    }
    pub fn clear(&mut self) {
        self.whole = None;
    }
    /// Pixel edits can be pending when composition changes. They still need a
    /// render, using the existing working grid rather than a new framing policy.
    pub fn quality_for_view_change(
        &self,
        request: &RenderRequest,
        cache: &DecodeCache,
    ) -> Option<Scale> {
        if request.detail {
            return None;
        }
        let whole = self.whole.as_ref()?;
        if cache
            .source_origin(&request.path)
            .is_some_and(|current| current != whole.identity)
        {
            return None;
        }
        let mut settings = request.settings.clone();
        resolve_camera_settings(&mut settings, &whole.images[0].metadata);
        let changed = whole.framing != request.framing_preview
            || whole.settings.crop != settings.crop
            || whole.settings.rotation != settings.rotation;
        if !changed
            && !request
                .accepts_scale(
                    whole.images[0].image.settings().working_scale,
                    &whole.images[0].metadata,
                )
                .ok()?
            && same_pixel_settings(&whole.settings, &settings)
        {
            return None;
        }
        (request.settings.preview_region.is_none()
            && request.settings.output_size.is_none()
            && whole.path == request.path
            && whole.root == request.output
            && whole.rendition == request.rendition
            && whole.comparison == request.comparison
            && whole.policy == request.processing_quality
            && same_calibration_snapshot(&whole.calibration, &request.calibration)
            && ((whole.framing && request.framing_preview)
                || whole.framing != request.framing_preview
                || whole.settings.crop != request.settings.crop
                || whole.settings.rotation != request.settings.rotation))
            .then_some(whole.images[0].image.settings().working_scale)
    }
    pub fn prepare(
        &self,
        request: &RenderRequest,
        cache: &DecodeCache,
    ) -> Result<Option<Prepared>> {
        let Some(whole) = &self.whole else {
            return Ok(None);
        };
        if request.settings.preview_region.is_some()
            || request.settings.output_size.is_some()
            || whole.path != request.path
            || whole.root != request.output
            || whole.rendition != request.rendition
            || whole.comparison != request.comparison
            || whole.policy != request.processing_quality
            || !same_calibration_snapshot(&whole.calibration, &request.calibration)
        {
            return Ok(None);
        }
        if cache
            .source_origin(&request.path)
            .is_some_and(|current| current != whole.identity)
        {
            return Ok(None);
        }
        let mut settings = request.settings.clone();
        resolve_camera_settings(&mut settings, &whole.images[0].metadata);
        if !same_pixel_settings(&whole.settings, &settings) {
            return Ok(None);
        }
        let changed = whole.framing != request.framing_preview
            || whole.settings.crop != settings.crop
            || whole.settings.rotation != settings.rotation;
        // Display demand can increase without changing photographic controls.
        // Composition must retain the already rendered grid.
        if (request.detail || !changed)
            && !request.accepts_scale(
                whole.images[0].image.settings().working_scale,
                &whole.images[0].metadata,
            )?
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
            origin: whole.identity.clone(),
            working_scale: whole.images[0].image.settings().working_scale,
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
        request: &RenderRequest,
        settings: &Settings,
        artifacts: &[Artifact],
        origin: &SourceOrigin,
    ) -> bool {
        if request.detail
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
            policy: request.processing_quality,
            settings: settings.clone(),
            framing: request.framing_preview,
            identity: origin.clone(),
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
    use crate::rendering::worker::{tests::TestWorker, RenderResult};
    use eframe::{egui, wgpu};
    use foveonic::{profile::Profile, render::Engine, source::Source};
    use std::sync::atomic::Ordering;

    fn request(
        path: &std::path::Path,
        revision: u64,
        pass: Scale,
        policy: ProcessingQuality,
        settings: &Settings,
        framing: bool,
    ) -> RenderRequest {
        RenderRequest {
            path: path.to_owned(),
            id: RequestId(revision),
            settings: Settings {
                processing_quality: policy,
                ..settings.clone()
            },
            preview_scale: pass,
            processing_quality: policy,
            detail: settings.preview_region.is_some(),
            framing_preview: framing,
            calibration: None,
            rendition: Default::default(),
            output: "final".into(),
            comparison: false,
        }
    }
    fn receive(worker: &TestWorker, revision: u64) -> RenderResult {
        loop {
            let reply = super::super::tests::receive(worker);
            if reply.request.id == RequestId(revision) {
                return reply.result.unwrap_rendered();
            }
        }
    }
    fn dispatches(worker: &TestWorker, revision: u64) -> u64 {
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
    #[test]
    #[ignore = "requires GPU and pinned Merrill FOVEONIC_FIXTURE_DIR original"]
    fn retained_snapshots_and_presentation_count_shared_frame_resources_once() {
        use foveonic::render::WorkingSet;
        use sha2::{Digest, Sha256};

        let manifest: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../scripts/profiling/wide_inputs.json"
        ))
        .unwrap();
        let entry = &manifest["files"][0];
        let path = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join(entry["file"].as_str().unwrap());
        assert_eq!(
            format!("{:x}", Sha256::digest(std::fs::read(&path).unwrap())),
            entry["sha256"].as_str().unwrap()
        );
        let engine = Engine::new().unwrap();
        let verifier = engine.executor();
        let worker = TestWorker::new(engine, egui::Context::default());
        let request = request(
            &path,
            1,
            Scale::new(8).unwrap(),
            ProcessingQuality::Speed,
            &Settings::new("Sunlight", "Standard"),
            false,
        );
        worker.submit(request.clone());
        let frame = receive(&worker, request.id.0);
        let snapshot = Snapshot::from_frame(&frame, &request).unwrap();
        let mut presentation = Presentation::default();
        presentation.seed(&snapshot);

        let working = WorkingSet::new();
        let resources = |inventory: &crate::memory::Inventory| {
            verifier.memory_resources(&working, inventory.reservation())
        };
        let empty = resources(&crate::memory::Inventory::default());
        let mut expected = crate::memory::Inventory::default();
        expected.frame(&frame);
        let expected = resources(&expected);
        assert!(expected.cpu_bytes > empty.cpu_bytes);
        assert!(expected.gpu_bytes > empty.gpu_bytes);
        assert_eq!(
            expected.reserved_bytes,
            crate::memory::texture_bytes(frame.texture.texture())
        );

        let mut retained = crate::memory::Inventory::default();
        snapshot.memory_resources(&mut retained);
        assert_eq!(resources(&retained), expected);
        snapshot.clone().memory_resources(&mut retained);
        presentation.memory_resources(&mut retained);
        retained.frame(&frame);
        assert_eq!(resources(&retained), expected);
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
        engine
            .device()
            .poll(wgpu::PollType::Wait)
            .expect("GPU polling failed");
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
    fn matches_fresh(
        engine: &Engine,
        frame: &RenderResult,
        path: &std::path::Path,
        settings: &Settings,
    ) {
        let mut settings = settings.clone();
        settings.working_scale = frame.artifact.settings.working_scale;
        settings.processing_quality = frame.processing_quality();
        settings.preview_region = None;
        let fresh = engine
            .render(
                &Source::open(path).unwrap(),
                &settings,
                &mut Profile::default(),
            )
            .unwrap();
        let image = if frame.request.framing_preview {
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
    #[ignore = "requires GPU and MERRILL_X3F"]
    fn smart_resolution_upgrades_cannot_reuse_coarser_unchanged_pixels() {
        let path = PathBuf::from(std::env::var_os("MERRILL_X3F").unwrap());
        let engine = Engine::new().unwrap();
        let verifier = engine.executor();
        let worker = TestWorker::new(engine, egui::Context::default());
        let settings = Settings::new("Sunlight", "Standard");
        for (index, quality) in [
            Scale::new(8).unwrap(),
            Scale::new(4).unwrap(),
            Scale::new(2).unwrap(),
            Scale::NATIVE,
        ]
        .into_iter()
        .enumerate()
        {
            let revision = index as u64 + 1;
            worker.submit(request(
                &path,
                revision,
                quality,
                ProcessingQuality::Quality,
                &settings,
                false,
            ));
            let frame = receive(&worker, revision);
            assert_eq!(frame.artifact.settings.working_scale, quality);
            assert!(dispatches(&worker, revision) > 0);
            matches_fresh(&verifier, &frame, &path, &settings);
        }
    }
    #[test]
    #[ignore = "requires GPU and MERRILL_X3F"]
    fn whole_detail_stays_native_after_reduced_composition_changes() {
        let path = PathBuf::from(std::env::var_os("MERRILL_X3F").unwrap());
        let engine = Engine::new().unwrap();
        let verifier = engine.executor();
        let worker = TestWorker::new(engine, egui::Context::default());
        let mut settings = Settings::new("Sunlight", "Standard");
        worker.submit(request(
            &path,
            1,
            Scale::new(8).unwrap(),
            ProcessingQuality::Speed,
            &settings,
            false,
        ));
        let base = receive(&worker, 1);
        assert!(base.artifact.settings.working_scale.is_reduced());
        settings.crop.bounds = [0.1, 0.1, 0.9, 0.9];
        settings.rotation.turns = 1;
        let mut detail = request(
            &path,
            2,
            Scale::NATIVE,
            ProcessingQuality::Speed,
            &settings,
            false,
        );
        detail.detail = true;
        worker.submit(detail);
        let frame = receive(&worker, 2);
        assert!(frame.request.detail);
        assert!(!frame.is_photographic());
        assert_eq!(frame.artifact.settings.working_scale, Scale::NATIVE);
        assert_eq!(frame.artifact.settings.preview_region, None);
        assert!(dispatches(&worker, 2) > 0);
        matches_fresh(&verifier, &frame, &path, &settings);
        worker.submit(request(
            &path,
            3,
            Scale::new(8).unwrap(),
            ProcessingQuality::Speed,
            &settings,
            false,
        ));
        let main = receive(&worker, 3);
        assert!(!main.request.detail);
        assert_eq!(main.artifact.settings.working_scale, Scale::new(8).unwrap());
        matches_fresh(&verifier, &main, &path, &settings);
    }

    #[test]
    #[ignore = "requires GPU and MERRILL_X3F"]
    fn crop_and_rotate_reuse_thumb_smart_fit_and_smart_full_without_numerical_dispatches() {
        let path = PathBuf::from(std::env::var_os("MERRILL_X3F").unwrap());
        for policy in [ProcessingQuality::Speed, ProcessingQuality::Quality] {
            let engine = Engine::new().unwrap();
            let verifier = engine.executor();
            let worker = TestWorker::new(engine, egui::Context::default());
            let mut settings = Settings::new("Sunlight", "Standard");
            settings.reconstruction = Some(Default::default());
            worker.submit(request(
                &path,
                1,
                if policy == ProcessingQuality::Speed {
                    Scale::new(8).unwrap()
                } else {
                    Scale::new(2).unwrap()
                },
                policy,
                &settings,
                false,
            ));
            let base = receive(&worker, 1);
            let quality = base.artifact.settings.working_scale;
            assert_eq!(
                quality,
                if policy == ProcessingQuality::Speed {
                    Scale::new(8).unwrap()
                } else {
                    Scale::new(2).unwrap()
                }
            );
            let baseline_loads = worker.checks.source_loads.load(Ordering::Relaxed);
            if policy == ProcessingQuality::Quality {
                let mut region = settings.clone();
                region.preview_region = Some([100, 100, 400, 500]);
                worker.submit(request(&path, 2, Scale::NATIVE, policy, &region, false));
                let tile = receive(&worker, 2);
                assert!(tile.artifact.settings.preview_region.is_some());
            }
            let loads = worker.checks.source_loads.load(Ordering::Relaxed);
            println!("framing reuse setup {policy:?}: baseline source loads={baseline_loads}, after native setup={loads}");
            worker.submit(request(&path, 3, Scale::NATIVE, policy, &settings, true));
            let plane = receive(&worker, 3);
            assert_eq!(
                plane.artifact.settings.working_scale, quality,
                "a regional Full tile cannot become the whole plane"
            );
            assert!(plane.artifact.settings.preview_region.is_none());
            assert_eq!(dispatches(&worker, 3), 0);
            settings.crop.bounds = [0.1, 0.15, 0.9, 0.85];
            settings.rotation = foveonic::rotation::Rotation {
                turns: 1,
                degrees: 7.,
            };
            worker.submit(request(&path, 4, Scale::NATIVE, policy, &settings, true));
            let draft = receive(&worker, 4);
            assert!(
                Arc::ptr_eq(&plane.texture, &draft.texture),
                "drag/commit keeps the untrimmed texture"
            );
            assert_eq!(dispatches(&worker, 4), 0);
            // Commit at the displayed grid before explicitly asking for native pixels.
            worker.submit(request(&path, 5, quality, policy, &settings, false));
            let committed = receive(&worker, 5);
            assert_eq!(committed.artifact.settings.working_scale, quality);
            assert_eq!(dispatches(&worker, 5), 0);
            assert_eq!(worker.checks.source_loads.load(Ordering::Relaxed), loads);
            matches_fresh(&verifier, &committed, &path, &settings);
            // A deliberate zoom asks for a new Full pass despite unchanged controls.
            worker.submit(request(&path, 6, Scale::NATIVE, policy, &settings, false));
            let full = receive(&worker, 6);
            assert_eq!(full.artifact.settings.working_scale, Scale::NATIVE);
            assert!(dispatches(&worker, 6) > 0);
            matches_fresh(&verifier, &full, &path, &settings);
            let full_loads = worker.checks.source_loads.load(Ordering::Relaxed);
            worker.submit(request(&path, 7, Scale::NATIVE, policy, &settings, true));
            let full_plane = receive(&worker, 7);
            assert_eq!(full_plane.artifact.settings.working_scale, Scale::NATIVE);
            assert_eq!(dispatches(&worker, 7), 0);
            settings.rotation.degrees = -4.;
            worker.submit(request(&path, 8, Scale::NATIVE, policy, &settings, false));
            let full_composed = receive(&worker, 8);
            assert_eq!(full_composed.artifact.settings.working_scale, Scale::NATIVE);
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
        let worker = TestWorker::new(Engine::new().unwrap(), egui::Context::default());
        let settings = Settings::new("Sunlight", "Standard");
        worker.submit(request(
            &path,
            1,
            Scale::new(8).unwrap(),
            ProcessingQuality::Speed,
            &settings,
            true,
        ));
        assert_eq!(
            receive(&worker, 1).artifact.settings.working_scale,
            Scale::new(8).unwrap()
        );
        worker.submit(request(
            &path,
            2,
            Scale::NATIVE,
            ProcessingQuality::Speed,
            &settings,
            true,
        ));
        assert_eq!(
            receive(&worker, 2).artifact.settings.working_scale,
            Scale::NATIVE
        );
        assert!(dispatches(&worker, 2) > 0);
    }

    #[test]
    #[ignore = "requires GPU and MERRILL_X3F"]
    fn composition_keeps_pending_exposure_and_explicit_quality_changes_are_numerical() {
        let path = PathBuf::from(std::env::var_os("MERRILL_X3F").unwrap());
        for (pass, policy, initially_framing) in [
            (Scale::new(8).unwrap(), ProcessingQuality::Speed, false),
            (Scale::NATIVE, ProcessingQuality::Quality, true),
        ] {
            let engine = Engine::new().unwrap();
            let verifier = engine.executor();
            let worker = TestWorker::new(engine, egui::Context::default());
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
            let quality = base.artifact.settings.working_scale;
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
                Scale::new(2).unwrap(),
                policy,
                &settings,
                !initially_framing,
            ));
            let frame = receive(&worker, 3);
            assert_eq!(frame.artifact.settings.exposure, 0.5);
            assert_eq!(
                frame.artifact.settings.working_scale, quality,
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
                Scale::NATIVE,
                ProcessingQuality::Quality,
                &settings,
                true,
            ));
            let changed = receive(&worker, 4);
            assert_eq!(changed.artifact.settings.working_scale, Scale::NATIVE);
            let quality_dispatches = dispatches(&worker, 4);
            if policy == ProcessingQuality::Speed {
                assert!(
                    quality_dispatches > 0,
                    "changing Speed to Quality requires native pixels"
                );
            } else {
                assert_eq!(
                    quality_dispatches, 0,
                    "an already-Quality native image is reused"
                );
            }
            matches_fresh(&verifier, &changed, &path, &settings);
        }
    }
    #[test]
    #[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR"]
    fn same_path_replacement_does_not_reactivate_old_reduced_metadata() {
        let fixtures = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("photo.x3f");
        std::fs::copy(fixtures.join("blue-parking-sign.x3f"), &path).unwrap();
        let engine = Engine::new().unwrap();
        let verifier = engine.executor();
        let worker = TestWorker::new(engine, egui::Context::default());
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.reconstruction = Some(Default::default());
        worker.submit(request(
            &path,
            1,
            Scale::new(8).unwrap(),
            ProcessingQuality::Speed,
            &settings,
            false,
        ));
        let first = receive(&worker, 1);
        assert_eq!(worker.checks.source_loads.load(Ordering::Relaxed), 1);
        std::fs::copy(fixtures.join("merrill-hazy-coast.x3f"), &path).unwrap();
        worker.submit(request(
            &path,
            2,
            Scale::NATIVE,
            ProcessingQuality::Quality,
            &settings,
            false,
        ));
        let native = receive(&worker, 2);
        assert_eq!(worker.checks.source_loads.load(Ordering::Relaxed), 2);
        matches_fresh(&verifier, &native, &path, &settings);
        worker.submit(request(
            &path,
            3,
            Scale::new(8).unwrap(),
            ProcessingQuality::Speed,
            &settings,
            false,
        ));
        let changed = receive(&worker, 3);
        assert_eq!(
            worker.checks.source_loads.load(Ordering::Relaxed),
            2,
            "new native source transfers into the reduced owner without another decode"
        );
        matches_fresh(&verifier, &changed, &path, &settings);
        assert_ne!(
            pixels(&verifier, &first.texture),
            pixels(&verifier, &changed.texture)
        );
    }
}

#[cfg(test)]
mod completed_sampling_tests {
    use super::*;
    use crate::rendering::{
        coordinator::{RenderCoordinator, Target},
        worker::{tests::TestWorker, RenderResult},
    };
    use std::time::Instant;

    #[test]
    #[ignore = "requires GPU and MERRILL_X3F"]
    fn completed_finer_snapshots_seed_new_sampling_tasks_without_changing_inflight_matching() {
        let path = PathBuf::from(std::env::var_os("MERRILL_X3F").unwrap());
        let worker = TestWorker::new(
            foveonic::render::Engine::new().unwrap(),
            eframe::egui::Context::default(),
        );
        let first = RenderRequest {
            detail: false,
            framing_preview: false,
            calibration: None,
            rendition: Rendition::Current,
            id: RequestId(1),
            path: path.clone(),
            settings: Settings {
                processing_quality: ProcessingQuality::Speed,
                ..Settings::new("Sunlight", "Standard")
            },
            preview_scale: Scale::new(4).unwrap(),
            processing_quality: ProcessingQuality::Speed,
            output: "final".into(),
            comparison: false,
        };
        worker.submit(first.clone());
        let frame: RenderResult = super::super::tests::receive(&worker)
            .result
            .unwrap_rendered();
        let seeded = |next: RenderRequest, origin: SourceOrigin| {
            let mut coordinator = RenderCoordinator::default();
            let target = Target::of(&first);
            coordinator.desire(
                Arc::new(first.clone()),
                Some(frame.origin.clone()),
                Instant::now(),
                false,
            );
            let original = coordinator.desired(&target).unwrap().stamp.clone();
            coordinator.finish(&original);
            assert!(coordinator.admit(&original, &frame.origin));
            coordinator.note_pixels(&original, &frame);
            let target = Target::of(&next);
            coordinator.desire(Arc::new(next), Some(origin), Instant::now(), false);
            let task = coordinator.desired(&target).unwrap().clone();
            assert_ne!(task.stamp.id, original.id);
            (coordinator, task)
        };
        let cache = DecodeCache::new();
        let mut coarser = first.clone();
        coarser.id = RequestId(2);
        coarser.preview_scale = Scale::new(8).unwrap();
        let (_, task) = seeded(coarser.clone(), frame.origin.clone());
        let mut presenter = Presentation::default();
        presenter.seed(
            task.seed
                .as_deref()
                .expect("completed pixels seed a new logical sampling task"),
        );
        let prepared = presenter.prepare(&task.request, &cache).unwrap().unwrap();
        assert_eq!(prepared.working_scale, Scale::new(4).unwrap());
        assert!(Arc::ptr_eq(
            &prepared.display.unwrap().texture,
            &frame.texture
        ));

        for (scale, policy, exposure) in [
            (Scale::new(2).unwrap(), ProcessingQuality::Speed, 0.),
            (Scale::new(8).unwrap(), ProcessingQuality::Quality, 0.),
            (Scale::new(8).unwrap(), ProcessingQuality::Speed, 0.25),
        ] {
            let mut next = coarser.clone();
            next.preview_scale = scale;
            next.processing_quality = policy;
            next.settings.processing_quality = policy;
            next.settings.exposure = exposure;
            let (_, task) = seeded(next, frame.origin.clone());
            let mut presenter = Presentation::default();
            presenter.seed(task.seed.as_deref().unwrap());
            assert!(presenter.prepare(&task.request, &cache).unwrap().is_none());
        }

        let replaced = SourceOrigin::default();
        let (mut coordinator, task) = seeded(coarser.clone(), replaced.clone());
        assert!(task.seed.is_none());
        let mut finer_replacement = coarser.clone();
        finer_replacement.id = RequestId(3);
        finer_replacement.preview_scale = Scale::new(2).unwrap();
        let target = Target::of(&finer_replacement);
        coordinator.desire(
            Arc::new(finer_replacement),
            Some(replaced),
            Instant::now(),
            false,
        );
        assert!(
            coordinator.desired(&target).unwrap().seed.is_none(),
            "a pending replacement must not revive an earlier source's snapshot"
        );

        worker.submit(coarser);
        let retained = super::super::tests::receive(&worker)
            .result
            .unwrap_rendered();
        assert_eq!(
            retained.artifact.settings.working_scale,
            Scale::new(4).unwrap()
        );
        assert!(Arc::ptr_eq(&retained.texture, &frame.texture));
        assert_eq!(
            worker
                .checks
                .recipes
                .lock()
                .unwrap()
                .iter()
                .find(|entry| entry.0 == 2)
                .unwrap()
                .1,
            0,
            "relaxing sampling must not dispatch photographic work"
        );
    }
}
