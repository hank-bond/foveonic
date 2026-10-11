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
