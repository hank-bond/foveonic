//! Checks continuous scalar filtering against bounded reference implementation workers.

use super::*;

#[test]
#[ignore = "requires GPU and LOW_BILATERAL_REFERENCE with scalar worker fixtures"]
fn continuous_variance_filter_matches_reference_workers() {
    let root = std::path::PathBuf::from(std::env::var_os("LOW_BILATERAL_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let noise = std::array::from_fn(|i| manifest["noise"][i].as_f64().unwrap());
    let bytes = std::fs::read(root.join("intensity.f32")).unwrap();
    let intensity = bytemuck::cast_slice::<u8, f32>(&bytes).to_vec();
    let gpu = Gpu::new().unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let extent = std::array::from_fn(|i| case["extent"][i].as_u64().unwrap() as u32);
        let values = std::fs::read(root.join(format!("{name}-input.f32"))).unwrap();
        let expected = std::fs::read(root.join(format!("{name}-expected.f32"))).unwrap();
        let input = gpu
            .upload(
                "Scalar reference input",
                bytemuck::cast_slice::<u8, f32>(&values),
            )
            .unwrap();
        let prepared = Prepared::new(
            extent,
            5,
            noise,
            case["strength"].as_f64().unwrap() as f32,
            0.25,
            intensity.clone(),
            vec![1.; 16384],
            13000.,
        )
        .unwrap();
        let output = apply(&gpu, &input, &prepared, &mut Default::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        let reference: &[f32] = bytemuck::cast_slice(&expected);
        let maximum = actual
            .iter()
            .zip(reference)
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        eprintln!("Scalar {name} maximum {maximum}");
        assert!(maximum <= 0.001, "{name}: {maximum}");
        assert_eq!(gpu.readback(&input).unwrap(), values);
    }
}
