//! Checks control preparation, retained GPU image lifetimes, scratch branches, and texture/export consumers.

use super::*;

#[test]
fn control_matrices_preserve_gray_and_signed_channel_gains() {
    let apply = |params: calibration::ColorParams, rgb: [f32; 3]| -> [f32; 3] {
        params
            .matrix
            .map(|row| (0..3).map(|c| row[c] * rgb[c]).sum())
    };
    assert_eq!(
        apply(gains([1, 1, 0, 0], [1., 2., 0.5]), [-0.25, 2., 1.]),
        [-0.25, 4., 0.5]
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
                space: ColorSpace::Srgb,
            },
            &[],
        )
        .unwrap();
    let a = graph
        .add(
            Operation::Saturation {
                params: gains([99, 99, 0, 0], [2.; 3]),
                space: ColorSpace::Srgb,
            },
            &[root],
        )
        .unwrap();
    let b = graph
        .add(
            Operation::Saturation {
                params: gains([99, 99, 0, 0], [3.; 3]),
                space: ColorSpace::Srgb,
            },
            &[a],
        )
        .unwrap();
    let c = graph
        .add(
            Operation::Saturation {
                params: gains([99, 99, 0, 0], [0.5; 3]),
                space: ColorSpace::Srgb,
            },
            &[b],
        )
        .unwrap();
    let mut profile = Profile::new(true);
    // Retain an intermediate while later nodes execute; it must not be recycled into c.
    let retained = engine
        .execute(Input::Rendered(&input), &graph, Some(a), &mut profile)
        .unwrap()
        .unwrap();
    let a_bytes = engine.gpu.readback(&retained.buffer).unwrap();
    for (actual, expected) in bytemuck::cast_slice::<u8, f32>(&a_bytes)
        .iter()
        .zip(&original)
    {
        assert!((actual - expected * 2.).abs() < 1e-6);
    }
    let latest = engine
        .execute(Input::Rendered(&input), &graph, Some(c), &mut profile)
        .unwrap()
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
