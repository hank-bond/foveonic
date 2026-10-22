//! Checks packed affine calibration and untouched sensor input ownership.
use super::*;
#[test]
fn rejects_incomplete_coefficients() {
    assert!(Prepared::new([16, 2], 2048, vec![1.; 95]).is_err());
}
#[test]
#[ignore = "requires a GPU"]
fn affine_column_correction_preserves_native_fused_rounding() {
    let gpu = Gpu::new().unwrap();
    let extent = [16, 3];
    let gains = (0..96)
        .map(|i| (i as f32 * 0.001 - 0.05) / (1.04 + i as f32 * 0.0001))
        .collect::<Vec<_>>();
    let p = Prepared::new(extent, 2048, gains.clone()).unwrap();
    let values = (0..288)
        .map(|i| i as f32 * 123.25 - 45.)
        .collect::<Vec<_>>();
    let expected = values
        .iter()
        .enumerate()
        .map(|(i, &v)| gains[i % 96].mul_add(v - 2048., 2048.))
        .collect::<Vec<_>>();
    let input = gpu.upload("Packed gain fixture", &values).unwrap();
    let output = apply(&gpu, &input, &p, &mut Profile::default()).unwrap();
    assert_eq!(
        gpu.readback(&output).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&expected)
    );
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&values)
    );
}
#[test]
#[ignore = "requires a GPU and COLUMN_GAIN_FIXTURES"]
fn composed_extraction_matches_native_column_wrapper() {
    let root = std::path::PathBuf::from(std::env::var_os("COLUMN_GAIN_FIXTURES").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("cases.json")).unwrap()).unwrap();
    let read = |name: &str| {
        let bytes = std::fs::read(root.join(name)).unwrap();
        bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
    };
    let gains = read("gains.bin");
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let offset = case["offset"].as_u64().unwrap() as u32;
        let scale = case["scale"].as_f64().unwrap() as f32;
        let p = Prepared::new([16, 3], offset, gains.clone()).unwrap();
        let values = read(&format!("{name}-input.bin"));
        let input = gpu
            .upload("Native column wrapper fixture", &values)
            .unwrap();
        let mut profile = Profile::default();
        let adjusted = apply(&gpu, &input, &p, &mut profile).unwrap();
        let affine = crate::nodes::sensor::phase_extraction::Params::calibrated(
            16,
            3,
            offset,
            scale,
            14300. * scale,
        )
        .unwrap();
        let (rgb, blue) =
            crate::nodes::sensor::phase_extraction::apply(&gpu, &adjusted, &affine, &mut profile)
                .unwrap();
        for (kind, actual) in [("rgb", rgb), ("blue", blue)] {
            let expected = std::fs::read(root.join(format!("{name}-{kind}.bin"))).unwrap();
            let bytes = gpu.readback(&actual).unwrap();
            assert_eq!(bytes, expected, "{name}/{kind}");
        }
    }
}
