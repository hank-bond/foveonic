//! Checks triangular RGB pyramid geometry and independent native pixels.

use super::*;

#[test]
fn rejects_empty_levels_and_keeps_explicit_ratio_geometry() {
    assert!(Prepared::new([1, 4], Ratio::Half).is_err());
    assert!(Prepared::new([0, 4], Ratio::Double).is_err());
    assert_eq!(
        Prepared::new([33, 35], Ratio::Half)
            .unwrap()
            .output_extent(),
        [16, 17]
    );
    assert_eq!(
        Prepared::new([33, 35], Ratio::Double)
            .unwrap()
            .output_extent(),
        [66, 70]
    );
}

#[test]
#[ignore = "requires PYRAMID_REFERENCE native fixtures and a GPU"]
fn native_separable_images_match() {
    let directory = std::path::PathBuf::from(std::env::var_os("PYRAMID_REFERENCE").unwrap());
    let records: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for record in records.as_array().unwrap() {
        let name = record["name"].as_str().unwrap();
        let extent = [
            record["width"].as_u64().unwrap() as u32,
            record["height"].as_u64().unwrap() as u32,
        ];
        let ratio = if record["scale"].as_f64().unwrap() < 1. {
            Ratio::Half
        } else {
            Ratio::Double
        };
        let prepared = Prepared::new(extent, ratio).unwrap();
        let input = gpu
            .upload(
                "Native RGB input",
                &std::fs::read(directory.join(format!("{name}-input.f32"))).unwrap(),
            )
            .unwrap();
        let result = apply(&gpu, &input, &prepared, &mut Profile::default()).unwrap();
        let actual = gpu.readback(&result).unwrap();
        let expected = std::fs::read(directory.join(format!("{name}-output.f32"))).unwrap();
        assert_eq!(actual, expected, "{name}");
    }
}
