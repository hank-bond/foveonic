//! Verifies native coefficient metadata, retained untrimmed starts, row spill, and immutable GPU resampling.
use super::*;
use serde_json::Value;
fn fixture() -> Value {
    serde_json::from_str(include_str!("../tests/data/resample-reference.json")).unwrap()
}
fn floats(v: &Value) -> Vec<f32> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect()
}
fn prepared(v: &Value) -> Prepared {
    let dims = |key: &str| std::array::from_fn(|i| v[key][i].as_u64().unwrap() as u32);
    Prepared::new(
        dims("source"),
        dims("target"),
        2,
        v["scale"].as_f64().unwrap() as f32,
    )
    .unwrap()
}
#[test]
fn native_polyphase_metadata_matches_all_reduced_and_restore_scales() {
    let f = fixture();
    for row in f["results"].as_array().unwrap() {
        for key in ["down", "up"] {
            let v = &row[key];
            let p = prepared(v);
            assert_eq!(p.params.output[2], v["taps"].as_u64().unwrap() as u32);
            assert_eq!(p.params.output[3], v["phases"].as_u64().unwrap() as u32);
            assert_eq!(p.starts[0], v["start"].as_i64().unwrap() as i32);
            assert_eq!(p.weights, floats(&v["weights"]));
            let steps: Vec<i32> = v["steps"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as i32)
                .collect();
            assert_eq!(p.params.advance[0], steps.iter().sum::<i32>());
            for i in 1..p.starts.len() {
                assert_eq!(p.starts[i] - p.starts[i - 1], steps[i - 1]);
            }
        }
    }
    assert!(Prepared::new([96, 80], [36, 30], 1, 0.25).is_err());
    assert!(Prepared::new([96, 80], [35, 30], 1, 0.375).is_err());
    assert!(Prepared::new([1, 1], [32, 24], 1, 0.5).is_err());
}
#[test]
#[ignore = "requires a GPU"]
fn gpu_polyphase_matches_native_down_restore_scalar_rows_and_tile_resets() {
    let gpu = Gpu::new().unwrap();
    let f = fixture();
    let source = floats(&f["input"]);
    for row in f["results"].as_array().unwrap() {
        for key in ["down", "up"] {
            let v = &row[key];
            let p = prepared(v);
            let values = if key == "down" {
                source.clone()
            } else {
                floats(&row["down"]["pixels"])
            };
            let two: Vec<f32> = values.iter().chain(&values).copied().collect();
            let input = gpu.upload("Native resample fixture", &two).unwrap();
            let out = apply(&gpu, &input, &p, &mut Profile::default()).unwrap();
            let bytes = gpu.readback(&out).unwrap();
            let expected = floats(&v["pixels"]);
            let samples = bytemuck::cast_slice::<u8, f32>(&bytes);
            let max = samples
                .iter()
                .zip(expected.iter().chain(&expected))
                .map(|(a, b)| (a - b).abs())
                .fold(0_f32, f32::max);
            assert!(max < 0.005, "{key} scale {} max {max}", p.params.source[0]);
            assert!(
                gpu.readback(&input).unwrap() == bytemuck::cast_slice::<f32, u8>(&two),
                "resampling changed input"
            );
            let scalar: Vec<f32> = two.as_chunks::<3>().0.iter().map(|v| v[0]).collect();
            let input = gpu.upload("Native scalar fixture", &scalar).unwrap();
            let out = apply_scalar(&gpu, &input, &p, &mut Profile::default()).unwrap();
            let bytes = gpu.readback(&out).unwrap();
            let expected: Vec<f32> = expected.as_chunks::<3>().0.iter().map(|v| v[0]).collect();
            let max = bytemuck::cast_slice::<u8, f32>(&bytes)
                .iter()
                .zip(expected.iter().chain(&expected))
                .map(|(a, b)| (a - b).abs())
                .fold(0_f32, f32::max);
            assert!(max < 0.005, "scalar {key} max {max}");
        }
    }
}
