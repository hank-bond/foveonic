//! Compares GPU clipped-layer restoration, immutable flag connectivity, and density with native fixtures.

use super::*;
use crate::gpu::Binding::{Read, Write};
use crate::source::Matrix;
fn values(v: &serde_json::Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}
fn threshold_metadata() -> Metadata {
    let mut metadata = Metadata {
        processing_version: None,
        width: 64,
        height: 128,
        rotation: 0,
        properties: Default::default(),
        lists: Default::default(),
        matrices: Default::default(),
    };
    for (name, values) in [
        ("ActiveImageArea", vec![8., 34., 48., 64.]),
        ("DarkShieldColRange", vec![0., 2., 56., 63.]),
        ("SatMapR", vec![8192.]),
        ("SatMapG", vec![8192.]),
        ("SatMapB", vec![8192.]),
    ] {
        metadata.matrices.insert(
            name.into(),
            Matrix {
                encoding: MatrixEncoding::Uint32,
                representation: "u32",
                dimensions: vec![values.len()],
                values,
            },
        );
    }
    metadata
}

#[test]
fn threshold_offsets_scale_only_camera_threshold_and_preserve_reference_defaults() {
    let mut metadata = threshold_metadata();
    for (net, expected) in [(0., 7000_f32), (1.71_f32, 6370.), (2.5, 4000.)] {
        metadata.matrices.insert(
            "ExpNet".into(),
            Matrix {
                encoding: MatrixEncoding::Float32,
                representation: "f32",
                dimensions: vec![1],
                values: vec![net as f64],
            },
        );
        let automatic = prepare(&metadata, None).unwrap();
        assert_eq!(automatic.params.value[0].to_bits(), expected.to_bits());
        for ev in [-2_f32, -1., 0., 0.5, 1., 2.] {
            let manual = prepare(&metadata, Some(ev)).unwrap();
            assert_eq!(manual.params.value[0], expected * ev.exp2());
            assert_eq!(manual.flags, automatic.flags);
            let mut normalized = manual.params;
            normalized.value[0] = expected;
            assert_eq!(
                bytemuck::bytes_of(&normalized),
                bytemuck::bytes_of(&automatic.params)
            );
            assert_eq!(manual.key() == automatic.key(), ev == 0.);
        }
    }
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -2.01, 2.01] {
        assert!(prepare(&metadata, Some(invalid)).is_err());
        let settings = crate::render::Settings {
            highlight_restoration: false,
            highlight_threshold_ev: Some(invalid),
            ..Default::default()
        };
        assert!(settings.validate_values().is_err());
    }
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn gpu_threshold_changes_only_brightness_blend_and_keeps_density_and_input() {
    let gpu = Gpu::test_executor();
    let count = 64 * 128;
    let mut prepared = Prepared {
        grid: None,
        params: Params {
            source: [64, 128, count, count.div_ceil(256)],
            area: [8, 2, 48, 97],
            value: [5500., 1., 0., 0.],
        },
        // No valid blue donors: the correction table stays empty, making the
        // expected brightness blend independent of the donor implementation.
        flags: Some(vec![7; count as usize].into()),
    };
    let mut reference_density = None;
    {
        let samples = [8000_f32, 6500., 5000.].repeat(count as usize);
        let input = gpu.upload("Threshold fixture", &samples).unwrap();
        for ev in [-2_f32, -1., 0., 0.25, 1., 2.] {
            let threshold = 5500. * ev.exp2();
            prepared.params.value[0] = threshold;
            let result =
                apply(&gpu, Some(&input), &prepared, true, &mut Profile::default()).unwrap();
            let density = gpu.readback(result.density.as_ref().unwrap()).unwrap();
            assert_eq!(
                density,
                *reference_density.get_or_insert_with(|| density.clone())
            );
            let pixels = gpu.readback(result.pixels.as_ref().unwrap()).unwrap();
            let actual = bytemuck::cast_slice::<u8, f32>(&pixels);
            let p = 40 * 64 + 20;
            let den = bytemuck::cast_slice::<u8, f32>(&density)[p];
            assert_eq!(den, 1.);
            let brightness = 8000_f32 * (2. / 3.) + 6500_f32 * (1. / 3.);
            let bright = ((brightness - threshold) / 1000.).clamp(0., 1.);
            let weight = ((bright * bright + den * den) * 0.5).sqrt();
            let expected = [
                8000.,
                (1. - weight) * 6500. + weight * 8000.,
                (1. - weight) * 5000. + weight * 8000.,
            ];
            for (actual, expected) in actual[p * 3..][..3].iter().zip(expected) {
                assert!((actual - expected).abs() < 0.004, "EV={ev}");
            }
            assert_eq!(&actual[..3], &samples[..3], "outside work area changed");
            assert_eq!(
                gpu.readback(&input).unwrap(),
                bytemuck::cast_slice::<f32, u8>(&samples)
            );
        }
    }
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn donor_bins_keep_integer_boundaries_and_adjacent_floats_distinct() {
    let gpu = Gpu::test_executor();
    let around = |value: f32| {
        [
            f32::from_bits(value.to_bits() - 1),
            value,
            f32::from_bits(value.to_bits() + 1),
        ]
    };
    let mut rgb: Vec<f32> = around(50.)
        .into_iter()
        .chain(around(70.))
        .flat_map(|red| [red, 100., 10.])
        .collect();
    rgb.extend([
        297.484_3,
        12345.6,
        10.,
        2_295.820_8,
        2_387.653_6,
        10.,
        2_510.251_7,
        2_824.033_4,
        10.,
        1_885.496_5,
        2223.919,
        10.,
        2_007.869_8,
        2_294.708_5,
        10.,
        2_956.256,
        2_798.288_8,
        10.,
    ]);
    // Compare the rounded f32 operands as an exact f64 ratio, independent of the GPU guard.
    let expected: Vec<u32> = rgb
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| f64::from(c[0] * 255.) / f64::from(c[0] + c[1]))
        .map(|ratio| ratio.floor() as u32)
        .collect();
    assert_eq!(
        expected,
        [84, 85, 85, 104, 105, 105, 5, 124, 119, 116, 118, 130]
    );
    let input = gpu.upload("Boundary RGB", &rgb).unwrap();
    let donors = gpu
        .upload("Boundary donors", &(0_u32..12).collect::<Vec<_>>())
        .unwrap();
    let counts = gpu.upload("Boundary donor total", &[0_u32, 12]).unwrap();
    let mut marks = gpu.storage("Boundary bins", 12 * 4).unwrap();
    let mut histogram = gpu.storage("Boundary histogram", 256 * 4).unwrap();
    let params = gpu.uniform(&Params {
        source: [12, 1, 12, 1],
        area: [0; 4],
        value: [0., 1., 0., 0.],
    });
    gpu.dispatch(
        Kernel::HighlightBinHistogram,
        &mut [
            (2, Read(&params)),
            (5, Read(&input)),
            (6, Write(marks.write())),
            (7, Read(&counts)),
            (8, Read(&donors)),
            (11, Write(histogram.write())),
        ],
        [1, 1],
        &mut Default::default(),
    );
    let bytes = gpu.readback(&marks).unwrap();
    assert_eq!(bytemuck::cast_slice::<u8, u32>(&bytes), expected.as_slice());
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn donor_bin_grouping_is_stable_across_blocks_partial_tail_and_empty_bins() {
    let gpu = Gpu::test_executor();
    let rgb: Vec<f32> = (0..8192)
        .flat_map(|i| {
            let red = if i % 16 != 0 {
                128.
            } else {
                match i / 16 % 5 {
                    0 => f32::from_bits(64_f32.to_bits() - 1),
                    1 => 64.,
                    2 => f32::from_bits(64_f32.to_bits() + 1),
                    3 => 4.,
                    _ => 1024.,
                }
            };
            // Exact binary ratios isolate the ordered sum from division accuracy.
            // Large and small terms make reassociation observably different.
            [red * 32., 4096., if i % 4 == 0 { 1. } else { 32768. }]
        })
        .collect();
    let ids: Vec<u32> = (0..2053).map(|i| i * 3).collect();
    let bucket = |p: u32| {
        let c = &rgb[p as usize * 3..][..3];
        (f64::from(c[0] * 255.) / f64::from(c[0] + c[1])).floor() as u32
    };
    let mut expected = ids.clone();
    expected.sort_by_key(|p| bucket(*p));
    let input = gpu.upload("Bin RGB", &rgb).unwrap();
    let donors = gpu.upload("Bin donors", &ids).unwrap();
    let mut totals = [0_u32; 33];
    totals[32] = ids.len() as u32;
    let counts = gpu.upload("Bin donor total", &totals).unwrap();
    let mut marks = gpu.storage("Reused bin scratch", 8192 * 4).unwrap();
    let mut histogram = gpu.storage("Bin histograms", 32 * 256 * 4).unwrap();
    let mut starts = gpu.storage("Bin offsets", 257 * 4).unwrap();
    let mut sorted = gpu.storage("Stable bin output", 8192 * 4).unwrap();
    let mut lookup = gpu
        .upload("Ordered bin sums and counts", &[123_f32; 512])
        .unwrap();
    let params = gpu.uniform(&Params {
        source: [128, 64, 8192, 32],
        area: [0; 4],
        value: [0., 1., 0., 0.],
    });
    gpu.dispatch(
        Kernel::HighlightBinHistogram,
        &mut [
            (2, Read(&params)),
            (5, Read(&input)),
            (6, Write(marks.write())),
            (7, Read(&counts)),
            (8, Read(&donors)),
            (11, Write(histogram.write())),
        ],
        [32, 1],
        &mut Default::default(),
    );
    gpu.dispatch(
        Kernel::HighlightBinPrefix,
        &mut [
            (2, Read(&params)),
            (7, Read(&counts)),
            (11, Write(histogram.write())),
            (12, Write(starts.write())),
        ],
        [1, 1],
        &mut Default::default(),
    );
    gpu.dispatch(
        Kernel::HighlightBinScatter,
        &mut [
            (2, Read(&params)),
            (6, Read(&marks)),
            (7, Read(&counts)),
            (8, Read(&donors)),
            (10, Write(sorted.write())),
            (11, Read(&histogram)),
            (12, Read(&starts)),
        ],
        [32, 1],
        &mut Default::default(),
    );
    let bytes = gpu.readback(&sorted).unwrap();
    assert_eq!(
        &bytemuck::cast_slice::<u8, u32>(&bytes)[..ids.len()],
        expected
    );
    let bytes = gpu.readback(&starts).unwrap();
    for (b, &start) in bytemuck::cast_slice::<u8, u32>(&bytes).iter().enumerate() {
        assert_eq!(
            start as usize,
            ids.iter().filter(|p| bucket(**p) < b as u32).count()
        );
    }
    let bytes = gpu.readback(&marks).unwrap();
    assert_eq!(
        &bytemuck::cast_slice::<u8, u32>(&bytes)[..ids.len()],
        ids.iter().map(|p| bucket(*p)).collect::<Vec<_>>()
    );
    gpu.dispatch(
        Kernel::HighlightTable,
        &mut [
            (5, Read(&input)),
            (9, Write(lookup.write())),
            (10, Read(&sorted)),
            (12, Read(&starts)),
        ],
        [256, 1],
        &mut Default::default(),
    );
    let mut expected_table = [0_f32; 512];
    for &id in &ids {
        let c = &rgb[id as usize * 3..][..3];
        let b = bucket(id) as usize;
        expected_table[b] += c[2] / c[1];
        expected_table[256 + b] += 1.;
    }
    assert!(expected_table[256 + 127] > 128.);
    let bytes = gpu.readback(&lookup).unwrap();
    assert_eq!(bytes, bytemuck::cast_slice::<f32, u8>(&expected_table));
}

#[test]
fn camera_saturation_maps_preserve_original_positions_and_validate_coverage() {
    let mut m = Metadata {
        processing_version: None,
        width: 8,
        height: 8,
        rotation: 0,
        properties: Default::default(),
        lists: Default::default(),
        matrices: Default::default(),
    };
    let matrix = |values: Vec<f64>| Matrix {
        encoding: MatrixEncoding::Uint32,
        representation: "u32",
        dimensions: vec![values.len()],
        values,
    };
    m.matrices
        .insert("SatMapR".into(), matrix(vec![1., 2., 61.]));
    m.matrices
        .insert("SatMapFullG".into(), matrix(vec![2., 7.]));
    m.matrices.insert("SatMapB".into(), matrix(vec![64.]));
    let flags = saturation(&m).unwrap();
    assert_eq!(&flags[..8], &[0, 4, 6, 0, 0, 0, 0, 2]);
    m.matrices.get_mut("SatMapR").unwrap().values[2] = 60.;
    assert!(saturation(&m).is_err());
    m.matrices.remove("SatMapR");
    assert!(saturation(&m).is_err());
}
#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn highlight_density_inspection_skips_photographic_prefix_and_retains_original_provenance() {
    use crate::{
        inspection::Content,
        render::{Engine, Settings},
        source::Source,
    };
    let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("sunlit-red-flower.x3f");
    let mut source = Source::open(path).unwrap();
    let metadata = std::sync::Arc::make_mut(&mut source.metadata);
    metadata.lists.clear(); // No WB/color choices are needed to inspect original flags.
    metadata.matrices.remove("BadnessTable");
    let settings = Settings::new("missing WB", "missing mode");
    let engine = Engine::new().unwrap();
    let mut profile = Profile::new(crate::profile::Mode::Gpu);
    let artifacts = engine
        .render_artifacts(&source, &settings, &["highlight.density"], &mut profile)
        .unwrap();
    assert!(!profile.host.iter().any(|t| [
        "prepare black",
        "prepare calibration",
        "prepare repair records",
        "prepare reference noise",
        "prepare color matrix"
    ]
    .contains(&t.stage)));
    if engine
        .device()
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY)
    {
        assert_eq!(profile.gpu.len(), 3);
    }
    let Content::Image(image) = &artifacts[0].content else {
        panic!()
    };
    assert_eq!(
        image.description.layout,
        crate::image::Layout::Scalar32Float
    );
    let gpu = Gpu::from_device(
        engine.device().clone(),
        engine.queue().clone(),
        "test".into(),
    );
    let before = gpu.readback(image.buffer()).unwrap();
    let values = bytemuck::cast_slice::<u8, f32>(&before);
    assert!(values.iter().all(|v| (0. ..=1.).contains(v)));
    assert!(values.contains(&1.));
    let mut changed = settings;
    changed.highlight_threshold_ev = Some(1.);
    engine
        .render_artifacts(
            &source,
            &changed,
            &["highlight.density"],
            &mut Profile::default(),
        )
        .unwrap();
    assert!(
        gpu.readback(image.buffer()).unwrap() == before,
        "retained original clipping density changed"
    );
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn gpu_highlight_restoration_matches_native_saturation_and_density() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/data/highlight-reference.json")).unwrap();
    let w = f["width"].as_u64().unwrap() as u32;
    let h = f["height"].as_u64().unwrap() as u32;
    let raw = values(&f["flags"]);
    let flags: Vec<_> = raw
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| (c[0] as u32) * 4 + (c[1] as u32) * 2 + c[2] as u32)
        .collect();
    let p = Prepared {
        grid: None,
        params: Params {
            source: [w, h, w * h, (w * h).div_ceil(256)],
            area: values(&f["area"])
                .into_iter()
                .map(|v| v as u32)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            value: [5500., 1., 0., 0.],
        },
        flags: Some(flags.into()),
    };
    let gpu = Gpu::test_executor();
    {
        let samples: Vec<_> = values(&f["input"]).into_iter().map(|v| v as f32).collect();
        let input = gpu.upload("Immutable highlight fixture", &samples).unwrap();
        let result = apply(&gpu, Some(&input), &p, true, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(result.pixels.as_ref().unwrap()).unwrap();
        for (i, (&actual, reference)) in bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .zip(values(&f["output"]))
            .enumerate()
        {
            assert!(
                (actual - reference as f32).abs() < 0.012,
                "sample {i}: {actual} != {reference}"
            );
        }
        let bytes = gpu.readback(result.density.as_ref().unwrap()).unwrap();
        for (&actual, reference) in bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .zip(values(&f["density"]))
        {
            assert!((actual - reference as f32).abs() < 0.000001);
        }
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&samples)
        );
    }
    let result = apply(&gpu, None, &p, true, &mut Profile::default()).unwrap();
    assert!(result.pixels.is_none());
    assert!(result.density.is_some());
}

#[test]
fn reduced_sensor_padding_keeps_highlight_halo_inside_the_original() {
    let mut metadata = threshold_metadata();
    metadata.matrices.get_mut("ActiveImageArea").unwrap().values[3] = 110.;
    let prepared = prepare(&metadata, None).unwrap();
    assert_eq!(prepared.params.area, [8, 2, 48, 125]);
    metadata.matrices.get_mut("ActiveImageArea").unwrap().values[3] = 128.;
    assert!(prepare(&metadata, None).is_err());
}

#[test]
#[ignore = "requires GPU"]
fn preview_original_clipping_survives_averaging() {
    use crate::{
        nodes::sensor::pool,
        render::{Scale, WorkingGrid},
    };
    let gpu = Gpu::test_executor();
    let extent = [64u32, 64u32];
    let count = 64 * 64;
    for divisor in [2, 4, 8, 16] {
        let grid = WorkingGrid::new(extent, Scale::new(divisor).unwrap(), 8)
            .unwrap()
            .with_halo(4)
            .unwrap();
        let pooling = pool::Prepared::new(extent, divisor, pool::Signal::Rgb)
            .unwrap()
            .with_extent(grid.extent)
            .unwrap()
            .with_origin(grid.origin);
        for saturated_cell in [false, true] {
            let mut pixels = [5000f32, 5000., 4000.].repeat(count as usize);
            let mut flags = vec![0u32; (grid.extent[0] * grid.extent[1]) as usize];
            let first = 16u32;
            let end = if saturated_cell {
                first + divisor
            } else {
                first + 1
            };
            for y in first..end {
                for x in first..end {
                    let p = (y * 64 + x) as usize;
                    pixels[3 * p + 2] = 12000.;
                }
            }
            let source = gpu.upload("Calibrated source", &pixels).unwrap();

            let input = pool::apply(&gpu, &source, &pooling, &mut Profile::default()).unwrap();
            let cell = ((first / divisor + grid.origin[1]) * grid.extent[0]
                + first / divisor
                + grid.origin[0]) as usize;
            flags[cell] = 1;
            let before = gpu.readback(&input).unwrap();
            let recovery = Prepared {
                params: Params {
                    source: [64, 64, count, count.div_ceil(256)],
                    area: [2, 2, 61, 61],
                    value: [20000., 0., 0., 0.],
                },
                flags: Some(flags.into()),
                grid: Some(grid),
            };
            let actual = apply(
                &gpu,
                Some(&input),
                &recovery,
                false,
                &mut Profile::default(),
            )
            .unwrap()
            .pixels
            .unwrap();
            let bytes = gpu.readback(&actual).unwrap();
            let values: &[f32] = bytemuck::cast_slice(&bytes);
            let p = ((16 / divisor + grid.origin[1]) * grid.extent[0]
                + 16 / divisor
                + grid.origin[0]) as usize;
            assert!(
                (values[3 * p + 2] - 4000.).abs() < 0.1,
                "divisor {divisor}, whole cell {saturated_cell}: {}",
                values[3 * p + 2]
            );
            assert_eq!(gpu.readback(&input).unwrap(), before);
            assert_eq!(
                gpu.readback(&source).unwrap(),
                bytemuck::cast_slice::<f32, u8>(&pixels)
            );
            assert!(values.iter().all(|v| v.is_finite()));
        }
    }
}

#[test]
fn original_clipping_projects_runs_points_partial_cells_and_halos() {
    use crate::render::{Scale, WorkingGrid};
    let mut m = threshold_metadata();
    m.width = 63;
    m.height = 127;
    let count = m.width * m.height;
    for c in ["R", "G", "B"] {
        m.matrices.get_mut(&format!("SatMap{c}")).unwrap().values = vec![count as f64];
    }
    let points = [0usize, 3, 62, 65, 16 * 63 + 17, count - 1];
    let mut runs = Vec::new();
    let mut start = 0;
    for p in points {
        runs.extend([(p - start) as f64, 1.]);
        start = p + 1;
    }
    runs.push((count - start) as f64);
    m.matrices.get_mut("SatMapR").unwrap().values = runs;
    m.matrices.remove("SatMapB");
    m.matrices.insert(
        "SatMapFullB".into(),
        Matrix {
            encoding: MatrixEncoding::Uint32,
            representation: "fixture",
            dimensions: vec![3],
            values: vec![3., 64., (count - 1) as f64],
        },
    );
    let native = saturation(&m).unwrap();
    assert_eq!(native[3], 5);
    for divisor in [2, 4, 8, 16, 64, 1 << 31] {
        let grid = WorkingGrid::new([63, 127], Scale::new(divisor).unwrap(), 8)
            .unwrap()
            .with_halo(4)
            .unwrap();
        let actual = clipping(&m, Some(grid)).unwrap();
        let mut expected = vec![0u32; actual.len()];
        for (y, row) in native.chunks(m.width).enumerate() {
            for (x, &flag) in row.iter().enumerate() {
                let xx = x as u32 / divisor + grid.origin[0];
                let yy = y as u32 / divisor + grid.origin[1];
                expected[(yy * grid.extent[0] + xx) as usize] |= flag;
            }
        }
        for y in 0..grid.extent[1] {
            for x in 0..grid.extent[0] {
                let xx = x.saturating_sub(grid.origin[0]).min(grid.logical[0] - 1) + grid.origin[0];
                let yy = y.saturating_sub(grid.origin[1]).min(grid.logical[1] - 1) + grid.origin[1];
                expected[(y * grid.extent[0] + x) as usize] =
                    expected[(yy * grid.extent[0] + xx) as usize];
            }
        }
        assert_eq!(actual, expected, "divisor {divisor}");
        assert_eq!(actual.len(), (grid.extent[0] * grid.extent[1]) as usize);
    }
}
