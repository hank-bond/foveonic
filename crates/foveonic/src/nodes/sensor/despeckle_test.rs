//! Checks reference lookup rounding, deterministic detection/repair, and the two photographed residual defects.
use super::*;
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../tests/data/despeckle-reference.json")).unwrap()
}
fn floats(value: &serde_json::Value) -> Vec<f32> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect()
}
#[test]
fn metadata_tables_match_native_rounding_and_reject_invalid_values() {
    let f = fixture();
    let t = tables([2.3, 1.1, 0.8], [15., 15., 5.]).unwrap();
    for v in f["tables"].as_array().unwrap() {
        let i =
            v["channel"].as_u64().unwrap() as usize * 4096 + v["bin"].as_u64().unwrap() as usize;
        assert_eq!(t[i], v["value"].as_f64().unwrap() as f32);
    }
    assert!(tables([0., 1., 1.], [15.; 3]).is_err());
    assert!(tables([1.; 3], [f64::NAN, 15., 5.]).is_err());
    assert!(parameters(64, 64, [4, 8, 56, 56]).is_err());
}
fn metadata() -> Metadata {
    let mut metadata = Metadata {
        processing_version: None,
        width: 64,
        height: 64,
        rotation: 0,
        properties: Default::default(),
        matrices: Default::default(),
        lists: Default::default(),
    };
    for (name, values) in [
        ("ActiveImageArea", vec![8., 8., 55., 55.]),
        ("KeepImageArea", vec![0., 0., 63., 63.]),
    ] {
        metadata.matrices.insert(
            name.into(),
            crate::source::Matrix {
                encoding: MatrixEncoding::Uint16,
                representation: "f64",
                dimensions: vec![4],
                values,
            },
        );
    }
    metadata
}

#[test]
fn preparation_uses_actual_calibration_factors_with_explicit_noise_dose() {
    // Clearing WB lists catches a second metadata lookup. Thresholds must use
    // the factors calibration already selected and rounded.
    let mut metadata = metadata();
    let gains = [4.6_f32, 2.2, 1.6];
    let expected = tables(gains.map(f64::from), [15., 15., 5.]).unwrap();
    assert_ne!(
        expected,
        tables(gains.map(|v| f64::from(v) * 0.5), [15., 15., 5.]).unwrap()
    );
    let prepared = prepare(&metadata, gains, 1.).unwrap();
    assert_eq!(prepared.tables, expected);
    metadata.matrices.insert(
        "DespAdjust".into(),
        crate::source::Matrix {
            encoding: MatrixEncoding::Int16,
            representation: "f64",
            dimensions: vec![3],
            values: vec![10., 10., 5.],
        },
    );
    assert_eq!(prepare(&metadata, gains, 1.).unwrap().tables, expected);
    let adjustment = metadata.matrices.get_mut("DespAdjust").unwrap();
    adjustment.encoding = MatrixEncoding::Float32;
    adjustment.values = vec![10.1, 7.3, 2.8];
    assert_eq!(
        prepare(&metadata, gains, 1.).unwrap().tables,
        tables(gains.map(f64::from), [10.1_f32, 7.3, 2.8].map(f64::from)).unwrap()
    );
    for scale in [0.75_f32, 1.3] {
        let adjustment = [10.1_f32, 7.3, 2.8].map(|v| f64::from(v) * f64::from(scale));
        assert_eq!(
            prepare(&metadata, gains, scale).unwrap().tables,
            tables(gains.map(f64::from), adjustment).unwrap()
        );
    }
    for invalid in [0., -1., f32::NAN, f32::INFINITY] {
        assert!(prepare(&metadata, [invalid, gains[1], gains[2]], 1.).is_err());
        assert!(prepare(&metadata, gains, invalid).is_err());
    }
    metadata.matrices.get_mut("DespAdjust").unwrap().values[0] = f64::NAN;
    assert!(prepare(&metadata, gains, 1.).is_err());
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR"]
fn original_presets_share_calibration_factors_with_despeckle_thresholds() {
    let root = std::path::PathBuf::from(
        std::env::var_os("FOVEONIC_FIXTURE_DIR").expect("fixture directory"),
    );
    for name in [
        "blue-parking-sign",
        "glossy-leaves-and-buds",
        "rocks-and-tree-shadows",
        "sunlit-red-flower",
        "turquoise-still-life",
        "yellow-flowers-and-stroller",
    ] {
        let source = crate::source::Source::open(root.join(format!("{name}.x3f"))).unwrap();
        let metadata = &source.metadata;
        let mut previous = None;
        let mut changed = false;
        for wb in metadata.lists["WhiteBalanceGains"].names() {
            let calibration = crate::nodes::sensor::calibration::prepare(metadata, wb).unwrap();
            let gains = std::array::from_fn(|c| calibration.params.gains[c]);
            let prepared = prepare(metadata, gains, 1.).unwrap();
            assert_eq!(
                prepared.tables,
                tables(gains.map(f64::from), [15., 15., 5.]).unwrap(),
                "{name}/{wb}"
            );
            if let Some(previous) = previous.replace(prepared.tables.clone()) {
                changed |= previous != prepared.tables;
            }
        }
        assert!(changed, "{name}: preset changes must update threshold WB");
    }
}

#[test]
fn corrections_are_independent_and_enabled_by_default() {
    let mut s = crate::render::Settings::new("Sunlight", "Standard");
    assert!(s.despeckle && matches!(s.repair, Some(crate::render::RepairThreshold::Camera)));
    s.repair = None;
    assert!(s.despeckle);
    assert!(crate::inspection::output_names(&s, None).contains(&"despeckle.layers"));
    s.despeckle = false;
    assert!(!crate::inspection::output_names(&s, None).contains(&"despeckle.layers"));
}
#[test]
#[ignore = "requires a GPU"]
fn gpu_despeckle_matches_ordered_reference_and_preserves_input() {
    let f = fixture();
    let source = floats(&f["input"]);
    let expected = floats(&f["output"]);
    let gpu = Gpu::new().unwrap();
    {
        let original = &source;
        let input = gpu.upload("Speckle fixture", original).unwrap();
        let p = Prepared {
            params: parameters(64, 64, [8, 8, 56, 56]).unwrap(),
            tables: vec![0.; 3 * 4096],
        };
        let out = apply(
            &gpu,
            &input,
            &input,
            &p,
            &mut Profile::new(crate::profile::Mode::Gpu),
        )
        .unwrap();
        let actual: Vec<f32> =
            bytemuck::cast_slice::<u8, f32>(&gpu.readback(&out).unwrap()).to_vec();
        for (i, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
            assert!((a - b).abs() < 0.002, "sample {i}: {a} != {b}");
        }
        assert_eq!(
            bytemuck::cast_slice::<u8, f32>(&gpu.readback(&input).unwrap()),
            *original
        );
    }
    // A capacity-one bank should keep the first record in row order, not
    // whichever GPU thread wins a race. The bank containing y=16 has seven candidates.
    let input = gpu.upload("Capacity fixture", &source).unwrap();
    let mut p = Prepared {
        params: parameters(64, 64, [8, 8, 56, 56]).unwrap(),
        tables: vec![0.; 3 * 4096],
    };
    p.params.source[3] = 1;
    let out = apply(
        &gpu,
        &input,
        &input,
        &p,
        &mut Profile::new(crate::profile::Mode::Off),
    )
    .unwrap();
    let data = gpu.readback(&out).unwrap();
    let data = bytemuck::cast_slice::<u8, f32>(&data);
    assert_eq!(
        &data[(16 * 64 + 20) * 3..(16 * 64 + 20) * 3 + 3],
        &source[(16 * 64 + 20) * 3..(16 * 64 + 20) * 3 + 3]
    );
    assert_ne!(
        &data[(16 * 64 + 15) * 3..(16 * 64 + 15) * 3 + 3],
        &source[(16 * 64 + 15) * 3..(16 * 64 + 15) * 3 + 3]
    );
}
#[test]
#[ignore = "requires a GPU"]
fn gpu_scale_matched_thresholds_keep_marginal_detail_and_correct_strong_defects() {
    let metadata = metadata();
    let factors = [4.6_f32, 2.2, 1.6];
    let prepared = prepare(&metadata, factors, 1.).unwrap();
    let old_tables = tables(factors.map(|v| f64::from(v) * 0.5), [15., 15., 5.]).unwrap();
    let gap = (prepared.tables[200] + old_tables[200]) * 0.5;
    assert!(prepared.tables[200] > old_tables[200]);
    let mut source = vec![100_f32; 64 * 64 * 3];
    for y in 31..=33 {
        for x in 31..=33 {
            source[(y * 64 + x) * 3] = 200. - gap;
        }
    }
    let marginal = (32 * 64 + 32) * 3;
    let defect = (40 * 64 + 44) * 3;
    source[marginal] = 200.;
    source[defect] = 3000.;
    let gpu = Gpu::new().unwrap();
    {
        let original = &source;
        let input = gpu.upload("WB scale fixture", original).unwrap();
        let output = apply(&gpu, &input, &input, &prepared, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        assert_eq!(actual[marginal], original[marginal]);
        assert!((actual[defect] - 100.).abs() < 0.002);
        assert_eq!(
            &actual[defect + 1..defect + 3],
            &original[defect + 1..defect + 3]
        );
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<f32, u8>(original)
        );
        {
            let old = Prepared {
                params: prepared.params,
                tables: old_tables.clone(),
            };
            let output = apply(&gpu, &input, &input, &old, &mut Profile::default()).unwrap();
            let bytes = gpu.readback(&output).unwrap();
            let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
            assert!((actual[marginal] - (200. - gap)).abs() < 0.002);
        }
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn known_unlisted_red_speckles_are_corrected_and_artifacts_remain_independent() {
    use crate::{
        inspection::Content,
        render::{Engine, Settings},
        source::Source,
    };
    let root = std::env::var_os("FOVEONIC_FIXTURE_DIR").expect("fixture directory");
    let source =
        Source::open(std::path::PathBuf::from(root).join("blue-parking-sign.x3f")).unwrap();
    let gpu = Gpu::new().unwrap();
    let engine = Engine::from_device(
        gpu.device.clone(),
        gpu.queue.clone(),
        gpu.adapter_name.clone(),
    )
    .unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    let artifacts = engine
        .render_artifacts(
            &source,
            &settings,
            &["highlight.layers", "despeckle.layers"],
            &mut Profile::new(crate::profile::Mode::Off),
        )
        .unwrap();
    let images: Vec<_> = artifacts
        .iter()
        .map(|a| {
            let Content::Image(i) = &a.content else {
                panic!()
            };
            i
        })
        .collect();
    let before = gpu.readback(&images[0].buffer).unwrap();
    let before = bytemuck::cast_slice::<u8, f32>(&before);
    let after = gpu.readback(&images[1].buffer).unwrap();
    let after = bytemuck::cast_slice::<u8, f32>(&after);
    // Protect the graph's parameter handoff, not just the preparation helper:
    // the rendered stage must use the actual calibration factors on its input.
    let calibration =
        crate::nodes::sensor::calibration::prepare(&source.metadata, &settings.white_balance)
            .unwrap();
    let factors = std::array::from_fn(|c| calibration.params.gains[c]);
    let prepared = prepare(&source.metadata, factors, 1.).unwrap();
    let expected = apply(
        &gpu,
        &images[0].buffer,
        &images[0].buffer,
        &prepared,
        &mut Profile::default(),
    )
    .unwrap();
    assert!(
        gpu.readback(&expected).unwrap() == bytemuck::cast_slice::<f32, u8>(after),
        "despeckle graph input/parameter handoff differs from direct application"
    );
    for (x, y) in [(220, 705), (3624, 2264)] {
        let i = (y * source.metadata.width + x) * 3;
        assert!(
            after[i] < before[i] * 0.4,
            "({x},{y}): {} -> {}",
            before[i],
            after[i]
        );
        assert_eq!(&before[i + 1..i + 3], &after[i + 1..i + 3]);
    }
    settings.despeckle = false;
    assert!(artifacts[1].settings.despeckle);
    let again = engine
        .render_artifacts(
            &source,
            &settings,
            &["highlight.layers"],
            &mut Profile::new(crate::profile::Mode::Off),
        )
        .unwrap();
    let Content::Image(i) = &again[0].content else {
        panic!()
    };
    assert!(
        gpu.readback(&i.buffer).unwrap() == bytemuck::cast_slice::<f32, u8>(before),
        "retained pre-despeckle image changed"
    );
}

#[test]
#[ignore = "requires a GPU"]
fn compact_speckles_remove_small_clusters_and_preserve_lines_and_source() {
    use crate::render::{MerrillSpeckleControls, MerrillSpeckleMethod};
    let gpu = Gpu::new().unwrap();
    let mut samples = vec![100_f32; 64 * 64 * 3];
    for y in 10..50 {
        for c in 0..3 {
            samples[(y * 64 + 20) * 3 + c] = 300.;
        }
    }
    for y in 31..33 {
        for x in 31..33 {
            for c in 0..3 {
                samples[(y * 64 + x) * 3 + c] = 200.;
            }
        }
    }
    samples[(40 * 64 + 44) * 3] = 500.;
    let source = gpu.upload("Compact speckle fixture", &samples).unwrap();
    for size in [3, 5, 7] {
        let prepared = Prepared {
            params: parameters(64, 64, [8, 8, 56, 56]).unwrap(),
            tables: vec![5.; 3 * 4096],
        }
        .with_controls(MerrillSpeckleControls {
            method: MerrillSpeckleMethod::Compact,
            threshold: 1.,
            size,
        })
        .unwrap();
        let output = apply(&gpu, &source, &source, &prepared, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        assert_eq!(actual[(40 * 64 + 44) * 3], 100.);
        assert_eq!(
            actual[(25 * 64 + 20) * 3],
            300.,
            "straight line must survive"
        );
        assert_eq!(
            actual[(31 * 64 + 31) * 3],
            if size == 3 { 200. } else { 100. }
        );
        assert_eq!(
            gpu.readback(&source).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&samples)
        );
    }
}

#[test]
#[ignore = "requires a GPU"]
fn compact_repairs_white_speckles_when_only_one_layer_crosses_threshold() {
    use crate::render::{MerrillSpeckleControls, MerrillSpeckleMethod};
    let gpu = Gpu::new().unwrap();
    let mut samples = vec![100_f32; 64 * 64 * 3];
    let center = (24 * 64 + 24) * 3;
    samples[center..center + 3].fill(200.);
    let input = gpu
        .upload("White speckle with unequal thresholds", &samples)
        .unwrap();
    for trigger in 0..3 {
        for size in [3, 5, 7] {
            let mut lookup = vec![150.; 3 * 4096];
            lookup[trigger * 4096..(trigger + 1) * 4096].fill(5.);
            let prepared = Prepared {
                params: parameters(64, 64, [8, 8, 56, 56]).unwrap(),
                tables: lookup,
            }
            .with_controls(MerrillSpeckleControls {
                method: MerrillSpeckleMethod::Compact,
                threshold: 1.,
                size,
            })
            .unwrap();
            let output = apply(&gpu, &input, &input, &prepared, &mut Profile::default()).unwrap();
            let bytes = gpu.readback(&output).unwrap();
            let actual: &[f32] = bytemuck::cast_slice(&bytes);
            assert!(
                actual.iter().all(|&v| v == 100.),
                "trigger {trigger}, size {size}"
            );
        }
    }
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&samples)
    );
}

#[test]
#[ignore = "requires a GPU"]
fn compact_uses_shared_trimmed_donors_including_tied_extremes() {
    use crate::render::{MerrillSpeckleControls, MerrillSpeckleMethod};
    let gpu = Gpu::new().unwrap();
    let center = (24 * 64 + 24) * 3;
    let offsets = [-65_isize, -64, -63, -1, 1, 63, 64, 65];
    for trigger in 0..3 {
        for tied in [false, true] {
            let mut samples = vec![100_f32; 64 * 64 * 3];
            samples[center..center + 3].copy_from_slice(&[140., 150., 160.]);
            samples[center + trigger] = 1000.;
            let mut donors = [[0_f32; 3]; 8];
            for (n, donor) in donors.iter_mut().enumerate() {
                *donor = [
                    80. + n as f32 * 10.,
                    200. - n as f32 * 7.,
                    50. + (n % 3) as f32 * 20.,
                ];
                donor[trigger] = if tied {
                    100.
                } else {
                    [100., 80., 110., 90., 95., 105., 120., 115.][n]
                };
                let p = (center as isize + offsets[n] * 3) as usize;
                samples[p..p + 3].copy_from_slice(donor);
            }
            // Flat rings exclude the first and last donors. Unequal rings
            // exclude indices 1 and 6, regardless of the other layers' values.
            let excluded = if tied { [0, 7] } else { [1, 6] };
            let mut expected = [0_f32; 3];
            for (n, donor) in donors.iter().enumerate() {
                if !excluded.contains(&n) {
                    for c in 0..3 {
                        expected[c] += donor[c] / 6.;
                    }
                }
            }
            let input = gpu
                .upload("Shared speckle donor fixture", &samples)
                .unwrap();
            let mut lookup = vec![10_000.; 3 * 4096];
            lookup[trigger * 4096..(trigger + 1) * 4096].fill(5.);
            let prepared = Prepared {
                params: parameters(64, 64, [8, 8, 56, 56]).unwrap(),
                tables: lookup,
            }
            .with_controls(MerrillSpeckleControls {
                method: MerrillSpeckleMethod::Compact,
                threshold: 1.,
                size: 7,
            })
            .unwrap();
            let output = apply(&gpu, &input, &input, &prepared, &mut Profile::default()).unwrap();
            let bytes = gpu.readback(&output).unwrap();
            let actual: &[f32] = bytemuck::cast_slice(&bytes);
            for c in 0..3 {
                assert!(
                    (actual[center + c] - expected[c]).abs() < 0.0001,
                    "trigger {trigger}, tied {tied}, layer {c}: {} != {}",
                    actual[center + c],
                    expected[c]
                );
            }
            assert_eq!(
                gpu.readback(&input).unwrap(),
                bytemuck::cast_slice::<f32, u8>(&samples)
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn compact_uses_smallest_detected_radius_for_every_layer() {
    use crate::render::{MerrillSpeckleControls, MerrillSpeckleMethod};
    let gpu = Gpu::new().unwrap();
    let mut samples = vec![100_f32; 64 * 64 * 3];
    let center = (24 * 64 + 24) * 3;
    samples[center..center + 3].fill(200.);
    for offset in [-65_isize, -64, -63, -1, 1, 63, 64, 65] {
        samples[(center as isize + offset * 3) as usize] = 200.;
    }
    let input = gpu
        .upload("Speckle layers with different support", &samples)
        .unwrap();
    for size in [5, 7] {
        let prepared = Prepared {
            params: parameters(64, 64, [8, 8, 56, 56]).unwrap(),
            tables: vec![5.; 3 * 4096],
        }
        .with_controls(MerrillSpeckleControls {
            method: MerrillSpeckleMethod::Compact,
            threshold: 1.,
            size,
        })
        .unwrap();
        let output = apply(&gpu, &input, &input, &prepared, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        assert_eq!(&actual[center..center + 3], &[200., 100., 100.]);
    }
}

#[test]
#[ignore = "requires a GPU"]
fn compact_uses_original_highlight_codes_after_calibration() {
    use crate::render::{MerrillSpeckleControls, MerrillSpeckleMethod};
    let gpu = Gpu::new().unwrap();
    let mut original = vec![20_f32; 64 * 64 * 3];
    let mut calibrated = vec![8_f32; 64 * 64 * 3];
    let defect = (24 * 64 + 24) * 3;
    let near_clipped = (32 * 64 + 32) * 3 + 1;
    let clipped_low_gain = (40 * 64 + 40) * 3 + 2;
    original[defect] = 1158.;
    calibrated[defect] = 6923.;
    original[near_clipped] = 3900.;
    calibrated[near_clipped] = 7000.;
    original[near_clipped - 1] = 600.;
    original[clipped_low_gain] = 4095.;
    calibrated[clipped_low_gain] = 3000.;
    original[clipped_low_gain + 3] = 600.;
    let raw = gpu
        .upload("Original highlight evidence", &original)
        .unwrap();
    let input = gpu.upload("Calibrated bright points", &calibrated).unwrap();
    for size in [3, 5, 7] {
        let prepared = Prepared {
            params: parameters(64, 64, [8, 8, 56, 56]).unwrap(),
            tables: vec![5.; 3 * 4096],
        }
        .with_controls(MerrillSpeckleControls {
            method: MerrillSpeckleMethod::Compact,
            threshold: 1.,
            size,
        })
        .unwrap();
        let out = apply(&gpu, &input, &raw, &prepared, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&out).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        assert_eq!(
            actual[defect], 8.,
            "unclipped amplified defect must be repaired"
        );
        assert_eq!(actual[near_clipped], calibrated[near_clipped]);
        assert_eq!(actual[clipped_low_gain], calibrated[clipped_low_gain]);
        for (i, &value) in actual.iter().enumerate() {
            if i != defect {
                assert_eq!(value, calibrated[i], "unexpected change at sample {i}");
            }
        }
    }
    assert_eq!(
        gpu.readback(&raw).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&original)
    );
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&calibrated)
    );
}

#[test]
#[ignore = "requires a GPU"]
fn compact_repairs_isolated_clipped_layers_but_preserves_supported_highlights() {
    use crate::render::{MerrillSpeckleControls, MerrillSpeckleMethod};
    let gpu = Gpu::new().unwrap();
    let mut original = vec![79_f32; 64 * 64 * 3];
    let mut calibrated = vec![69_f32; 64 * 64 * 3];
    let defects = [
        (16 * 64 + 16) * 3,
        (16 * 64 + 32) * 3 + 1,
        (16 * 64 + 48) * 3 + 2,
    ];
    for (i, &p) in defects.iter().enumerate() {
        original[p] = [3900., 4045., 4095.][i];
        calibrated[p] = [7000., 6362., 3000.][i];
    }
    let supported = [(32 * 64 + 16) * 3, (32 * 64 + 32) * 3, (32 * 64 + 48) * 3];
    for &p in &supported {
        original[p] = 4045.;
        calibrated[p] = 6362.;
        // Both unprotected layers would independently qualify as outliers.
        // Highlight evidence in the first layer must preserve the triplet.
        calibrated[p + 1] = 300.;
        calibrated[p + 2] = 400.;
    }
    // Cross-layer support, a colored neighbor, and a diagonal in another layer
    // each keep an ambiguous near-clipped sample protected.
    original[supported[0] + 1] = 500.;
    original[supported[1] + 3] = 500.;
    original[supported[2] + (64 + 1) * 3 + 2] = 500.;
    let raw = gpu
        .upload("Original clipped outliers and highlights", &original)
        .unwrap();
    let input = gpu
        .upload("Calibrated clipped outliers and highlights", &calibrated)
        .unwrap();
    for size in [3, 5, 7] {
        let prepared = Prepared {
            params: parameters(64, 64, [8, 8, 56, 56]).unwrap(),
            tables: vec![5.; 3 * 4096],
        }
        .with_controls(MerrillSpeckleControls {
            method: MerrillSpeckleMethod::Compact,
            threshold: 0.8,
            size,
        })
        .unwrap();
        let out = apply(&gpu, &input, &raw, &prepared, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&out).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        for (p, &value) in actual.iter().enumerate() {
            assert_eq!(
                value,
                if defects.contains(&p) {
                    69.
                } else {
                    calibrated[p]
                },
                "sample {p}, size {size}"
            );
        }
    }
    assert_eq!(
        gpu.readback(&raw).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&original)
    );
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&calibrated)
    );
}

fn serial_replay(
    gpu: &Gpu,
    input: &crate::gpu::ReadGuard,
    prepared: &Prepared,
) -> crate::gpu::ReadGuard {
    // Keep the original four-pass execution here as the scheduling oracle.
    // Production always uses the fixed parallel algorithm.
    let p = prepared.params;
    let count = u64::from(p.source[0]) * u64::from(p.source[1]);
    let mut output = gpu.storage("Serial despeckle check", count * 12).unwrap();
    let mut marks = gpu.storage("Serial block ranks", count * 4).unwrap();
    let mut counts = gpu
        .storage("Serial bank prefixes", u64::from(p.source[2] + BANKS) * 4)
        .unwrap();
    let mut records = gpu
        .storage(
            "Serial records",
            u64::from(p.source[3]) * u64::from(BANKS) * 8,
        )
        .unwrap();
    let uniform = gpu.uniform(&p);
    let lookup = gpu.upload("Serial thresholds", &prepared.tables).unwrap();
    let mut trace = gpu.trace(false, 4);
    gpu.dispatch(
        Kernel::SpeckleDetect,
        &mut [
            (0, Read(input)),
            (1, Read(&lookup)),
            (2, Read(&uniform)),
            (3, Write(output.write())),
            (4, Write(marks.write())),
            (5, Write(counts.write())),
        ],
        [p.source[2], 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::SpecklePrefix,
        &mut [(2, Read(&uniform)), (5, Write(counts.write()))],
        [BANKS, 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::SpeckleScatter,
        &mut [
            (2, Read(&uniform)),
            (4, Write(marks.write())),
            (5, Read(&counts)),
            (6, Write(records.write())),
        ],
        [p.source[2], 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::SpeckleRepair,
        &mut [
            (2, Read(&uniform)),
            (3, Write(output.write())),
            (5, Read(&counts)),
            (6, Read(&records)),
        ],
        [1, 1],
        &mut trace,
    );
    output
}

#[test]
#[ignore = "requires a GPU"]
fn parallel_speckle_rounds_match_serial_chains_capacity_and_empty_work() {
    let gpu = Gpu::new().unwrap();
    let width = 64;
    let height = 64;
    let mut horizontal = vec![100_f32; width * height * 3];
    let mut vertical = horizontal.clone();
    let mut diagonal = horizontal.clone();
    // Vary donors so an earlier repair survives min/max trimming and changes
    // the next repair. Flat donors could hide an incorrectly reordered chain.
    for i in 0..width {
        horizontal[(30 * width + i) * 3 + 2] = 80.;
        horizontal[(32 * width + i) * 3 + 2] = 120.;
        vertical[(i * width + 30) * 3] = 80.;
        vertical[(i * width + 32) * 3] = 120.;
        for x in 0..width {
            diagonal[(i * width + x) * 3 + 1] += i as f32 * 0.25;
        }
    }
    for i in 4..60 {
        let v = if i % 2 == 0 { 300. } else { -100. };
        horizontal[(31 * width + i) * 3 + 2] = v;
        vertical[(i * width + 31) * 3] = v;
        diagonal[(i * width + (63 - i)) * 3 + 1] = v;
    }
    let mut seed = 0x843f_2201_u32;
    let random: Vec<f32> = (0..width * height * 3)
        .map(|_| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 16) as f32 / 128. - 128.
        })
        .collect();
    let fixture = floats(&fixture()["input"]);
    for (name, pixels) in [
        ("empty", vec![100_f32; width * height * 3]),
        ("horizontal chain", horizontal),
        ("vertical chain across banks", vertical),
        ("upper-right dependencies", diagonal),
        ("dense signed layers", random),
        ("seven layer masks", fixture),
    ] {
        let input = gpu.upload(name, &pixels).unwrap();
        for capacity in [1, 7, (width * height / BANKS as usize) as u32] {
            let mut p = Prepared {
                params: parameters(width as u32, height as u32, [8, 8, 56, 56]).unwrap(),
                tables: vec![0.; 3 * 4096],
            };
            p.params.source[3] = capacity;
            let before = gpu.activity();
            let expected = serial_replay(&gpu, &input, &p);
            let serial_activity = gpu.activity().since(before);
            let before = gpu.activity();
            let actual = apply(
                &gpu,
                &input,
                &input,
                &p,
                &mut Profile::new(crate::profile::Mode::Off),
            )
            .unwrap();
            let parallel_activity = gpu.activity().since(before);
            assert_eq!(parallel_activity.allocations, serial_activity.allocations);
            assert_eq!(
                parallel_activity.storage_bytes,
                serial_activity.storage_bytes
            );
            assert_eq!(parallel_activity.upload_bytes, serial_activity.upload_bytes);
            assert_eq!(parallel_activity.readbacks, 0);
            assert_eq!(
                gpu.readback(&actual).unwrap(),
                gpu.readback(&expected).unwrap(),
                "{name}, bank capacity {capacity}"
            );
        }
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&pixels)
        );
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR; prints completed despeckle timings"]
fn parallel_speckle_originals_match_serial_and_measure_completed_work() {
    use crate::{inspection::Content, render::Settings, source::Source};
    use std::time::Instant;
    let root = std::path::PathBuf::from(
        std::env::var_os("FOVEONIC_FIXTURE_DIR").expect("fixture directory"),
    );
    let engine = crate::render::Engine::new().unwrap();
    let gpu = engine.test_gpu();
    let settings = Settings::new("Sunlight", "Standard");
    for name in [
        "blue-parking-sign",
        "glossy-leaves-and-buds",
        "rocks-and-tree-shadows",
        "sunlit-red-flower",
        "turquoise-still-life",
        "yellow-flowers-and-stroller",
    ] {
        let source = Source::open(root.join(format!("{name}.x3f"))).unwrap();
        let mut artifacts = engine
            .render_artifacts(
                &source,
                &settings,
                &["highlight.layers"],
                &mut Profile::new(crate::profile::Mode::Off),
            )
            .unwrap();
        let Content::Image(input) = artifacts.remove(0).content else {
            panic!("expected native layers");
        };
        let calibration = super::super::calibration::prepare(&source.metadata, "Sunlight").unwrap();
        let gains = std::array::from_fn(|c| calibration.params.gains[c]);
        let couplings = crate::noise::couplings(
            &source.metadata,
            "Sunlight",
            settings.sensor_noise_controls(),
        )
        .unwrap();
        for threshold in [1., 0.2] {
            let prepared = prepare(&source.metadata, gains, couplings.despeckle_scale)
                .unwrap()
                .with_controls(crate::render::MerrillSpeckleControls {
                    threshold,
                    ..Default::default()
                })
                .unwrap();
            gpu.device.poll(wgpu::Maintain::Wait);
            let start = Instant::now();
            let expected = serial_replay(gpu, &input.buffer, &prepared);
            gpu.device.poll(wgpu::Maintain::Wait);
            let serial_ms = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            let actual = apply(
                gpu,
                &input.buffer,
                &input.buffer,
                &prepared,
                &mut Profile::new(crate::profile::Mode::Off),
            )
            .unwrap();
            gpu.device.poll(wgpu::Maintain::Wait);
            let parallel_ms = start.elapsed().as_secs_f64() * 1000.;
            assert_eq!(
                gpu.readback(&actual).unwrap(),
                gpu.readback(&expected).unwrap(),
                "{name}, threshold {threshold}"
            );
            println!(
                "speckle_completed {}",
                serde_json::json!({"source":name,"threshold":threshold,"serial_ms":serial_ms,"parallel_ms":parallel_ms})
            );
        }
    }
}
