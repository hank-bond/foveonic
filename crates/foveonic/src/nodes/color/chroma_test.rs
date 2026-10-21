//! Checks signed chroma kernels against composed native row callbacks.
use super::*;
#[test]
fn rejects_invalid_correction_contracts() {
    assert!(Prepared::new([17, 11], [0., 1., 1.], [0.8; 3], 4, 3, Support::Binomial3).is_err());
    assert!(Prepared::new([17, 11], [1.; 3], [0.8; 3], 4, 16, Support::Box5).is_err());
}
#[test]
#[ignore = "requires a GPU"]
fn gpu_signed_chroma_matches_native_composed_rows() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/data/chroma-reference.json")).unwrap();
    let input: Vec<f32> = fixture["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap() as f32)
        .collect();
    let gpu = Gpu::new().unwrap();
    let source = gpu.upload("Signed chroma native fixture", &input).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let triplet = |name: &str| std::array::from_fn(|i| case[name][i].as_f64().unwrap());
        let p = Prepared::new(
            [17, 11],
            triplet("amplitude"),
            triplet("slope"),
            case["setting"].as_u64().unwrap() as u32,
            case["shift"].as_u64().unwrap() as u32,
            if case["support"] == "binomial3" {
                Support::Binomial3
            } else {
                Support::Box5
            },
        )
        .unwrap();
        let output = apply(&gpu, &source, &p, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        for (i, v) in actual.iter().enumerate() {
            assert_eq!(
                *v,
                case["expected"][i].as_i64().unwrap() as f32,
                "{} pixel {i}",
                case["support"]
            );
        }
    }
    assert_eq!(
        gpu.readback(&source).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&input)
    );
}
