//! Checks native preview gamut, linear interpolation, and the bounded fallback through egui's actual shader.

use super::*;

fn memory_error() -> wgpu::Error {
    wgpu::Error::OutOfMemory {
        source: std::io::Error::other("injected allocation failure").into(),
    }
}
fn validation_error() -> wgpu::Error {
    wgpu::Error::Validation {
        source: std::io::Error::other("injected invalid resource").into(),
        description: "invalid resource".into(),
    }
}

#[test]
fn upload_capture_is_scoped_sticky_and_thread_local() {
    assert!(capture_upload_error(memory_error()).is_err());
    {
        let upload = Upload::begin();
        assert!(capture_upload_error(validation_error()).is_err());
        assert!(
            std::thread::spawn(|| capture_upload_error(memory_error()).is_err())
                .join()
                .unwrap()
        );
        assert!(upload.check("test").is_ok());
        capture_upload_error(memory_error()).unwrap();
        capture_upload_error(validation_error()).unwrap();
        assert!(upload
            .check("test")
            .unwrap_err()
            .contains("Not enough GPU memory"));
        assert!(upload.check("cleanup").is_err());
    }
    assert!(capture_upload_error(memory_error()).is_err());
    assert!(Upload::begin().check("retry").is_ok());
    let unwind = std::panic::catch_unwind(|| {
        let _upload = Upload::begin();
        panic!("test guard cleanup");
    });
    assert!(unwind.is_err());
    assert!(Upload::begin().check("after unwind").is_ok());
}

#[test]
fn upload_validates_geometry_and_samples_without_allocating_gpu_resources() {
    for (size, count) in [
        ([0, 1], 0),
        ([2, 2], 3),
        ([8193, 1], 0),
        ([usize::MAX, 2], 0),
    ] {
        assert!(Pixels {
            size,
            rgba: vec![[0; 4]; count]
        }
        .validate(8192)
        .is_err());
    }
    assert!(Pixels {
        size: [2, 1],
        rgba: vec![[0; 4]; 2]
    }
    .validate(8192)
    .is_ok());
}

#[test]
#[ignore = "requires a GPU"]
fn failed_browser_uploads_report_errors_release_registration_and_allow_other_photos() {
    let rs = crate::navigator::gpu_tests::state();
    install_upload_error_handler(&rs.device);
    let ctx = egui::Context::default();
    let mut browser = crate::browser::Browser::with_cache(ctx.clone(), None, rs.clone());
    let first = std::path::PathBuf::from("failed.x3f");
    let second = std::path::PathBuf::from("healthy.x3f");
    browser.files = vec![first.clone(), second.clone()];
    let prepared = || {
        crate::preview_image::PreparedPreview::new(
            foveonic::preview::Preview {
                rgb: vec![32768; 6],
                color_space: foveonic::color_space::TiffColorSpace::Srgb,
                extent: [2, 1],
                native_extent: [2, 1],
                rotation: 0,
            },
            false,
        )
        .unwrap()
    };
    for (index, stage) in [
        "creating preview texture",
        "uploading preview pixels",
        "creating preview view",
        "registering preview",
    ]
    .into_iter()
    .enumerate()
    {
        FAIL_UPLOAD_AT.with(|target| target.set(Some(stage)));
        browser.rendered(&ctx, first.clone(), index as u64, prepared());
        assert!(browser.thumbnail(&first).is_none());
        assert!(UPLOAD_ERROR.with(|slot| slot.borrow().is_none()));
        let mut log = crate::activity::Log::default();
        log.receive(&ctx);
        assert!(log.unread_errors());
        browser.rendered(&ctx, second.clone(), index as u64, prepared());
        assert!(browser.thumbnail(&second).is_some());
    }
    let pixels = || Pixels {
        size: [1, 1],
        rgba: vec![[0; 4]],
    };
    let previous = pixels().upload(&rs).unwrap();
    let egui::TextureId::User(id) = previous.id() else {
        unreachable!()
    };
    FAIL_UPLOAD_AT.with(|target| target.set(Some("registering preview")));
    assert!(pixels().upload(&rs).is_err());
    // A dropped texture is freed when the next frame starts.
    free_retired(&rs);
    assert!(rs
        .renderer
        .read()
        .texture(&egui::TextureId::User(id + 1))
        .is_none());
    let next = pixels().upload(&rs).unwrap();
    assert!(rs.renderer.read().texture(&next.id()).is_some());
}

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
                let texture = Pixels::from_linear(&image, space).upload(&rs).unwrap();
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
                rs.device
                    .poll(wgpu::PollType::Wait)
                    .expect("GPU polling failed");
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

#[test]
#[ignore = "requires a GPU"]
fn thumbnail_quarter_turns_preserve_direction_and_linear_wide_colors() {
    let rs = crate::navigator::gpu_tests::state();
    // Two columns make clockwise and counterclockwise orientation unambiguous.
    let colors = [[-0.25_f32, 1.25, 0.1], [0.8, 0.2, 0.3]];
    let image = image::Rgb32FImage::from_raw(2, 1, colors.concat()).unwrap();
    let original = Pixels::from_linear(&image, foveonic::image::ColorSpace::Srgb)
        .upload(&rs)
        .unwrap();
    for clockwise in [false, true] {
        let rotated = original.quarter_turn(clockwise).unwrap();
        assert_eq!(rotated.size(), [1, 2]);
        let Storage::Native {
            _texture: source, ..
        } = &*rotated.0
        else {
            unreachable!()
        };
        let target = crate::navigator::gpu_tests::texture(
            &rs,
            [1, 2],
            wgpu::TextureFormat::Rgba16Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        crate::viewport::rotate_linear_preview(
            &rs,
            source,
            &target,
            crate::framing::Presentation {
                rotation: Default::default(),
                native_extent: [1, 2],
            },
        );
        let pitch = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = rs.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(pitch) * 2,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = rs.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch),
                    rows_per_image: None,
                },
            },
            target.size(),
        );
        rs.queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        rs.device
            .poll(wgpu::PollType::Wait)
            .expect("GPU polling failed");
        rx.recv().unwrap().unwrap();
        let bytes = buffer.slice(..).get_mapped_range();
        for row in 0..2 {
            let expected = colors[if clockwise { row } else { 1 - row }];
            for (c, value) in expected.into_iter().enumerate() {
                let i = row * pitch as usize + c * 2;
                let actual = half::f16::from_le_bytes([bytes[i], bytes[i + 1]]).to_f32();
                assert!(
                    (actual - value).abs() < 0.005,
                    "clockwise={clockwise}, row={row}, c={c}: {actual} != {value}"
                );
            }
        }
    }
}
