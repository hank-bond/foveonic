//! Measures completed ordinary egui navigator drawing against repeated full-resolution float presentation.

use super::gpu_tests::{state, texture};
use super::*;

fn pixels(rs: &egui_wgpu::RenderState, texture: &wgpu::Texture) -> Vec<u8> {
    let row =
        (texture.width() * texture.format().block_copy_size(None).unwrap()).div_ceil(256) * 256;
    let buffer = rs.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Synthetic presentation comparison"),
        size: u64::from(row * texture.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = rs.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
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
    rs.queue.submit(Some(encoder.finish()));
    let (tx, rx) = mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
    rs.device.poll(wgpu::Maintain::Wait);
    rx.recv().unwrap().unwrap();
    let bytes = buffer.slice(..).get_mapped_range().to_vec();
    buffer.unmap();
    bytes
}

#[test]
#[ignore = "requires a GPU"]
fn signed_difference_subtracts_unmapped_samples_and_preserves_display_neutral() {
    let rs = state();
    let make_source = || {
        texture(
            &rs,
            [1, 1],
            wgpu::TextureFormat::Rgba32Float,
            wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        )
    };
    let output = make_source();
    let input = make_source();
    let target = texture(
        &rs,
        [1, 1],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let upload = |texture: &wgpu::Texture, value: [f32; 4]| {
        rs.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&value),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(16),
                rows_per_image: Some(1),
            },
            texture.size(),
        );
    };
    for (space, range) in [
        (Space::CalibratedLayers, 4095.),
        (
            Space::LinearRgb(foveonic::image::ColorSpace::AdobeRgb),
            4095.,
        ),
        (Space::ToneMappedRgb(foveonic::image::ColorSpace::Srgb), 1.),
    ] {
        for baseline in [-range * 0.125, range * 2.] {
            for gain in [1., 32., 1024.] {
                for (fraction, expected) in
                    [(-1., 0_u8), (-0.5, 64), (0., 128), (0.5, 191), (1., 255)]
                {
                    let delta = fraction * range / gain;
                    upload(&input, [baseline, baseline, baseline, 1.]);
                    // Change only channel 1 to catch accidental use of another channel.
                    upload(&output, [baseline, baseline + delta, baseline, 1.]);
                    viewport::prepare_navigator(
                        &rs,
                        &output,
                        space,
                        &target,
                        viewport::Mapping {
                            channel: 1,
                            input: false,
                            difference_gain: gain,
                        },
                        Some(&input),
                    );
                    let bytes = pixels(&rs, &target);
                    for value in &bytes[..3] {
                        assert!(
                            value.abs_diff(expected) <= 1,
                            "{space:?} baseline={baseline} gain={gain} delta={delta}: {value} != {expected}"
                        );
                    }
                    assert_eq!(bytes[3], 255);
                }
            }
        }
    }
    // The normal output mapping must ignore a retained reference when Difference is off.
    upload(&output, [0.25; 4]);
    upload(&input, [0.9; 4]);
    viewport::prepare_navigator(
        &rs,
        &output,
        Space::ToneMappedRgb(foveonic::image::ColorSpace::Srgb),
        &target,
        viewport::Mapping::normal(1),
        Some(&input),
    );
    assert!(pixels(&rs, &target)[0].abs_diff(137) <= 1);
    viewport::prepare_navigator(
        &rs,
        &input,
        Space::ToneMappedRgb(foveonic::image::ColorSpace::Srgb),
        &target,
        viewport::Mapping {
            input: true,
            ..viewport::Mapping::normal(1)
        },
        Some(&input),
    );
    assert!(pixels(&rs, &target)[0].abs_diff(243) <= 1);
}

#[test]
#[ignore = "requires a GPU"]
fn romm_presentation_converts_and_clips_without_changing_float_source() {
    let rs = state();
    let source = texture(
        &rs,
        [1, 1],
        wgpu::TextureFormat::Rgba32Float,
        wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let target = texture(
        &rs,
        [1, 1],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    // At low intensity, ROMM green converts to approximately (-0.073, 0.123, -0.015)
    // in linear sRGB. This distinguishes its primary conversion from Adobe RGB.
    for (green, expected) in [(0.1_f32, 98_u8), (1.2, 255)] {
        let value = [0., green, 0., 1.];
        rs.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&value),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(16),
                rows_per_image: Some(1),
            },
            source.size(),
        );
        let before = pixels(&rs, &source);
        viewport::prepare_navigator(
            &rs,
            &source,
            Space::ToneMappedRgb(foveonic::image::ColorSpace::ProPhotoRgb),
            &target,
            viewport::Mapping::normal(-1),
            None,
        );
        let displayed = pixels(&rs, &target);
        assert_eq!(displayed[0], 0);
        assert!(
            displayed[1].abs_diff(expected) <= 1,
            "green={green}: {:?}",
            &displayed[..4]
        );
        assert_eq!(displayed[2], 0);
        assert_eq!(displayed[3], 255);
        assert_eq!(pixels(&rs, &source), before);
        let retained = f32::from_le_bytes(before[4..8].try_into().unwrap());
        assert_eq!(retained, green);
    }
}

#[test]
#[ignore = "requires a GPU; reports completed old/new navigator drawing without profiling synchronization"]
fn ordinary_navigator_drawing_cost() {
    let rs = state();
    let extent = [512, 341];
    let source = texture(
        &rs,
        [4704, 3136],
        wgpu::TextureFormat::Rgba32Float,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let tile = texture(
        &rs,
        extent,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let old_target = texture(
        &rs,
        extent,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let new_target = texture(
        &rs,
        extent,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    // Native-sized synthetic source, initialized entirely on GPU. No photographic samples are read back.
    let view = source.create_view(&Default::default());
    let mut encoder = rs.device.create_command_encoder(&Default::default());
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Synthetic float source"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.15,
                        g: 0.35,
                        b: 0.65,
                        a: 1.,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    rs.queue.submit(Some(encoder.finish()));
    let space = Space::ToneMappedRgb(foveonic::image::ColorSpace::Srgb);
    viewport::prepare_navigator(
        &rs,
        &source,
        space,
        &tile,
        viewport::Mapping::normal(-1),
        None,
    );
    rs.device.poll(wgpu::Maintain::Wait);
    let id = rs.renderer.write().register_native_texture(
        &rs.device,
        &tile.create_view(&Default::default()),
        wgpu::FilterMode::Linear,
    );
    let rect = egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(extent[0] as f32, extent[1] as f32),
    );
    let mut mesh = egui::epaint::Mesh::with_texture(id);
    mesh.add_rect_with_uv(
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
        egui::Color32::WHITE,
    );
    let jobs = [egui::epaint::ClippedPrimitive {
        clip_rect: rect,
        primitive: egui::epaint::Primitive::Mesh(mesh),
    }];
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: extent,
        pixels_per_point: 1.,
    };
    let draw_new = || {
        let mut renderer = rs.renderer.write();
        let mut encoder = rs.device.create_command_encoder(&Default::default());
        let commands = renderer.update_buffers(&rs.device, &rs.queue, &mut encoder, &jobs, &screen);
        assert!(commands.is_empty());
        let view = new_target.create_view(&Default::default());
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Ordinary egui navigator"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            renderer.render(&mut pass, &jobs, &screen);
        }
        rs.queue.submit(Some(encoder.finish()));
    };
    let draw_old = || {
        viewport::prepare_navigator(
            &rs,
            &source,
            space,
            &old_target,
            viewport::Mapping::normal(-1),
            None,
        );
    };
    draw_old();
    draw_new();
    let old = pixels(&rs, &old_target);
    let new = pixels(&rs, &new_target);
    assert!(old.iter().zip(&new).all(|(a, b)| a.abs_diff(*b) <= 1));
    // Alternate the order so warming and clock changes do not favor one path.
    // Include preparation, submission, and completion but leave shared overlays out.
    let mut old_ms = Vec::new();
    let mut new_ms = Vec::new();
    for round in 0..5 {
        for ordinary in if round % 2 == 0 {
            [false, true]
        } else {
            [true, false]
        } {
            let started = Instant::now();
            for _ in 0..25 {
                if ordinary {
                    draw_new();
                } else {
                    draw_old();
                }
            }
            // Wait once for the batch so per-draw waits do not distort the cost of
            // ordinary drawing.
            rs.device.poll(wgpu::Maintain::Wait);
            let elapsed = started.elapsed().as_secs_f64() * 1000. / 25.;
            if ordinary {
                new_ms.push(elapsed);
            } else {
                old_ms.push(elapsed);
            }
        }
    }
    old_ms.sort_by(f64::total_cmp);
    new_ms.sort_by(f64::total_cmp);
    eprintln!(
        "NAVIGATOR-DRAW old_completed_median_ms={:.3} ordinary_completed_median_ms={:.3} old_rounds={old_ms:?} ordinary_rounds={new_ms:?}",
        old_ms[2], new_ms[2]
    );
    rs.renderer.write().free_texture(&id);
}
