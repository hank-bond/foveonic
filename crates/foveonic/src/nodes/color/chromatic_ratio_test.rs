//! Compares ratio noise suppression against bounded native camera-code fixtures.

use super::*;

#[test]
#[ignore = "requires a GPU and CHROMATIC_RATIO_REFERENCE native fixture directory"]
fn matches_native_ratio_worker() {
    let root = std::path::PathBuf::from(std::env::var_os("CHROMATIC_RATIO_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let floats = |name: &str| {
            let bytes = std::fs::read(root.join(case[name].as_str().unwrap())).unwrap();
            bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
        };
        let extent = std::array::from_fn(|i| case["extent"][i].as_u64().unwrap() as u32);
        let gains = std::array::from_fn(|i| case["gains"][i].as_f64().unwrap());
        let model = std::array::from_fn(|i| case["model"][i].as_f64().unwrap());
        let prepared = Prepared::new(extent, gains, model).unwrap();
        let input = gpu.upload("Native CSR input", &floats("input")).unwrap();
        let output = apply(&gpu, &input, &prepared, &mut Default::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        let expected = floats("output");
        let (mut maximum, mut total) = (0_f32, 0_f64);
        for (&a, &b) in actual.iter().zip(&expected) {
            assert!(a.is_finite());
            maximum = maximum.max((a - b).abs());
            total += f64::from((a - b).abs());
        }
        eprintln!(
            "CSR {extent:?}: max {maximum} mean {}",
            total / actual.len() as f64
        );
        assert!(maximum < 0.005);
    }
}

#[test]
#[ignore = "requires a GPU and CHROMATIC_RATIO_BOUNDARY owned original Stage1 CSR boundary"]
fn original_stage1_ratio_worker_matches_native() {
    let root = std::path::PathBuf::from(std::env::var_os("CHROMATIC_RATIO_BOUNDARY").unwrap());
    let controls: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("csr-controls.json")).unwrap()).unwrap();
    let images: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("images.json")).unwrap()).unwrap();
    let extent =
        std::array::from_fn(|i| images["csr"]["source"]["extent"][i].as_u64().unwrap() as u32);
    let gains = std::array::from_fn(|i| controls["gains"][i].as_f64().unwrap());
    let model = std::array::from_fn(|i| controls["model"][i].as_f64().unwrap());
    let prepared = Prepared::new(extent, gains, model).unwrap();
    let input_bytes = std::fs::read(root.join("csr-input.f32")).unwrap();
    let expected_bytes = std::fs::read(root.join("csr-output.f32")).unwrap();
    let gpu = Gpu::new().unwrap();
    let input = gpu
        .upload(
            "Native original CSR source",
            bytemuck::cast_slice::<u8, f32>(&input_bytes),
        )
        .unwrap();
    let output = apply(&gpu, &input, &prepared, &mut Default::default()).unwrap();
    let bytes = gpu.readback(&output).unwrap();
    let actual: &[f32] = bytemuck::cast_slice(&bytes);
    let expected: &[f32] = bytemuck::cast_slice(&expected_bytes);
    let (mut maximum, mut sum, mut changed) = (0_f32, 0_f64, 0_usize);
    let mut worst = 0;
    for (i, (&a, &b)) in actual.iter().zip(expected).enumerate() {
        assert!(a.is_finite());
        let difference = (a - b).abs();
        if difference > maximum {
            maximum = difference;
            worst = i;
        }
        sum += f64::from(difference);
        changed += usize::from(a.to_bits() != b.to_bits());
    }
    let mean = sum / actual.len() as f64;
    eprintln!(
        "Original CSR {extent:?}: max {maximum}, mean {mean}, changed {changed}/{}, worst ({},{},{}) actual{} native{}",
        actual.len(),
        worst / 3 % extent[0] as usize,
        worst / 3 / extent[0] as usize,
        worst % 3,
        actual[worst],
        expected[worst]
    );
    assert!(mean < 0.00005);
    assert!(maximum <= 0.02);
}
