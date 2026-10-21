//! Checks GPU navigator transfer mapping and completed preparation cost using synthetic floating textures.

use super::*;
pub(super) fn state() -> egui_wgpu::RenderState {
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("Navigator test"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: Default::default(),
        },
        None,
    ))
    .unwrap();
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
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
pub(super) fn texture(
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
        wgpu::TextureFormat::Rgba8UnormSrgb,
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
        viewport::prepare_navigator(
            &rs,
            &source,
            space,
            &target,
            viewport::Mapping::normal(channel),
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
        rs.device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().unwrap();
        let bytes = buffer.slice(..).get_mapped_range();
        for c in 0..3 {
            assert!(
                (i32::from(bytes[c]) - i32::from(expected)).abs() <= 1,
                "{space:?} {channel} {c}: {}",
                bytes[c]
            );
        }
        assert_eq!(bytes[3], 255);
        drop(bytes);
        buffer.unmap();
    }
    let target = texture(
        &rs,
        [512, 341],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    viewport::prepare_navigator(
        &rs,
        &source,
        Space::ScalarField,
        &target,
        viewport::Mapping::normal(-1),
        None,
    );
    rs.device.poll(wgpu::Maintain::Wait);
    let start = Instant::now();
    for _ in 0..25 {
        viewport::prepare_navigator(
            &rs,
            &source,
            Space::ScalarField,
            &target,
            viewport::Mapping::normal(-1),
            None,
        );
        rs.device.poll(wgpu::Maintain::Wait);
    }
    eprintln!(
        "NAVIGATOR-PREP completed_average_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000. / 25.
    );
}
