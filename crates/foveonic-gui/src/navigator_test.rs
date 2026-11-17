//! Checks navigator mapping, preparation cost and restoration of cached photographic sources.

use super::*;
pub(crate) fn state() -> egui_wgpu::RenderState {
    state_for(wgpu::TextureFormat::Rgba8UnormSrgb)
}
pub(crate) fn state_for(format: wgpu::TextureFormat) -> egui_wgpu::RenderState {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Navigator test"),
        required_features: wgpu::Features::empty(),
        required_limits: adapter.limits(),
        memory_hints: Default::default(),
        trace: wgpu::Trace::Off,
    }))
    .unwrap();
    let renderer = egui_wgpu::Renderer::new(&device, format, None, 1, false);
    let rs = egui_wgpu::RenderState {
        available_adapters: vec![adapter.clone()],
        adapter,
        device,
        queue,
        target_format: format,
        renderer: Arc::new(egui::epaint::mutex::RwLock::new(renderer)),
    };
    viewport::install(&rs);
    rs
}
pub(crate) fn texture(
    rs: &egui_wgpu::RenderState,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    rs.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Navigator fixture"),
        size: wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}
#[test]
#[ignore = "requires a GPU"]
fn gpu_mapping_matches_main_presentation_for_layers_rgb_ratios_signed_and_channels() {
    let rs = state();
    let source = texture(
        &rs,
        [4, 4],
        wgpu::TextureFormat::Rgba32Float,
        wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let target = texture(
        &rs,
        [4, 4],
        wgpu::TextureFormat::Rgba16Float,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    for (space, value, channel, expected) in [
        (Space::SensorLayers, [4095_f32, 2047.5, 0., 1.], -1, 255u8),
        (
            Space::LinearRgb(foveonic::image::ColorSpace::AdobeRgb),
            [2047.5, 2047.5, 2047.5, 1.],
            -1,
            188,
        ),
        (
            Space::ToneMappedRgb(foveonic::image::ColorSpace::Srgb),
            [0.1, 0.5, 0.9, 1.],
            1,
            188,
        ),
        (
            Space::ToneMappedRgb(foveonic::image::ColorSpace::ProPhotoRgb),
            [0.5, 0.5, 0.5, 1.],
            -1,
            188,
        ),
        (Space::ScalarField, [0.5, 0.5, 0.5, 1.], -1, 128),
        (Space::LuminanceRatio, [1., 1., 1., 1.], -1, 128),
        (Space::SignedDetail, [0., 0., 0., 1.], -1, 128),
    ] {
        rs.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&[value; 16]),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(64),
                rows_per_image: Some(4),
            },
            source.size(),
        );
        viewport::prepare_preview(
            &rs,
            &source,
            space,
            &target,
            viewport::Mapping::normal(channel),
            None,
            None,
        );
        let buffer = rs.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Synthetic navigator pixels"),
            size: 1024,
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
                    bytes_per_row: Some(256),
                    rows_per_image: Some(4),
                },
            },
            target.size(),
        );
        rs.queue.submit(Some(encoder.finish()));
        let (tx, rx) = mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                tx.send(result).unwrap();
            });
        rs.device
            .poll(wgpu::PollType::Wait)
            .expect("GPU polling failed");
        rx.recv().unwrap().unwrap();
        let bytes = buffer.slice(..).get_mapped_range();
        for c in 0..3 {
            let linear = half::f16::from_le_bytes([bytes[c * 2], bytes[c * 2 + 1]]).to_f32();
            let encoded = (foveonic::preview::encode_sample(
                foveonic::color_space::TiffColorSpace::Srgb,
                linear,
            ) / 257) as u8;
            assert!(
                (i32::from(encoded) - i32::from(expected)).abs() <= 1,
                "{space:?} {channel} {c}: {}",
                bytes[c]
            );
        }
        assert_eq!(half::f16::from_le_bytes([bytes[6], bytes[7]]).to_f32(), 1.);
        drop(bytes);
        buffer.unmap();
    }
    let target = texture(
        &rs,
        [512, 341],
        wgpu::TextureFormat::Rgba16Float,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    viewport::prepare_preview(
        &rs,
        &source,
        Space::ScalarField,
        &target,
        viewport::Mapping::normal(-1),
        None,
        None,
    );
    rs.device
        .poll(wgpu::PollType::Wait)
        .expect("GPU polling failed");
    let start = Instant::now();
    for _ in 0..25 {
        viewport::prepare_preview(
            &rs,
            &source,
            Space::ScalarField,
            &target,
            viewport::Mapping::normal(-1),
            None,
            None,
        );
        rs.device
            .poll(wgpu::PollType::Wait)
            .expect("GPU polling failed");
    }
    eprintln!(
        "NAVIGATOR-PREP completed_average_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000. / 25.
    );
}

/// A retained photograph remains a valid navigator source after newer alternate views.
#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR with pinned originals"]
fn cached_current_sources_restore_after_newer_views_and_photo_switches() {
    use crate::{
        editor::Rendition,
        rendering::{
            delivery::ViewState,
            request::Request,
            worker::{Frame, Update, Worker},
        },
    };
    use foveonic::render::{Engine, ProcessingQuality, Scale, Settings};
    use sha2::{Digest, Sha256};
    use std::path::Path;

    fn pinned(name: &str) -> PathBuf {
        let directory = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        let path = directory.join(name);
        let inputs: serde_json::Value =
            serde_json::from_str(include_str!("../../../scripts/profiling/test_inputs.json"))
                .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            inputs["sample_hashes"][name].as_str().unwrap(),
            "pinned navigator original: {}",
            path.display()
        );
        path
    }

    fn render(
        worker: &Worker,
        path: &Path,
        revision: u64,
        rendition: Rendition,
        output: &str,
    ) -> Frame {
        let settings = Settings {
            exposure: if rendition == Rendition::Current {
                if revision == 1 {
                    0.2
                } else {
                    0.7
                }
            } else {
                0.
            },
            output_size: Some([32, 48]),
            ..Default::default()
        };
        worker.set_view(ViewState {
            processing_quality: ProcessingQuality::Quality,
            active: Some(path.to_owned()),
            rendition,
            output: output.into(),
            comparison: false,
            framing_preview: false,
            exporting: false,
            calibration: None,
            settings: settings.clone(),
            overviews: Vec::new(),
            displayed_revision: None,
            detail_revision: None,
        });
        worker.submit(Request {
            auxiliary_preview: false,
            detail: false,
            framing_preview: false,
            calibration: None,
            rendition,
            revision,
            path: path.to_owned(),
            settings,
            preview_scale: Scale::new(8).unwrap(),
            processing_quality: ProcessingQuality::Quality,
            output: output.into(),
            comparison: false,
            export: None,
        });
        let frame = loop {
            if let Update::Image(reply) = worker
                .replies
                .recv_timeout(Duration::from_secs(30))
                .unwrap()
            {
                assert_eq!(reply.revision, revision);
                break reply.result.unwrap().expect("navigator source pixels");
            }
        };
        worker.progress.finish(revision, false);
        worker.delivered(revision);
        let deadline = Instant::now() + Duration::from_secs(5);
        while worker.busy() {
            assert!(Instant::now() < deadline, "source render did not settle");
            std::thread::sleep(Duration::from_millis(1));
        }
        frame
    }

    fn settle(navigator: &mut Navigator, frame: &Frame, context: &egui::Context) -> bool {
        let mapping = viewport::Mapping::normal(-1);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            navigator.update(Some(frame), mapping, false, context);
            if navigator.ready(frame, mapping) {
                assert!(navigator.image(frame, mapping).id.is_some());
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    let rs = state();
    let context = egui::Context::default();
    let engine = Engine::from_device(
        rs.device.clone(),
        rs.queue.clone(),
        rs.adapter.get_info().name,
    )
    .unwrap();
    let worker = Worker::new(engine, context.clone());
    worker.cache.configure(foveonic::decode_cache::Settings {
        enabled: false,
        ..Default::default()
    });
    let first = pinned("blue-parking-sign.x3f");
    let second = pinned("merrill-hazy-coast.x3f");
    let older_current = render(&worker, &first, 1, Rendition::Current, "final");
    let current = render(&worker, &first, 6, Rendition::Current, "final");
    let camera = render(&worker, &first, 10, Rendition::Camera, "final");
    let inspection = render(&worker, &first, 11, Rendition::Current, "sensor.layers");
    let other = render(&worker, &second, 12, Rendition::Current, "final");
    let mapping = viewport::Mapping::normal(-1);
    let mut failures = Vec::new();
    let mut navigator = Navigator::new(rs.clone(), context.clone(), worker.cache.clone());
    for (label, frame) in [
        ("Current", &current),
        ("Camera", &camera),
        ("cached Current after Camera", &current),
        ("inspection", &inspection),
        ("cached Current after inspection", &current),
        ("another photograph", &other),
        ("cached Current after another photograph", &current),
    ] {
        if !settle(&mut navigator, frame, &context) {
            failures.push(format!(
                "{label}: {}",
                navigator.smoke_state(Some(frame), mapping)
            ));
        }
    }
    drop(navigator);

    // A completed obsolete reply must not replace the currently requested image.
    let mut navigator = Navigator::new(rs.clone(), context.clone(), worker.cache.clone());
    assert!(settle(&mut navigator, &current, &context));
    let (late, controlled) = mpsc::channel();
    let original_replies = std::mem::replace(&mut navigator.replies, controlled);
    let current_id = navigator.image(&current, mapping).id;
    for frame in [&older_current, &camera, &inspection, &other] {
        let key = Key::new(frame, mapping);
        let tile = texture(
            &rs,
            tile_extent(key.extent),
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        viewport::prepare_preview(
            &rs,
            frame.texture.texture(),
            key.space,
            &tile,
            mapping,
            None,
            None,
        );
        late.send(Tile {
            key,
            texture: tile,
            memory: None,
        })
        .unwrap();
        navigator.update(Some(&current), mapping, false, &context);
        assert!(
            navigator.ready(&current, mapping),
            "obsolete reply replaced cached Current"
        );
        assert_eq!(
            navigator.image(&current, mapping).id,
            current_id,
            "obsolete reply replaced the retained Current texture"
        );
    }
    drop(original_replies);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
