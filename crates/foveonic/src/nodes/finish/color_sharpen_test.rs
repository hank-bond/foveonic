//! Checks native channel-sharpen fixtures, wrapped sums and defined storage boundaries.

use super::*;

#[test]
fn rejects_invalid_geometry_and_controls() {
    assert!(Params::new([2, 7], 1., 1.).is_err());
    assert!(Params::new([17, 7], 0., 1.).is_err());
    assert!(Params::new([17, 7], 1., f32::NAN).is_err());
}

#[test]
#[ignore = "requires GPU"]
fn channel_sharpen_matches_native_wrapped_sum_fixtures() {
    // Captured from the reference worker with an explicitly initialized final
    // threshold. Only its unowned bottom halo is excluded from comparison.
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("color_sharpen.fixture")).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let read = |name: &str| -> Vec<f32> {
            case[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap() as f32)
                .collect()
        };
        let source = read("input");
        let input = gpu.upload("Channel sharpening fixture", &source).unwrap();
        let extent = [17, 7];
        let params = Params::new(
            extent,
            case["normalization"].as_f64().unwrap() as f32,
            case["amount"].as_f64().unwrap() as f32,
        )
        .unwrap();
        let output = apply(&gpu, &input, &params, &mut Default::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        let expected = read("expected");
        for i in 0..17 * 6 * 3 {
            assert_eq!(actual[i], expected[i], "sample {i}, case {case}");
        }
        assert!(actual
            .iter()
            .all(|v| v.is_finite() && (0. ..=65535.).contains(v)));
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&source)
        );
    }
}
