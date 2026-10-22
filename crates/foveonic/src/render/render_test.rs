//! Checks control preparation, retained GPU image lifetimes, scratch branches, and texture/export consumers.

use super::recipes::saturation;
use super::*;
use crate::color_space::ColorSpace;
use crate::{graph::DataType, image::View, nodes::sensor::calibration, source::Metadata};

#[test]
fn saturation_matrices_preserve_gray() {
    let apply = |params: calibration::ColorParams, rgb: [f32; 3]| -> [f32; 3] {
        params
            .matrix
            .map(|row| (0..3).map(|c| row[c] * rgb[c]).sum())
    };
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
fn color_conversion_preserves_signed_calibrated_layers_and_headroom() {
    let engine = Engine::new().unwrap();
    let original = [
        -0.25_f32, 2., 1., 0.5, -0.5, 3., 1., 0., -1., 0., 0.25, 0.75, 2., 1., 0., -2., -1., -0.5,
    ];
    let mut input = image(&engine, &original);
    input.description.space = Space::CalibratedLayers;
    let matrix = [[1., 2., 3., 0.], [0., 1., -1., 0.], [2., 0., 1., 0.]];
    {
        let mut graph = Graph::default();
        let root = graph
            .add(
                Operation::RenderedInput {
                    data: DataType::CalibratedLayers,
                },
                &[],
            )
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
                &[root],
            )
            .unwrap();
        let outputs = engine
            .execute(
                Input::Rendered(&input),
                &graph,
                &[root, color],
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
            assert_eq!(*actual, source);
        }
        let rgb = engine.gpu.readback(&outputs[1].buffer).unwrap();
        for (actual, source) in bytemuck::cast_slice::<u8, f32>(&rgb)
            .as_chunks::<3>()
            .0
            .iter()
            .zip(original.as_chunks::<3>().0.iter())
        {
            for (c, row) in matrix.iter().enumerate() {
                let expected = (0..3).map(|k| row[k] * source[k]).sum::<f32>();
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
        let mut histogram_profile = Profile::new(crate::profile::Mode::Gpu);
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
        assert!(engine
            .render(&source, &Settings::new("", ""), &mut Profile::default())
            .is_err());
        let mut multi_profile = Profile::new(crate::profile::Mode::Gpu);
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
    let bytes = engine
        .gpu
        .readback_texture(texture.texture(), [2, 2], 256)
        .unwrap();
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
    let scopes = engine
        .scopes(&buffer, None, &mut Profile::default())
        .unwrap();
    assert_eq!(scopes.samples, [2, 2]);
    for c in 0..3 {
        assert_eq!(scopes.histogram(c).iter().sum::<u32>(), 4);
        assert_eq!(
            scopes.density(c).iter().sum::<u32>(),
            4 * crate::scopes::DENSITY_SCALE
        );
    }
    assert_eq!(scopes.outside(0), [1, 0, 0]);
    assert_eq!(scopes.outside(1), [1, 0, 0]);
    assert_eq!(scopes.outside(2), [0, 2, 0]);
    // The left sample column is centred at x=63.5, with red=.75 at y=63.75.
    assert_eq!(scopes.density(0)[63 * 256 + 63], 32);
    assert_eq!(scopes.density(0)[64 * 256 + 63], 96);
    // Two zero/clipped red samples share the right column's two neighboring cells.
    assert_eq!(scopes.density(0)[255 * 256 + 191], 256);
    assert_eq!(scopes.density(0)[255 * 256 + 192], 256);
    buffer.settings.exposure = 3.; // analysis follows the stored samples
    let repeat = engine
        .scopes(&buffer, None, &mut Profile::default())
        .unwrap();
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
    let sensor = engine
        .scopes(&buffer, None, &mut Profile::default())
        .unwrap();
    for bin in [0, 64, 128, 255] {
        assert_eq!(sensor.histogram(0)[bin], 1);
    }
    assert_eq!(sensor.outside(0), [0, 0, 0]);
    buffer.buffer = engine
        .gpu
        .upload("RGB luminance fixture", &[1_f32, 0., 0.].repeat(6))
        .unwrap();
    for (space, bin) in [
        (ColorSpace::Srgb, 54),
        (ColorSpace::AdobeRgb, 76),
        (ColorSpace::ProPhotoRgb, 73),
    ] {
        buffer.description.space = Space::ToneMappedRgb(space);
        let rgb = engine
            .scopes(&buffer, None, &mut Profile::default())
            .unwrap();
        assert_eq!(rgb.histogram(3)[bin], 4);
    }
    buffer.buffer = engine
        .gpu
        .upload(
            "Camera-code RGB scope fixture",
            &[4095_f32, 0., 0.].repeat(6),
        )
        .unwrap();
    for (space, bin) in [
        (ColorSpace::Srgb, 54),
        (ColorSpace::AdobeRgb, 76),
        (ColorSpace::ProPhotoRgb, 73),
    ] {
        buffer.description.space = Space::LinearRgb(space);
        let rgb = engine
            .scopes(&buffer, None, &mut Profile::default())
            .unwrap();
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
    let scalar = engine
        .scopes(&buffer, None, &mut Profile::default())
        .unwrap();
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
    let detail = engine
        .scopes(&buffer, None, &mut Profile::default())
        .unwrap();
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
    let ratio = engine
        .scopes(&buffer, None, &mut Profile::default())
        .unwrap();
    assert_eq!(ratio.channels(), 1);
    assert_eq!(ratio.outside(0), [0, 1, 0]);
    assert_eq!(ratio.histogram(0)[0], 1);
    assert_eq!(ratio.histogram(0)[128], 1);
    assert_eq!(ratio.histogram(0)[255], 2);
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn viewport_scopes_use_oriented_region_and_crop_relative_columns() {
    let engine = Engine::new().unwrap();
    let mut buffer = image(
        &engine,
        &[
            0., 0., 0., 0.125, 0.125, 0.125, 0.25, 0.25, 0.25, 0., 0., 0., 0.375, 0.375, 0.375,
            0.5, 0.5, 0.5,
        ],
    );
    // Rotation changes which source pixel occupies the top-left of the logical crop.
    for (rotation, value) in [(0, 0.125_f32), (90, 0.375), (180, 0.5), (270, 0.25)] {
        buffer.view.rotation = rotation;
        let scopes = engine
            .scopes(&buffer, Some([0, 0, 1, 1]), &mut Profile::default())
            .unwrap();
        assert_eq!(scopes.samples, [1, 1]);
        let bin = (value * 255.).round() as usize;
        assert_eq!(scopes.histogram(0)[bin], 1);
        assert_eq!(
            scopes.density(0).iter().sum::<u32>(),
            crate::scopes::DENSITY_SCALE
        );
        // A one-pixel region lands at the waveform centre, without snapping its level.
        for x in [127, 128] {
            assert_eq!(
                (0..256)
                    .map(|y| scopes.density(0)[y * 256 + x])
                    .sum::<u32>(),
                crate::scopes::DENSITY_SCALE / 2
            );
        }
        let vertical_weight = scopes
            .density(0)
            .iter()
            .enumerate()
            .map(|(i, weight)| (i / 256) as f64 * f64::from(*weight))
            .sum::<f64>();
        assert!(
            (vertical_weight / f64::from(crate::scopes::DENSITY_SCALE)
                - f64::from((1. - value) * 255.))
            .abs()
                < 0.01
        );
        assert_eq!(scopes.histogram(0).iter().sum::<u32>(), 1);
    }
    buffer.view.rotation = 90;
    let whole = engine
        .scopes(&buffer, None, &mut Profile::default())
        .unwrap();
    let fitted = engine
        .scopes(&buffer, Some([0, 0, 2, 2]), &mut Profile::default())
        .unwrap();
    assert_eq!(whole.counts, fitted.counts);
    let column = engine
        .scopes(&buffer, Some([1, 0, 2, 2]), &mut Profile::default())
        .unwrap();
    assert_eq!(column.samples, [1, 2]);
    assert_eq!(column.histogram(0).iter().sum::<u32>(), 2);
    assert_eq!(column.histogram(0)[32], 1);
    assert_eq!(column.histogram(0)[64], 1);
    for region in [[0, 0, 0, 1], [0, 1, 1, 1], [0, 0, 3, 2], [2, 0, 1, 1]] {
        assert!(engine
            .scopes(&buffer, Some(region), &mut Profile::default())
            .is_err());
    }
    // A resized output still maps region coordinates through the logical view.
    buffer.view.extent = [1024, 1024];
    let resized = engine
        .scopes(&buffer, Some([0, 0, 512, 1024]), &mut Profile::default())
        .unwrap();
    assert_eq!(resized.samples, [256, 512]);
    assert_eq!(resized.histogram(0).iter().sum::<u32>(), 256 * 512);
    assert_eq!(resized.outside(0), [0, 0, 0]);
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn interpolated_waveforms_conserve_weight_reduce_column_beats_and_track_sub_bin_levels() {
    let engine = Engine::new().unwrap();
    let scale = crate::scopes::DENSITY_SCALE;
    let mut buffer = image(&engine, &[0.5; 18]);
    for width in [341, 490, 512] {
        buffer.buffer = engine
            .gpu
            .upload("Flat waveform fixture", &vec![0.5_f32; width as usize * 3])
            .unwrap();
        buffer.description.extent = [width, 1];
        buffer.view = View {
            crop: [0, 0, width, 1],
            rotation: 0,
            extent: [width, 1],
        };
        let data = engine
            .scopes(&buffer, None, &mut Profile::default())
            .unwrap();
        for channel in 0..4 {
            assert_eq!(data.histogram(channel).iter().sum::<u32>(), width);
            assert_eq!(data.density(channel).iter().sum::<u32>(), width * scale);
        }
        let columns: Vec<u32> = (0..256)
            .map(|x| (0..256).map(|y| data.density(0)[y * 256 + x]).sum())
            .collect();
        // Hard placement alternates between one and two samples per column here.
        assert!(columns.iter().max().unwrap() - columns.iter().min().unwrap() <= scale / 4);
        assert_eq!(
            data.density(0)[127 * 256..129 * 256].iter().sum::<u32>(),
            width * scale
        );

        buffer.buffer = engine
            .gpu
            .upload(
                "Sub-bin waveform fixture",
                &vec![0.5001_f32; width as usize * 3],
            )
            .unwrap();
        let shifted = engine
            .scopes(&buffer, None, &mut Profile::default())
            .unwrap();
        assert_eq!(data.histogram(0), shifted.histogram(0));
        assert_ne!(data.density(0), shifted.density(0));
        assert_eq!(shifted.density(0).iter().sum::<u32>(), width * scale);
        for (value, outside) in [(-0.1_f32, [width, 0, 0]), (1.1, [0, width, 0])] {
            buffer.buffer = engine
                .gpu
                .upload(
                    "Waveform boundary fixture",
                    &vec![value; width as usize * 3],
                )
                .unwrap();
            let clipped = engine
                .scopes(&buffer, None, &mut Profile::default())
                .unwrap();
            assert_eq!(clipped.outside(0), outside);
            assert_eq!(clipped.density(0).iter().sum::<u32>(), width * scale);
        }
    }
}

fn image(engine: &Engine, pixels: &[f32]) -> RenderedImage {
    RenderedImage {
        buffer: engine.gpu.upload("Retained fixture", pixels).unwrap(),
        description: Description {
            view_baked: false,
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
            processing_version: None,
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
    let scale_rgb = |factor| calibration::ColorParams {
        source: [99, 99, 0, 0],
        matrix: [
            [factor, 0., 0., 0.],
            [0., factor, 0., 0.],
            [0., 0., factor, 0.],
        ],
    };
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
                params: scale_rgb(2.),
                space: ColorSpace::Srgb,
            },
            &[root],
        )
        .unwrap();
    let b = graph
        .add(
            Operation::Saturation {
                params: scale_rgb(3.),
                space: ColorSpace::Srgb,
            },
            &[a],
        )
        .unwrap();
    let c = graph
        .add(
            Operation::Saturation {
                params: scale_rgb(0.5),
                space: ColorSpace::Srgb,
            },
            &[b],
        )
        .unwrap();
    let mut profile = Profile::new(crate::profile::Mode::Gpu);
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
    let bytes = engine
        .gpu
        .readback_texture(texture.texture(), [2, 2], 256)
        .unwrap();
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
    let mut profile = Profile::new(crate::profile::Mode::Gpu);
    let mut artifacts = engine
        .render_artifacts(&source, &settings, &["noise.edges"], &mut profile)
        .unwrap();
    assert!(!profile.host.iter().any(|t| t.stage == "prepare color mode"));
    assert!(!profile
        .gpu
        .iter()
        .any(|t| t.stage == "directional local mean" || t.stage.starts_with("column")));
    assert!(!profile
        .host
        .iter()
        .any(|t| matches!(t.stage, "prepare column cuts" | "prepare pattern recipe")));
    assert!(!profile.gpu.iter().any(|t| t.stage.starts_with("pattern")));
    let crate::inspection::Content::Image(edge) = artifacts.remove(0).content else {
        panic!("expected an image")
    };
    assert_eq!(edge.description.space, Space::ScalarField);
    assert_eq!(edge.description.layout, Layout::Scalar32Float);
    let before = engine.gpu.readback(&edge.buffer).unwrap();
    profile = Profile::new(crate::profile::Mode::Gpu);
    let mut columns = engine
        .render_artifacts(&source, &settings, &["column.layers"], &mut profile)
        .unwrap();
    assert!(profile.gpu.iter().any(|t| t.stage == "column conditioning"));
    assert!(!profile.gpu.iter().any(|t| matches!(
        t.stage,
        "noise bilateral base"
            | "directional local mean"
            | "reconstruction luminance"
            | "residual detail mixing"
            | "tone"
    )));
    assert!(!profile.host.iter().any(|t| t.stage == "prepare color mode"));
    let crate::inspection::Content::Image(column) = columns.remove(0).content else {
        panic!("expected conditioned layers")
    };
    assert_eq!(column.description.space, Space::CalibratedLayers);
    let column_before = engine.gpu.readback(&column.buffer).unwrap();
    profile = Profile::new(crate::profile::Mode::Gpu);
    let raw = engine
        .render_artifacts(&source, &settings, &["detail.raw-ratio"], &mut profile)
        .unwrap();
    assert!(!profile
        .host
        .iter()
        .any(|t| t.stage == "prepare pattern recipe"));
    assert!(!profile
        .gpu
        .iter()
        .any(|t| t.stage.starts_with("pattern") || t.stage == "residual detail mixing"));
    drop(raw);
    profile = Profile::new(crate::profile::Mode::Gpu);
    let mask = engine
        .render_artifacts(&source, &settings, &["pattern.mask"], &mut profile)
        .unwrap();
    assert!(profile.gpu.iter().any(|t| t.stage == "pattern minmax mask"));
    assert!(!profile.gpu.iter().any(|t| matches!(
        t.stage,
        "pattern horizontal swaps"
            | "pattern vertical swaps"
            | "pattern ratio blend"
            | "residual detail mixing"
            | "tone"
    )));
    drop(mask);
    profile = Profile::new(crate::profile::Mode::Gpu);
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
    profile = Profile::new(crate::profile::Mode::Gpu);
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
    let scopes = engine
        .scopes(&signal, None, &mut Profile::default())
        .unwrap();
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
    assert!(
        engine.gpu.readback(&column.buffer).unwrap() == column_before,
        "retained conditioned layers changed"
    );
}

#[test]
fn color_dq_selection_is_auto_only_with_reconstruction_and_explicit_off_wins() {
    let mut s = Settings::new("Sunlight", "Standard");
    assert!(!s.color_dq_selected());
    s.reconstruction = Some(Default::default());
    assert!(s.color_dq_selected());
    s.color_dq_gain = Some(0.);
    assert!(!s.color_dq_selected());
    s.reconstruction = None;
    s.color_dq_gain = Some(1.);
    assert!(s.color_dq_selected());
    s.color_dq_gain = Some(f32::NAN);
    assert!(s.validate().is_err());
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn automatic_color_dq_matches_native_gain_manual_override_and_preserves_source() {
    let dir = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let source = Source::open(dir.join("blue-parking-sign.x3f")).unwrap();
    let engine = Engine::new().unwrap();
    let mut s = Settings::new("Sunlight", "Standard");
    s.reconstruction = Some(Default::default());
    s.repair = None;
    s.highlight_restoration = false;
    s.despeckle = false;
    s.row_correction = false;
    let recipe = crate::noise::couplings(
        &source.metadata,
        &s.white_balance,
        s.reconstruction.unwrap(),
    )
    .unwrap();
    let gain = crate::noise::processing_gain(&source.metadata)
        .unwrap()
        .1
        .min(recipe.color_dq_limit);
    let auto = engine
        .render_artifacts(&source, &s, &["color-dq.rgb"], &mut Profile::default())
        .unwrap();
    let crate::inspection::Content::Image(auto) = &auto[0].content else {
        panic!()
    };
    let before = engine.gpu.readback(&auto.buffer).unwrap();
    s.color_dq_gain = Some(gain);
    let manual = engine
        .render_artifacts(&source, &s, &["color-dq.rgb"], &mut Profile::default())
        .unwrap();
    let crate::inspection::Content::Image(manual) = &manual[0].content else {
        panic!()
    };
    assert!(
        engine.gpu.readback(&manual.buffer).unwrap() == before,
        "automatic and explicit native gain differ"
    );
    s.color_dq_gain = Some(0.);
    let mut profile = Profile::new(crate::profile::Mode::Gpu);
    engine
        .render_artifacts(&source, &s, &["reconstruction.rgb"], &mut profile)
        .unwrap();
    assert!(!profile.gpu.iter().any(|t| t.stage == "ColorDQ"));
    assert!(
        engine.gpu.readback(&auto.buffer).unwrap() == before,
        "manual/off route changed retained auto image"
    );
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn native_sharpen_is_before_tone_reuses_signed_signal_and_keeps_inputs_immutable() {
    let dir = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let source = Source::open(dir.join("blue-parking-sign.x3f")).unwrap();
    let engine = Engine::new().unwrap();
    for reconstruction in [false, true] {
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.sharpen_amount = Some(0.75);
        if reconstruction {
            settings.reconstruction = Some(Default::default());
        }
        let root = if reconstruction {
            "reconstruction.rgb"
        } else {
            "color.rgb"
        };
        let mut profile = Profile::new(crate::profile::Mode::Gpu);
        let mut artifacts = engine
            .render_artifacts(
                &source,
                &settings,
                &[root, "detail.signal", "sharpen.rgb"],
                &mut profile,
            )
            .unwrap();
        assert!(!profile.host.iter().any(|t| t.stage == "prepare tone curve"));
        assert!(!profile.gpu.iter().any(|t| t.stage == "tone"));
        let crate::inspection::Content::Image(sharp) = artifacts.pop().unwrap().content else {
            panic!()
        };
        let crate::inspection::Content::Image(signal) = artifacts.pop().unwrap().content else {
            panic!()
        };
        let crate::inspection::Content::Image(input) = artifacts.pop().unwrap().content else {
            panic!()
        };
        assert_eq!(sharp.description.space, input.description.space);
        assert!(matches!(sharp.description.space, Space::LinearRgb(_)));
        assert_eq!(signal.description.space, Space::SignedDetail);
        let before = engine.gpu.readback(&input.buffer).unwrap();
        let signal_before = engine.gpu.readback(&signal.buffer).unwrap();
        settings.sharpen_amount = Some(1.5);
        let stronger = engine
            .render_artifacts(
                &source,
                &settings,
                &["sharpen.rgb"],
                &mut Profile::default(),
            )
            .unwrap();
        let crate::inspection::Content::Image(stronger) = &stronger[0].content else {
            panic!()
        };
        let bytes = engine.gpu.readback(&stronger.buffer).unwrap();
        assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .all(|v| v.is_finite()));
        assert!(
            engine.gpu.readback(&input.buffer).unwrap() == before,
            "sharp processing changed earlier RGB"
        );
        assert!(
            engine.gpu.readback(&signal.buffer).unwrap() == signal_before,
            "sharp processing changed retained signed signal"
        );
    }
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
    for decay in [0.95, 0.99, 1.] {
        settings
            .reconstruction
            .as_mut()
            .unwrap()
            .overrides
            .set(crate::noise::Parameter::MeanDecay, Some(decay));
        let base = read(&settings, "noise.layers");
        assert_ne!(base, baseline, "manual Mean decay {decay} was still capped");
        assert!(bytemuck::cast_slice::<u8, f32>(&base)
            .iter()
            .all(|v| v.is_finite()));
        let result = read(&settings, "reconstruction.rgb");
        assert!(bytemuck::cast_slice::<u8, f32>(&result)
            .iter()
            .all(|v| v.is_finite()));
    }
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
fn reconstruction_originals_stay_finite_across_noise_choices() {
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
            settings.color_dq_gain = Some(if choice == 4 { 1. } else { 0. });
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
        }
    }
}
