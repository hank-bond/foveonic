//! Checks finite contracts and bounded native ordered-neighborhood fixtures.
use super::*;

#[test]
fn parameters_require_complete_finite_tables() {
    let model = [1., 1., 1., 0., 0., 0., 0., 0., 0.];
    assert!(Prepared::new([8, 6], model, vec![1.; 14385], vec![1.; 2048], 1.).is_ok());
    assert!(Prepared::new([0, 6], model, vec![1.; 14385], vec![1.; 2048], 1.).is_err());
    assert!(Prepared::new([8, 6], model, vec![1.; 14384], vec![1.; 2048], 1.).is_err());
    assert!(Prepared::new([8, 6], model, vec![1.; 14385], vec![1.; 2047], 1.).is_err());
    assert!(Prepared::new([8, 6], model, vec![1.; 14385], vec![1.; 2048], 0.).is_err());
}

#[test]
#[ignore = "requires a GPU and NEIGHBORHOOD_NOISE_REFERENCE native callback fixtures"]
fn ordered_updates_match_bounded_native_workers() {
    let root = std::path::PathBuf::from(std::env::var_os("NEIGHBORHOOD_NOISE_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let read = |key: &str| std::fs::read(root.join(case[key].as_str().unwrap())).unwrap();
        let input = read("source");
        let expected = read("output");
        let extent = [
            case["extent"][0].as_u64().unwrap() as u32,
            case["extent"][1].as_u64().unwrap() as u32,
        ];
        let model: [f64; 9] = manifest["model"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let intensity = std::fs::read(root.join("intensity.f32")).unwrap();
        let range = std::fs::read(root.join("range.f32")).unwrap();
        let prepared = Prepared::new(
            extent,
            model,
            bytemuck::cast_slice::<u8, f32>(&intensity).to_vec(),
            bytemuck::cast_slice::<u8, f32>(&range).to_vec(),
            case["strength"].as_f64().unwrap() as f32,
        )
        .unwrap();
        let source = gpu
            .upload(
                "Native neighborhood input",
                bytemuck::cast_slice::<u8, f32>(&input),
            )
            .unwrap();
        let result = apply(&gpu, &source, &prepared, &mut Default::default()).unwrap();
        let actual = gpu.readback(&result).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&actual);
        let expected: &[f32] = bytemuck::cast_slice(&expected);
        let maximum = actual
            .iter()
            .zip(expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        assert!(maximum <= 0.004, "{} maximum={maximum}", case["name"]);
        assert_eq!(gpu.readback(&source).unwrap(), input);
    }
}
