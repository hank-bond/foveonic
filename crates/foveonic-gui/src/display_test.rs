//! Checks wide-gamut output and unchanged premultiplied UI blending on the floating surface.

use super::*;
use eframe::egui;
use wgpu::util::DeviceExt;

fn device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    pollster::block_on(adapter.request_device(&Default::default(), None)).unwrap()
}
fn target(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Floating display check"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
fn read(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> [f32; 4] {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Floating display readback"),
        size: 256,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: None,
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        sender.send(r).unwrap();
    });
    device.poll(wgpu::Maintain::Wait);
    receiver.recv().unwrap().unwrap();
    let data = buffer.slice(..).get_mapped_range();
    std::array::from_fn(|i| {
        half::f16::from_bits(u16::from_le_bytes([data[i * 2], data[i * 2 + 1]])).to_f32()
    })
}
fn encode(v: f32) -> f32 {
    let a = v.abs();
    v.signum()
        * if a <= 0.0031308 {
            12.92 * a
        } else {
            1.055 * a.powf(1. / 2.4) - 0.055
        }
}

#[test]
#[ignore = "requires a GPU"]
fn photographic_shader_preserves_extended_colors_and_retains_the_srgb_fallback() {
    let (device, queue) = device();
    let source = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Adobe RGB source"),
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        view_formats: &[],
    });
    queue.write_texture(
        source.as_image_copy(),
        bytemuck::cast_slice(&[0_f32, 1., 0., 1.]),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(16),
            rows_per_image: None,
        },
        source.size(),
    );
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Actual photographic display shader"),
        source: wgpu::ShaderSource::Wgsl(
            format!(
                "{}\n{}",
                egui_wgpu::COLOR_TRANSFORM_SHADER,
                include_str!("view.wgsl")
            )
            .into(),
        ),
    });
    use foveonic::color_space::{ColorSpace, TiffColorSpace};
    let primaries = TiffColorSpace::Srgb.transform(ColorSpace::AdobeRgb);
    for (extended, gamut) in [false, true].into_iter().flat_map(|extended| {
        [
            None,
            Some(TiffColorSpace::AdobeRgb),
            Some(TiffColorSpace::Srgb),
            Some(TiffColorSpace::ProPhotoRgb),
        ]
        .map(|gamut| (extended, gamut))
    }) {
        let transform = gamut
            .map(|space| {
                egui_wgpu::ColorTransform::new(
                    space.transform(ColorSpace::Srgb),
                    TiffColorSpace::Srgb.transform(space.into()),
                )
            })
            .unwrap_or_default();
        let mut bytes = bytemuck::cast_slice(&[0_f32, 0., 1., 1., -1., 1., 1., 0.]).to_vec();
        bytes.extend_from_slice(bytemuck::cast_slice(&primaries));
        bytes.extend_from_slice(bytemuck::bytes_of(&transform));
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: &bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let constants =
            std::collections::HashMap::from([("EXTENDED_DISPLAY".into(), f64::from(extended))]);
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: wgpu::PipelineCompilationOptions {
                    constants: &constants,
                    ..Default::default()
                },
                targets: &[Some(wgpu::TextureFormat::Rgba16Float.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let view = source.create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        });
        let output = target(&device);
        let output_view = output.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..6, 0..1);
        }
        queue.submit([encoder.finish()]);
        let actual = read(&device, &queue, &output);
        let expected = if extended && gamut != Some(TiffColorSpace::Srgb) {
            [
                encode(primaries[0][1]),
                encode(primaries[1][1]),
                encode(primaries[2][1]),
                1.,
            ]
        } else {
            [0., encode(primaries[1][1].clamp(0., 1.)), 0., 1.]
        };
        for (a, b) in actual.into_iter().zip(expected) {
            assert!(
                (a - b).abs() < 0.0005,
                "extended={extended}: {actual:?} != {expected:?}"
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn floating_egui_keeps_white_transparency_white() {
    let (device, queue) = device();
    let mut renderer =
        egui_wgpu::Renderer::new(&device, wgpu::TextureFormat::Rgba16Float, None, 1, false);
    let texture_id = egui::TextureId::Managed(0);
    renderer.update_texture(
        &device,
        &queue,
        texture_id,
        &egui::epaint::ImageDelta::full(
            egui::ColorImage::new(
                [1, 1],
                egui::Color32::from_rgba_premultiplied(128, 128, 128, 128),
            ),
            egui::TextureOptions::NEAREST,
        ),
    );
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1., 1.));
    let mut mesh = egui::Mesh::with_texture(texture_id);
    mesh.add_rect_with_uv(rect, rect, egui::Color32::WHITE);
    let jobs = [egui::ClippedPrimitive {
        clip_rect: rect,
        primitive: egui::epaint::Primitive::Mesh(mesh),
    }];
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [1, 1],
        pixels_per_point: 1.,
    };
    use foveonic::color_space::{ColorSpace, TiffColorSpace};
    for destination in [
        None,
        Some(TiffColorSpace::Srgb),
        Some(TiffColorSpace::AdobeRgb),
        Some(TiffColorSpace::ProPhotoRgb),
    ] {
        renderer.set_color_transform(
            destination
                .map(|space| {
                    egui_wgpu::ColorTransform::new(
                        space.transform(ColorSpace::Srgb),
                        TiffColorSpace::Srgb.transform(space.into()),
                    )
                })
                .unwrap_or_default(),
        );
        let output = target(&device);
        let output_view = output.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        let commands = renderer.update_buffers(&device, &queue, &mut encoder, &jobs, &screen);
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            renderer.render(&mut pass.forget_lifetime(), &jobs, &screen);
        }
        queue.submit(commands.into_iter().chain([encoder.finish()]));
        let actual = read(&device, &queue, &output);
        assert!(
            actual.iter().all(|v| (*v - 1.).abs() < 0.0005),
            "premultiplied white gained a dark fringe: {actual:?}"
        );
    }
}
