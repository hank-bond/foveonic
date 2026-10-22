//! Checks complete blue-pattern composition against independently generated native fixtures.
use super::*;
#[test]
fn rejects_partial_quarter_cells() {
    assert!(Prepared::new([65, 64], 1., 100.).is_err());
    assert!(Prepared::new([64, 64], 1., 100.).is_ok());
}
#[test]
#[ignore = "requires a GPU and BLUE_PATTERN_FIXTURES"]
fn complete_native_blue_pattern_cases() {
    let directory = std::path::PathBuf::from(std::env::var("BLUE_PATTERN_FIXTURES").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let extent = std::array::from_fn(|i| case["extent"][i].as_u64().unwrap() as u32);
        let p = Prepared::new(
            extent,
            case["strength"].as_f64().unwrap() as f32,
            case["iso"].as_f64().unwrap() as f32,
        )
        .unwrap();
        let bytes = std::fs::read(directory.join(format!("{name}-input.bin"))).unwrap();
        let expected = std::fs::read(directory.join(format!("{name}-output.bin"))).unwrap();
        let input = gpu
            .upload(
                "Native blue-pattern fixture",
                bytemuck::cast_slice::<u8, f32>(&bytes),
            )
            .unwrap();
        let output = apply(&gpu, &input, &p, &mut Profile::default()).unwrap();
        let actual = gpu.readback(&output).unwrap();
        let mut errors = Vec::new();
        let mut count = 0;
        let mut maximum = 0f32;
        for (i, (a, b)) in actual
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.as_chunks::<4>().0)
            .enumerate()
        {
            if a != b {
                count += 1;
                let av = f32::from_le_bytes(*a);
                let bv = f32::from_le_bytes(*b);
                maximum = maximum.max((av - bv).abs());
                if errors.len() < 6 {
                    errors.push((i, av, bv));
                }
            }
        }
        assert_eq!(count, 0, "{name}: maximum {maximum}, {errors:?}");
    }
}
