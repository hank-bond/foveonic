//! Checks active bounds and complete three-pass results against owned native fixtures.

use super::*;

const IDENTITY: [f64; 9] = [1., 0., 0., 0., 1., 0., 0., 0., 1.];

#[test]
fn excludes_uninitialized_odd_axis_tails_and_invalid_controls() {
    assert!(Prepared::new([19, 17], [1, 1, 18, 15], [1.; 3], IDENTITY).is_ok());
    assert!(Prepared::new([19, 17], [1, 1, 19, 15], [1.; 3], IDENTITY).is_err());
    assert!(Prepared::new([19, 17], [1, 1, 18, 16], [1.; 3], IDENTITY).is_err());
    assert!(Prepared::new([18, 16], [1, 1, 17, 14], [0.; 3], IDENTITY).is_err());
    let p = Prepared::new([18, 16], [1, 1, 17, 14], [1.; 3], IDENTITY).unwrap();
    assert_eq!(p.extent(), [18, 16]);
    let _ = p.key();
    assert_eq!(spatial(19).len(), 18);
}

#[test]
#[ignore = "requires GPU and PURPLE_FRINGE_REFERENCE native three-pass fixtures"]
fn complete_native_wrappers_preserve_immutable_pass_ownership() {
    let directory = std::path::PathBuf::from(std::env::var_os("PURPLE_FRINGE_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let ints = |name: &str| {
            case[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect::<Vec<_>>()
        };
        let floats = |name: &str| {
            case[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect::<Vec<_>>()
        };
        let prepared = Prepared::new(
            ints("extent").try_into().unwrap(),
            ints("active").try_into().unwrap(),
            floats("gains")
                .into_iter()
                .map(|v| v as f32)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            floats("matrix").try_into().unwrap(),
        )
        .unwrap();
        let bytes = std::fs::read(directory.join(case["input"].as_str().unwrap())).unwrap();
        let input = gpu
            .upload(
                "Native fringe input",
                bytemuck::cast_slice::<u8, f32>(&bytes),
            )
            .unwrap();
        let output = apply(&gpu, &input, &prepared, &mut Default::default()).unwrap();
        assert_eq!(gpu.readback(&input).unwrap(), bytes, "source changed");
        let actual = gpu.readback(&output).unwrap();
        let expected = std::fs::read(directory.join(case["output"].as_str().unwrap())).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&actual);
        let expected: &[f32] = bytemuck::cast_slice(&expected);
        assert_eq!(actual.len(), expected.len());
        let mut maximum = 0_f32;
        let mut different = 0;
        for (&a, &b) in actual.iter().zip(expected) {
            assert!(a.is_finite() && b.is_finite());
            maximum = maximum.max((a - b).abs());
            different += usize::from(a.to_bits() != b.to_bits());
        }
        eprintln!(
            "{}: {different}/{} differ, maximum {maximum}",
            case["input"],
            actual.len()
        );
        assert!(maximum <= 0.000_25, "{}: {maximum}", case["input"]);
    }
}
