//! Checks native preview gamut, linear interpolation, and the bounded fallback through egui's actual shader.

use super::*;

#[test]
#[ignore = "requires a GPU"]
fn native_previews_preserve_gamut_and_filter_in_linear_light() {
    use foveonic::image::ColorSpace;
    for format in [
        wgpu::TextureFormat::Rgba16Float,
        wgpu::TextureFormat::Rgba8Unorm,
    ] {
        let rs = crate::navigator::gpu_tests::state_for(format);
        for (gamut_index, gamut) in crate::preferences::DisplayGamut::ALL
            .into_iter()
            .enumerate()
        {
            rs.renderer.write().set_color_transform(gamut.transform());
            // Expected linear sRGB rows follow Monitor, sRGB, Adobe RGB, ROMM.
            for (samples, space, expected) in [
                (
                    vec![0., 1., 0., 0., 1., 0.],
                    ColorSpace::AdobeRgb,
                    [
                        [-0.39835574, 1., -0.04292899],
                        [0., 1., 0.],
                        [-0.39835574, 1., -0.04292899],
                        [-0.39835574, 1., -0.04292899],
                    ],
                ),
                (
                    vec![0., 1., 0., 0., 1., 0.],
                    ColorSpace::ProPhotoRgb,
                    [
                        [-0.7274671, 1.2316059, -0.1532819],
                        [0., 1., 0.],
                        [-0.39835574, 1., -0.04292899],
                        [-0.7274671, 1.2316059, -0.1532819],
                    ],
                ),
                // An out-of-range synthetic ROMM sample distinguishes its limit from Monitor.
                (
                    vec![1.2, 0., 0., 1.2, 0., 0.],
                    ColorSpace::ProPhotoRgb,
                    [
                        [2.4410413, -0.2743703, -0.01022152],
                        [1., 0., 0.],
                        [1.3983557, 0., 0.],
                        [2.034201, -0.22864193, -0.008517936],
                    ],
                ),
                (
                    vec![0., 0., 0., 1., 1., 1.],
                    ColorSpace::Srgb,
                    [[0.5; 3]; 4],
                ),
                (
                    vec![0.1, 0.4, 0.7, 0.1, 0.4, 0.7],
                    ColorSpace::Srgb,
                    [[0.1, 0.4, 0.7]; 4],
                ),
            ] {
                let expected = expected[gamut_index].map(|value: f32| {
                    let a = value.abs();
                    value.signum()
                        * if a <= 0.0031308 {
                            a * 12.92
                        } else {
                            1.055 * a.powf(1. / 2.4) - 0.055
                        }
                });
                let image = image::Rgb32FImage::from_raw(2, 1, samples).unwrap();
                let texture = Pixels::from_linear(&image, space).upload(&rs);
                let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1., 1.));
                let mut mesh = egui::Mesh::with_texture(texture.id());
                mesh.add_rect_with_uv(rect, rect, egui::Color32::WHITE);
                let jobs = [egui::ClippedPrimitive {
                    clip_rect: rect,
                    primitive: egui::epaint::Primitive::Mesh(mesh),
                }];
                let screen = egui_wgpu::ScreenDescriptor {
                    size_in_pixels: [1, 1],
                    pixels_per_point: 1.,
                };
                let target = crate::navigator::gpu_tests::texture(
                    &rs,
                    [1, 1],
                    format,
                    wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                );
                let view = target.create_view(&Default::default());
                let mut encoder = rs.device.create_command_encoder(&Default::default());
                let mut renderer = rs.renderer.write();
                let commands =
                    renderer.update_buffers(&rs.device, &rs.queue, &mut encoder, &jobs, &screen);
                {
                    let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
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
                    renderer.render(&mut pass.forget_lifetime(), &jobs, &screen);
                }
                drop(renderer);
                let row_bytes = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
                let buffer = rs.device.create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: u64::from(row_bytes),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                });
                encoder.copy_texture_to_buffer(
                    target.as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffer,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(row_bytes),
                            rows_per_image: None,
                        },
                    },
                    target.size(),
                );
                rs.queue
                    .submit(commands.into_iter().chain([encoder.finish()]));
                let (tx, rx) = std::sync::mpsc::channel();
                buffer
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
                rs.device.poll(wgpu::Maintain::Wait);
                rx.recv().unwrap().unwrap();
                let bytes = buffer.slice(..).get_mapped_range();
                for c in 0..3 {
                    let (actual, expected) = if format == wgpu::TextureFormat::Rgba16Float {
                        (
                            half::f16::from_le_bytes([bytes[c * 2], bytes[c * 2 + 1]]).to_f32(),
                            expected[c],
                        )
                    } else {
                        (f32::from(bytes[c]) / 255., expected[c].clamp(0., 1.))
                    };
                    assert!(
                        (actual - expected).abs() < 0.005,
                        "{format:?} {space:?} channel {c}: {actual} != {expected}"
                    );
                }
                drop(bytes);
                buffer.unmap();
            }
        }
    }
}
