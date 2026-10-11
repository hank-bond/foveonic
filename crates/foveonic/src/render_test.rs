//! Checks control preparation, retained GPU image lifetimes, scratch branches, and texture/export consumers.

use super::*;

#[test]
fn control_matrices_preserve_gray_and_signed_uniform_sensor_gain() {
    let apply = |params: calibration::ColorParams, rgb: [f32; 3]| -> [f32; 3] {
        params
            .matrix
            .map(|row| (0..3).map(|c| row[c] * rgb[c]).sum())
    };
    assert_eq!(
        apply(sensor_gain([1, 1, 0, 0], 2.), [-0.25, 2., 1.]),
        [-0.5, 4., 2.]
    );
    assert_eq!(
        apply(saturation([1, 1, 0, 0], 1.), [1., 0.25, 0.5]),
        [1., 0.25, 0.5]
    );
    assert_eq!(
        apply(saturation([1, 1, 0, 0], 0.), [1., 0., 0.]),
        [0.299; 3]
    );
    for factor in [0., 1., 2.] {
        for value in apply(saturation([1, 1, 0, 0], factor), [0.42; 3]) {
            assert!((value - 0.42).abs() < 1e-6);
        }
    }
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn uniform_sensor_gain_preserves_signed_calibrated_layers_before_color() {
    let engine = Engine::new().unwrap();
    let original = [
        -0.25_f32, 2., 1., 0.5, -0.5, 3., 1., 0., -1., 0., 0.25, 0.75, 2., 1., 0., -2., -1., -0.5,
    ];
    let mut input = image(&engine, &original);
    input.description.space = Space::CalibratedLayers;
    let matrix = [[1., 2., 3., 0.], [0., 1., -1., 0.], [2., 0., 1., 0.]];
    for factor in [0_f32, 0.5, 2.] {
        let mut graph = Graph::default();
        let root = graph
            .add(
                Operation::RenderedInput {
                    data: DataType::CalibratedLayers,
                },
                &[],
            )
            .unwrap();
        let gain = graph
            .add(Operation::SensorGain { factor }, &[root])
            .unwrap();
        let color = graph
            .add(
                Operation::Color {
                    params: calibration::ColorParams {
                        source: [99, 99, 0, 0],
                        matrix,
                    },
                    space: ColorSpace::Srgb,
                },
                &[gain],
            )
            .unwrap();
        let outputs = engine
            .execute(
                Input::Rendered(&input),
                &graph,
                &[gain, color],
                &mut Profile::default(),
            )
            .unwrap();
        assert_eq!(
            outputs[0].description.unwrap().space,
            Space::CalibratedLayers
        );
        assert_eq!(
            outputs[1].description.unwrap().space,
            Space::LinearRgb(ColorSpace::Srgb)
        );
        let layers = engine.gpu.readback(&outputs[0].buffer).unwrap();
        for (actual, source) in bytemuck::cast_slice::<u8, f32>(&layers)
            .iter()
            .zip(original)
        {
            assert!((actual - source * factor).abs() < 0.000001);
        }
        let rgb = engine.gpu.readback(&outputs[1].buffer).unwrap();
        for (actual, source) in bytemuck::cast_slice::<u8, f32>(&rgb)
            .as_chunks::<3>()
            .0
            .iter()
            .zip(original.as_chunks::<3>().0.iter())
        {
            for (c, row) in matrix.iter().enumerate() {
                let expected = (0..3).map(|k| row[k] * source[k] * factor).sum::<f32>();
                assert!((actual[c] - expected).abs() < 0.000001);
            }
        }
    }
    assert_eq!(
        engine.gpu.readback(&input.buffer).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&original)
    );
}

#[test]
fn gaussian_weights_are_symmetric_finite_and_normalized() {
    for sigma in [1e-8, 0.5, 1., 1.2, 16.] {
        let weights = gaussian_weights(sigma);
        assert!(weights.len() % 2 == 1);
        assert!(weights.iter().all(|v| v.is_finite() && *v >= 0.));
        assert!((weights.iter().sum::<f32>() - 1.).abs() < 2e-6);
        assert!(weights
            .iter()
            .zip(weights.iter().rev())
            .all(|(a, b)| a == b));
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR with original Merrill files"]
fn original_result_outlives_source_and_another_render() {
    let directory = std::env::var_os("FOVEONIC_FIXTURE_DIR").expect("set FOVEONIC_FIXTURE_DIR");
    let mut files: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("x3f"))
        })
        .collect();
    files.sort();
    let engine = Engine::new().unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.output_size = Some([64, 96]);
    let retained = {
        let source = Source::open(&files[0]).unwrap();
        let mut histogram_profile = Profile::new(true);
        let artifacts = engine
            .render_artifacts(
                &source,
                &Settings::new("", ""),
                &["black.histogram"],
                &mut histogram_profile,
            )
            .unwrap();
        assert_eq!(artifacts.len(), 1);
        let crate::inspection::Content::Table { columns, values } = &artifacts[0].content else {
            panic!("expected histogram");
        };
        assert_eq!(*columns, 4096);
        assert_eq!(values.len(), 12288);
        assert!(!histogram_profile
            .host
            .iter()
            .any(|t| t.stage == "submit calibration" || t.stage == "prepare tone curve"));
        assert!(!histogram_profile
            .gpu
            .iter()
            .any(|t| t.stage == "black mean"));
        engine
            .render_artifacts(
                &source,
                &Settings::new("Sunlight", ""),
                &["calibration.layers"],
                &mut Profile::default(),
            )
            .unwrap();
        let mut gain_settings = Settings::new("Sunlight", "");
        gain_settings.sensor_gain = 1.3;
        let mut gain_profile = Profile::new(true);
        let gain = engine
            .render_artifacts(&source, &gain_settings, &["gain.layers"], &mut gain_profile)
            .unwrap();
        let crate::inspection::Content::Image(gain) = &gain[0].content else {
            panic!("expected gained layers")
        };
        assert_eq!(gain.description().space, Space::CalibratedLayers);
        assert_eq!(gain.settings().sensor_gain, 1.3);
        assert!(!gain_profile
            .host
            .iter()
            .any(|t| t.stage == "prepare color mode" || t.stage == "prepare color matrix"));
        assert!(engine
            .render(&source, &Settings::new("", ""), &mut Profile::default())
            .is_err());
        let mut multi_profile = Profile::new(true);
        let roots = engine
            .render_artifacts(
                &source,
                &settings,
                &["calibration.layers", "final"],
                &mut multi_profile,
            )
            .unwrap();
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].name, "calibration.layers");
        engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap()
    };
    assert!(!retained.source_metadata().matrices.is_empty());
    let root = std::env::temp_dir().join(format!(
        "foveonic-retained-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    engine
        .write_tiff(
            &retained,
            &root.join("before.tiff"),
            &mut Profile::default(),
        )
        .unwrap();
    settings.exposure = 1.;
    let another = {
        let source = Source::open(&files[0]).unwrap();
        engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap()
    };
    engine
        .write_tiff(
            &another,
            &root.join("another.tiff"),
            &mut Profile::default(),
        )
        .unwrap();
    engine
        .write_tiff(&retained, &root.join("after.tiff"), &mut Profile::default())
        .unwrap();
    assert_eq!(
        std::fs::read(root.join("before.tiff")).unwrap(),
        std::fs::read(root.join("after.tiff")).unwrap()
    );
    assert_ne!(
        std::fs::read(root.join("before.tiff")).unwrap(),
        std::fs::read(root.join("another.tiff")).unwrap()
    );
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn scalar_fields_preserve_geometry_and_values_without_rgb_storage() {
    let engine = Engine::new().unwrap();
    let mut scalar = image(&engine, &[0.; 18]);
    scalar.buffer = engine
        .gpu
        .upload("Scalar fixture", &[-10_f32, -0.25, 0.5, -0.1, 0.75, 1.5])
        .unwrap();
    scalar.description.layout = Layout::Scalar32Float;
    scalar.description.space = Space::ScalarField;
    assert_eq!(scalar.description.byte_len(), 24);
    let mut graph = Graph::default();
    let input = graph
        .add(
            Operation::RenderedInput {
                data: DataType::ScalarField,
            },
            &[],
        )
        .unwrap();
    let imported = engine
        .execute(
            Input::Rendered(&scalar),
            &graph,
            &[input],
            &mut Profile::default(),
        )
        .unwrap();
    assert_eq!(imported[0].description, Some(scalar.description));
    let texture = engine
        .view_texture(&scalar, None, &mut Profile::default())
        .unwrap();
    assert_eq!(texture.description().space, Space::ScalarField);
    let readback = engine.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("Scalar view readback"),
        size: 512,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let mut encoder = engine.device().create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: texture.texture(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(2),
            },
        },
        wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    engine.queue().submit(Some(encoder.finish()));
    let bytes = engine.gpu.readback(&readback).unwrap();
    for (i, value) in [0.75_f32, -0.25, 1.5, 0.5].iter().enumerate() {
        let offset = (i / 2) * 256 + (i % 2) * 16;
        assert_eq!(
            bytemuck::cast_slice::<u8, f32>(&bytes[offset..offset + 16]),
            &[*value, *value, *value, 1.]
        );
    }
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn current_buffer_scopes_track_signed_values_and_oriented_columns() {
    let engine = Engine::new().unwrap();
    let mut buffer = image(
        &engine,
        &[
            0., 0., 0., -0.25, 0.5, 1.5, 0., 1., 0.25, 0., 0., 0., 0.75, 0.25, 0.5, 1., -0.5, 2.,
        ],
    );
    let scopes = engine.scopes(&buffer, &mut Profile::default()).unwrap();
    assert_eq!(scopes.samples, [2, 2]);
    for c in 0..3 {
        assert_eq!(scopes.histogram(c).iter().sum::<u32>(), 4);
        assert_eq!(scopes.density(c).iter().sum::<u32>(), 4);
    }
    assert_eq!(scopes.outside(0), [1, 0, 0]);
    assert_eq!(scopes.outside(1), [1, 0, 0]);
    assert_eq!(scopes.outside(2), [0, 2, 0]);
    assert_eq!(scopes.density(0)[64 * 256], 1); // rotated pixel 4 is left/top, red=.75
    assert_eq!(scopes.density(0)[255 * 256 + 128], 2); // right column contains both negative and zero red
    buffer.settings.exposure = 3.; // analysis follows the stored samples
    let repeat = engine.scopes(&buffer, &mut Profile::default()).unwrap();
    assert_eq!(scopes.counts, repeat.counts);
    buffer.buffer = engine
        .gpu
        .upload(
            "Sensor scope fixture",
            &[
                0_f32, 0., 0., 0., 2047.5, 4095., 4095., 0., 2047.5, 0., 0., 0., 1023.75, 4095.,
                0., 2047.5, 1023.75, 4095.,
            ],
        )
        .unwrap();
    buffer.description.space = Space::SensorLayers;
    let sensor = engine.scopes(&buffer, &mut Profile::default()).unwrap();
    for bin in [0, 64, 128, 255] {
        assert_eq!(sensor.histogram(0)[bin], 1);
    }
    assert_eq!(sensor.outside(0), [0, 0, 0]);
    buffer.buffer = engine
        .gpu
        .upload("RGB luminance fixture", &[1_f32, 0., 0.].repeat(6))
        .unwrap();
    for (space, bin) in [(ColorSpace::Srgb, 54), (ColorSpace::AdobeRgb, 76)] {
        buffer.description.space = Space::ToneMappedRgb(space);
        let rgb = engine.scopes(&buffer, &mut Profile::default()).unwrap();
        assert_eq!(rgb.histogram(3)[bin], 4);
    }
    buffer.buffer = engine
        .gpu
        .upload(
            "Camera-code RGB scope fixture",
            &[4095_f32, 0., 0.].repeat(6),
        )
        .unwrap();
    for (space, bin) in [(ColorSpace::Srgb, 54), (ColorSpace::AdobeRgb, 76)] {
        buffer.description.space = Space::LinearRgb(space);
        let rgb = engine.scopes(&buffer, &mut Profile::default()).unwrap();
        assert_eq!(rgb.histogram(0)[255], 4);
        assert_eq!(rgb.histogram(3)[bin], 4);
        assert_eq!(rgb.outside(0), [0, 0, 0]);
    }
    buffer.buffer = engine
        .gpu
        .upload(
            "Scalar scope fixture",
            &[-10_f32, -0.25, 0.5, -0.1, 0.75, 1.5],
        )
        .unwrap();
    buffer.description.layout = Layout::Scalar32Float;
    buffer.description.space = Space::ScalarField;
    let scalar = engine.scopes(&buffer, &mut Profile::default()).unwrap();
    assert_eq!(scalar.channels(), 1);
    assert_eq!(scalar.outside(0), [1, 1, 0]);
    buffer.buffer = engine
        .gpu
        .upload(
            "Signed detail scope fixture",
            &[-2000_f32, -1000., 0., 999., 1000., 2000.],
        )
        .unwrap();
    buffer.description.space = Space::SignedDetail;
    let detail = engine.scopes(&buffer, &mut Profile::default()).unwrap();
    assert_eq!(detail.channels(), 1);
    assert_eq!(detail.outside(0), [0, 1, 0]);
    assert_eq!(detail.histogram(0)[0], 1);
    assert_eq!(detail.histogram(0)[128], 1);
    assert_eq!(detail.histogram(0)[255], 2);
    buffer.buffer = engine
        .gpu
        .upload(
            "Luminance ratio scope fixture",
            &[0_f32, 0., 1., 0., 2., 3.],
        )
        .unwrap();
    buffer.description.space = Space::LuminanceRatio;
    let ratio = engine.scopes(&buffer, &mut Profile::default()).unwrap();
    assert_eq!(ratio.channels(), 1);
    assert_eq!(ratio.outside(0), [0, 1, 0]);
    assert_eq!(ratio.histogram(0)[0], 1);
    assert_eq!(ratio.histogram(0)[128], 1);
    assert_eq!(ratio.histogram(0)[255], 2);
}

fn image(engine: &Engine, pixels: &[f32]) -> RenderedImage {
    RenderedImage {
        buffer: engine.gpu.upload("Retained fixture", pixels).unwrap(),
        description: Description {
            extent: [3, 2],
            layout: Layout::Rgb32Float,
            space: Space::ToneMappedRgb(ColorSpace::Srgb),
        },
        view: View {
            crop: [1, 0, 3, 2],
            rotation: 90,
            extent: [2, 2],
        },
        // Original source geometry differs deliberately: pointwise nodes must use resource extents.
        metadata: Arc::new(Metadata {
            width: 30,
            height: 20,
            rotation: 90,
            properties: Default::default(),
            matrices: Default::default(),
            lists: Default::default(),
        }),
        settings: Settings::new("Sunlight", "Standard"),
        owner: engine.owner.clone(),
    }
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn gpu_retained_results_survive_scratch_reuse_and_feed_texture_and_export() {
    let engine = Engine::new().unwrap();
    let original = [
        [0_f32, 0., 0.],
        [-0.1, 0.2, 1.5],
        [0.5, 0.6, 0.7],
        [0.1, 0.3, 0.5],
        [0.25, 0.5, 0.75],
        [1., 1., 1.],
    ]
    .concat();
    let input = image(&engine, &original);
    let mut graph = Graph::default();
    let root = graph
        .add(
            Operation::RenderedInput {
                data: DataType::ToneMappedRgb(ColorSpace::Srgb),
            },
            &[],
        )
        .unwrap();
    let a = graph
        .add(
            Operation::Saturation {
                params: sensor_gain([99, 99, 0, 0], 2.),
                space: ColorSpace::Srgb,
            },
            &[root],
        )
        .unwrap();
    let b = graph
        .add(
            Operation::Saturation {
                params: sensor_gain([99, 99, 0, 0], 3.),
                space: ColorSpace::Srgb,
            },
            &[a],
        )
        .unwrap();
    let c = graph
        .add(
            Operation::Saturation {
                params: sensor_gain([99, 99, 0, 0], 0.5),
                space: ColorSpace::Srgb,
            },
            &[b],
        )
        .unwrap();
    let mut profile = Profile::new(true);
    // Retain two roots while later nodes execute, preserving a while c is written.
    let mut outputs = engine
        .execute(Input::Rendered(&input), &graph, &[a, c], &mut profile)
        .unwrap();
    let _other = outputs.pop().unwrap();
    let retained = outputs.pop().unwrap();
    let a_bytes = engine.gpu.readback(&retained.buffer).unwrap();
    for (actual, expected) in bytemuck::cast_slice::<u8, f32>(&a_bytes)
        .iter()
        .zip(&original)
    {
        assert!((actual - expected * 2.).abs() < 1e-6);
    }
    let latest = engine
        .execute(Input::Rendered(&input), &graph, &[c], &mut profile)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        engine.gpu.readback(&input.buffer).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&original)
    );
    assert_eq!(engine.gpu.readback(&retained.buffer).unwrap(), a_bytes);
    let latest_bytes = engine.gpu.readback(&latest.buffer).unwrap();
    for (actual, expected) in bytemuck::cast_slice::<u8, f32>(&latest_bytes)
        .iter()
        .zip(&original)
    {
        assert!((actual - expected * 3.).abs() < 1e-6);
    }
    assert!(profile.scratch_reuses > 0);
    if engine
        .device()
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY)
    {
        assert!(!profile.gpu.is_empty());
    }

    let texture = engine
        .view_texture(&input, None, &mut Profile::default())
        .unwrap();
    assert_eq!(texture.description().extent, [2, 2]);
    assert_eq!(texture.description().space, input.description.space);
    let readback = engine.device().create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 512,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let mut encoder = engine
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: texture.texture(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(2),
            },
        },
        wgpu::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    engine.queue().submit(Some(encoder.finish()));
    let bytes = engine.gpu.readback(&readback).unwrap();
    let floats = bytemuck::cast_slice::<u8, f32>(&bytes);
    for (index, source) in [4, 1, 5, 2].into_iter().enumerate() {
        let offset = (index / 2) * 64 + (index % 2) * 4;
        assert_eq!(
            &floats[offset..offset + 3],
            &original[source * 3..source * 3 + 3]
        );
        assert_eq!(floats[offset + 3], 1.);
    }
    let resized = engine
        .view_texture(&input, Some([1, 3]), &mut Profile::default())
        .unwrap();
    assert_eq!(resized.description().extent, [1, 3]);
    assert_eq!(input.view.extent, [2, 2]);
    assert!(engine
        .view_texture(&input, Some([0, 3]), &mut Profile::default())
        .is_err());

    let path = std::env::temp_dir().join(format!(
        "foveonic-resource-{}-{}.tiff",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    engine
        .write_tiff(&input, &path, &mut Profile::default())
        .unwrap();
    let mut decoder = tiff::decoder::Decoder::new(std::fs::File::open(&path).unwrap()).unwrap();
    assert_eq!(decoder.dimensions().unwrap(), (2, 2));
    assert!(engine
        .write_tiff(&input, &path, &mut Profile::default())
        .is_err());
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        engine.gpu.readback(&input.buffer).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&original)
    );
    let shared = Engine::from_device(
        engine.device().clone(),
        engine.queue().clone(),
        "shared fixture device".into(),
    )
    .unwrap();
    assert!(shared
        .view_texture(&input, None, &mut Profile::default())
        .is_err());
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn reconstruction_inspection_is_selective_and_retains_signed_detail() {
    let dir = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let source = Source::open(dir.join("blue-parking-sign.x3f")).unwrap();
    let engine = Engine::new().unwrap();
    let mut settings = Settings::new("Sunlight", "");
    settings.reconstruction = Some(Default::default());
    let mut profile = Profile::new(true);
    let mut artifacts = engine
        .render_artifacts(&source, &settings, &["noise.edges"], &mut profile)
        .unwrap();
    assert!(!profile.host.iter().any(|t| t.stage == "prepare color mode"));
    assert!(!profile
        .gpu
        .iter()
        .any(|t| t.stage == "directional local mean"));
    let crate::inspection::Content::Image(edge) = artifacts.remove(0).content else {
        panic!("expected an image")
    };
    assert_eq!(edge.description.space, Space::ScalarField);
    assert_eq!(edge.description.layout, Layout::Scalar32Float);
    let before = engine.gpu.readback(&edge.buffer).unwrap();
    profile = Profile::new(true);
    let artifacts = engine
        .render_artifacts(
            &source,
            &settings,
            &["noise.layers", "detail.ratio"],
            &mut profile,
        )
        .unwrap();
    assert!(!profile.host.iter().any(|t| t.stage == "prepare color mode"));
    assert!(!profile
        .gpu
        .iter()
        .any(|t| t.stage == "residual detail mixing"));
    assert_eq!(artifacts.len(), 2);
    for artifact in &artifacts {
        assert_eq!(artifact.settings.reconstruction, settings.reconstruction);
        let crate::inspection::Content::Image(image) = &artifact.content else {
            panic!("expected an image")
        };
        assert_eq!(
            image.description.space,
            if artifact.name == "detail.ratio" {
                Space::LuminanceRatio
            } else {
                Space::CalibratedLayers
            }
        );
    }
    drop(artifacts);
    settings.color_mode = "Standard".into();
    profile = Profile::new(true);
    let mut artifacts = engine
        .render_artifacts(&source, &settings, &["detail.signal"], &mut profile)
        .unwrap();
    assert!(!profile.gpu.iter().any(|t| matches!(
        t.stage,
        "residual detail mixing" | "detail reconstruction" | "tone"
    )));
    let crate::inspection::Content::Image(signal) = artifacts.remove(0).content else {
        panic!("expected an image")
    };
    assert_eq!(signal.description.space, Space::SignedDetail);
    assert_eq!(signal.description.layout, Layout::Scalar32Float);
    let samples = engine.gpu.readback(&signal.buffer).unwrap();
    let samples = bytemuck::cast_slice::<u8, f32>(&samples);
    assert!(samples.iter().all(|v| v.is_finite()));
    assert!(samples.iter().any(|v| *v < 0.) && samples.iter().any(|v| *v > 0.));
    let scopes = engine.scopes(&signal, &mut Profile::default()).unwrap();
    assert_eq!(scopes.channels(), 1);
    assert_eq!(
        scopes.histogram(0).iter().sum::<u32>(),
        scopes.samples[0] * scopes.samples[1]
    );
    let texture = engine
        .view_texture(&signal, Some([64, 96]), &mut Profile::default())
        .unwrap();
    assert_eq!(texture.description().space, Space::SignedDetail);
    drop(source);
    assert_eq!(engine.gpu.readback(&edge.buffer).unwrap(), before);
    assert_eq!(signal.settings.reconstruction, settings.reconstruction);
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn camera_repair_matches_manual_cutoff_and_sensor_inspection_skips_selection() {
    let engine = Engine::new().unwrap();
    let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("blue-parking-sign.x3f");
    let mut source = Source::open(path).unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.repair = Some(super::RepairThreshold::Camera);
    let read = |source: &Source, settings: &Settings| {
        let artifact = engine
            .render_artifacts(
                source,
                settings,
                &["repair.layers"],
                &mut Profile::default(),
            )
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(artifact.settings.repair, settings.repair);
        let crate::inspection::Content::Image(image) = artifact.content else {
            panic!()
        };
        engine.gpu.readback(&image.buffer).unwrap()
    };
    let automatic = read(&source, &settings);
    settings.repair = Some(super::RepairThreshold::Manual(2));
    assert_eq!(read(&source, &settings), automatic);
    settings.repair = Some(super::RepairThreshold::Manual(8));
    assert_ne!(read(&source, &settings), automatic);
    drop(automatic);
    settings.repair = Some(super::RepairThreshold::Camera);
    std::sync::Arc::make_mut(&mut source.metadata)
        .matrices
        .remove("BadnessTable");
    assert!(engine
        .render_artifacts(
            &source,
            &settings,
            &["sensor.layers"],
            &mut Profile::default()
        )
        .is_ok());
    assert!(engine
        .render_artifacts(
            &source,
            &settings,
            &["repair.layers"],
            &mut Profile::default()
        )
        .is_err());
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn advanced_overrides_preserve_captured_base_and_produce_finite_extremes() {
    let engine = Engine::new().unwrap();
    let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("blue-parking-sign.x3f");
    let source = Source::open(path).unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    let read = |settings: &Settings, root| {
        let artifact = engine
            .render_artifacts(&source, settings, &[root], &mut Profile::default())
            .unwrap()
            .pop()
            .unwrap();
        let crate::inspection::Content::Image(image) = artifact.content else {
            panic!()
        };
        engine.gpu.readback(&image.buffer).unwrap()
    };
    let baseline = read(&settings, "noise.layers");
    let values = crate::noise::calculated_values(
        &source.metadata,
        "Sunlight",
        1.,
        settings.reconstruction.unwrap(),
    )
    .unwrap();
    for parameter in crate::noise::Parameter::ALL {
        settings
            .reconstruction
            .as_mut()
            .unwrap()
            .overrides
            .set(parameter, Some(values[parameter as usize]));
    }
    assert_eq!(read(&settings, "noise.layers"), baseline);
    settings
        .reconstruction
        .as_mut()
        .unwrap()
        .overrides
        .set(crate::noise::Parameter::MeanDecay, Some(0.));
    assert_ne!(read(&settings, "noise.layers"), baseline);
    drop(baseline);
    for maximum in [false, true] {
        for parameter in crate::noise::Parameter::ALL {
            let bounds = parameter.bounds();
            settings.reconstruction.as_mut().unwrap().overrides.set(
                parameter,
                Some(if maximum {
                    *bounds.end()
                } else {
                    *bounds.start()
                }),
            );
        }
        let bytes = read(&settings, "reconstruction.rgb");
        assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .all(|v| v.is_finite()));
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn reconstruction_originals_stay_finite_across_noise_choices_and_zero_gain() {
    let dir = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let engine = Engine::new().unwrap();
    for file in [
        "blue-parking-sign",
        "glossy-leaves-and-buds",
        "rocks-and-tree-shadows",
        "sunlit-red-flower",
        "turquoise-still-life",
        "yellow-flowers-and-stroller",
    ] {
        let source = Source::open(dir.join(format!("{file}.x3f"))).unwrap();
        for choice in 0..=4 {
            let mut settings = Settings::new("Sunlight", "Standard");
            settings.reconstruction = Some(crate::noise::Controls {
                chroma: choice,
                luminance: (choice + 2) % 5,
                detail: if choice == 0 { 0. } else { 1. },
                ..Default::default()
            });
            settings.sensor_gain = if choice == 2 { 0. } else { 1. };
            settings.color_dq_gain = if choice == 4 { 1. } else { 0. };
            let mut artifacts = engine
                .render_artifacts(
                    &source,
                    &settings,
                    &["reconstruction.rgb"],
                    &mut Profile::default(),
                )
                .unwrap();
            let crate::inspection::Content::Image(image) = artifacts.remove(0).content else {
                panic!("expected an image")
            };
            let bytes = engine.gpu.readback(&image.buffer).unwrap();
            let samples = bytemuck::cast_slice::<u8, f32>(&bytes);
            assert!(
                samples.iter().all(|v| v.is_finite()),
                "{file}, choice {choice}"
            );
            if settings.sensor_gain == 0. {
                let [x0, y0, x1, y1] = image.view.crop;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let pixel = ((y * image.description.extent[0] + x) * 3) as usize;
                        assert_eq!(
                            &samples[pixel..pixel + 3],
                            &[0.; 3],
                            "zero gain at {x},{y} in {file}"
                        );
                    }
                }
            }
        }
    }
}
