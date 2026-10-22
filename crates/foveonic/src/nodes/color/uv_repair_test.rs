//! Checks bounded numerical inputs and complete guarded native callback fixtures.

use super::*;

#[test]
fn inputs_require_finite_models_and_complete_conditioning_tables() {
    let model = [1., 1., 1., 0., 0., 0., 0., 0., 0.];
    assert!(Prepared::new([3, 5], model, vec![1.; 14385], 40.).is_ok());
    assert!(Prepared::new([0, 5], model, vec![1.; 14385], 40.).is_err());
    assert!(Prepared::new([3, 5], model, vec![1.; 14384], 40.).is_err());
    assert!(Prepared::new([3, 5], model, vec![f32::NAN; 14385], 40.).is_err());
    let mut negative = model;
    negative[0] = -1.;
    assert!(Prepared::new([3, 5], negative, vec![1.; 14385], 40.).is_err());
    let a = Prepared::new([3, 5], model, vec![1.; 14385], 40.).unwrap();
    let b = Prepared::new([3, 5], model, vec![1.; 14385], 41.).unwrap();
    assert!(a.key() != b.key());
}

#[test]
#[ignore = "requires a GPU and UV_REPAIR_REFERENCE guarded native callback fixtures"]
fn complete_reference_conversion_and_classification_fixtures() {
    let directory = std::path::PathBuf::from(std::env::var_os("UV_REPAIR_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for fixture in manifest["cases"].as_array().unwrap() {
        let extent = [
            fixture["extent"][0].as_u64().unwrap() as u32,
            fixture["extent"][1].as_u64().unwrap() as u32,
        ];
        let read =
            |name: &str| std::fs::read(directory.join(fixture[name].as_str().unwrap())).unwrap();
        let rgb = read("rgb");
        let yuv = read("yuv");
        let rgb = gpu
            .upload("UV original RGB", bytemuck::cast_slice::<u8, f32>(&rgb))
            .unwrap();
        let yuv = gpu
            .upload("UV reference YUV", bytemuck::cast_slice::<u8, f32>(&yuv))
            .unwrap();
        for (input, inverse, expected) in [(&rgb, false, "yuv"), (&yuv, true, "restored")] {
            let output = convert(&gpu, input, extent, inverse, &mut Default::default()).unwrap();
            let actual = gpu.readback(&output).unwrap();
            compare(
                fixture["name"].as_str().unwrap(),
                expected,
                &actual,
                &read(expected),
            );
        }
        let model: [f64; 9] = fixture["model"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let intensity = read("intensity");
        let params = Prepared::new(
            extent,
            model,
            bytemuck::cast_slice::<u8, f32>(&intensity).to_vec(),
            fixture["threshold"].as_f64().unwrap() as f32,
        )
        .unwrap();
        let output = apply(&gpu, &rgb, &yuv, &params, &mut Default::default()).unwrap();
        compare(
            fixture["name"].as_str().unwrap(),
            "classified",
            &gpu.readback(&output.yuv).unwrap(),
            &read("classified"),
        );
        assert_eq!(
            gpu.readback(&output.guide).unwrap(),
            read("guide"),
            "guide {fixture}"
        );
        assert_eq!(
            gpu.readback(&output.mask).unwrap(),
            read("mask"),
            "mask {fixture}"
        );
    }
}

fn compare(case: &str, plane: &str, actual: &[u8], expected: &[u8]) {
    assert_eq!(actual.len(), expected.len());
    let mut changed = 0;
    let mut maximum = 0_f32;
    for (&actual, &expected) in bytemuck::cast_slice::<u8, f32>(actual)
        .iter()
        .zip(bytemuck::cast_slice::<u8, f32>(expected))
    {
        assert!(actual.is_finite());
        changed += usize::from(actual.to_bits() != expected.to_bits());
        maximum = maximum.max((actual - expected).abs());
    }
    eprintln!(
        "{case} {plane}: {changed}/{} differing words, max {maximum}",
        actual.len() / 4
    );
    assert!(maximum <= 0.001, "{case} {plane} error {maximum}");
}
