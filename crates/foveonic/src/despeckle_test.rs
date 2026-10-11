//! Checks reference lookup rounding, deterministic detection/repair, gain invariance, and the two photographed residual defects.
use super::*;
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../tests/data/despeckle-reference.json")).unwrap()
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
    assert!(parameters(64, 64, [4, 8, 56, 56], 1.).is_err());
    assert!(parameters(64, 64, [8, 8, 56, 56], -1.).is_err());
}
fn metadata() -> Metadata {
    let mut metadata = Metadata {
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
fn preparation_uses_actual_calibration_factors_and_keeps_adjustment_independent() {
    // No WB lists are needed: threshold preparation consumes the exact factors
    // already selected/rounded for calibration, not a second metadata recipe.
    let mut metadata = metadata();
    let gains = [4.6_f32, 2.2, 1.6];
    let expected = tables(gains.map(f64::from), [15., 15., 5.]).unwrap();
    assert_ne!(
        expected,
        tables(gains.map(|v| f64::from(v) * 0.5), [15., 15., 5.]).unwrap()
    );
    for gain in [0., 1., 1.3, 2.] {
        let prepared = prepare(&metadata, gains, gain).unwrap();
        assert_eq!(prepared.tables, expected);
        assert_eq!(prepared.params.gain[0], gain);
    }
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
    for invalid in [0., -1., f32::NAN, f32::INFINITY] {
        assert!(prepare(&metadata, [invalid, gains[1], gains[2]], 1.).is_err());
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
            let calibration = crate::calibration::prepare(metadata, wb).unwrap();
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
    assert!(crate::inspection::output_names(&s).contains(&"despeckle.layers"));
    s.despeckle = false;
    assert!(!crate::inspection::output_names(&s).contains(&"despeckle.layers"));
}
#[test]
#[ignore = "requires a GPU"]
fn gpu_despeckle_matches_ordered_reference_and_preserves_gain_and_input() {
    let f = fixture();
    let source = floats(&f["input"]);
    let expected = floats(&f["output"]);
    let gpu = Gpu::new().unwrap();
    for gain in [0., 1., 1.3] {
        let original: Vec<f32> = source.iter().map(|v| v * gain).collect();
        let input = gpu.upload("Speckle fixture", &original).unwrap();
        let p = Prepared {
            params: parameters(64, 64, [8, 8, 56, 56], gain).unwrap(),
            tables: vec![0.; 3 * 4096],
        };
        let out = apply(&gpu, &input, &p, &mut Profile::new(true)).unwrap();
        let actual: Vec<f32> =
            bytemuck::cast_slice::<u8, f32>(&gpu.readback(&out).unwrap()).to_vec();
        for (i, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
            assert!(
                (a - b * gain).abs() < 0.002,
                "gain {gain}, sample {i}: {a} != {}",
                b * gain
            );
        }
        assert_eq!(
            bytemuck::cast_slice::<u8, f32>(&gpu.readback(&input).unwrap()),
            original
        );
    }
    // A capacity-one bank must retain its first row-ordered record, not the
    // first thread that wins an atomic append. Bank containing y=16 has seven.
    let input = gpu.upload("Capacity fixture", &source).unwrap();
    let mut p = Prepared {
        params: parameters(64, 64, [8, 8, 56, 56], 1.).unwrap(),
        tables: vec![0.; 3 * 4096],
    };
    p.params.source[3] = 1;
    let out = apply(&gpu, &input, &p, &mut Profile::new(false)).unwrap();
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
fn gpu_scale_matched_thresholds_keep_marginal_detail_and_sensor_gain_invariance() {
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
    for gain in [0., 1., 1.3] {
        let original: Vec<_> = source.iter().map(|v| v * gain).collect();
        let input = gpu.upload("WB scale fixture", &original).unwrap();
        let prepared = prepare(&metadata, factors, gain).unwrap();
        let output = apply(&gpu, &input, &prepared, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        assert_eq!(actual[marginal], original[marginal]);
        assert!((actual[defect] - 100. * gain).abs() < 0.002);
        assert_eq!(
            &actual[defect + 1..defect + 3],
            &original[defect + 1..defect + 3]
        );
        if gain == 0. {
            assert!(actual.iter().all(|v| *v == 0.));
        }
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&original)
        );
        if gain == 1. {
            let old = Prepared {
                params: prepared.params,
                tables: old_tables.clone(),
            };
            let output = apply(&gpu, &input, &old, &mut Profile::default()).unwrap();
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
            &["repair.layers", "despeckle.layers"],
            &mut Profile::new(false),
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
        crate::calibration::prepare(&source.metadata, &settings.white_balance).unwrap();
    let factors = std::array::from_fn(|c| calibration.params.gains[c]);
    let prepared = prepare(&source.metadata, factors, settings.sensor_gain).unwrap();
    let expected = apply(&gpu, &images[0].buffer, &prepared, &mut Profile::default()).unwrap();
    assert_eq!(
        gpu.readback(&expected).unwrap(),
        bytemuck::cast_slice::<f32, u8>(after)
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
            &["repair.layers"],
            &mut Profile::new(false),
        )
        .unwrap();
    let Content::Image(i) = &again[0].content else {
        panic!()
    };
    assert_eq!(
        gpu.readback(&i.buffer).unwrap(),
        bytemuck::cast_slice::<f32, u8>(before)
    );
}
