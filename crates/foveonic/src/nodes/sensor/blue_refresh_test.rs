//! Checks ordered blue averaging, immutable inputs and logical image bounds.

use super::*;

#[test]
fn rejects_empty_and_overflowing_geometry() {
    for extent in [[0, 1], [1, 0], [u32::MAX, 1], [65536, 65536]] {
        assert!(Params::new(extent).is_err());
    }
    assert!(Params::new([3, 5]).is_ok());
}

#[test]
#[ignore = "requires a GPU and BLUE_REFRESH_REFERENCE containing native callback fixtures"]
fn original_and_bounded_refresh_match_native_logical_pixels() {
    let directory = std::path::PathBuf::from(std::env::var("BLUE_REFRESH_REFERENCE").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let extent = [
            case["extent"][0].as_u64().unwrap() as u32,
            case["extent"][1].as_u64().unwrap() as u32,
        ];
        let read =
            |suffix: &str| std::fs::read(directory.join(format!("{name}-{suffix}.f32"))).unwrap();
        let rgb = read("rgb");
        let blue = read("blue");
        let expected = read("output");
        let input_rgb = gpu
            .upload("Refresh RGB", bytemuck::cast_slice::<u8, f32>(&rgb))
            .unwrap();
        let input_blue = gpu
            .upload("Refresh blue", bytemuck::cast_slice::<u8, f32>(&blue))
            .unwrap();
        let output = apply(
            &gpu,
            &input_rgb,
            &input_blue,
            &Params::new(extent).unwrap(),
            &mut Profile::default(),
        )
        .unwrap();
        let actual = gpu.readback(&output).unwrap();
        assert_eq!(actual.len(), expected.len());
        for (i, (actual, expected)) in actual
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.as_chunks::<4>().0)
            .enumerate()
        {
            assert_eq!(actual, expected, "{name} word {i}");
        }
        assert_eq!(gpu.readback(&input_rgb).unwrap(), rgb);
        assert_eq!(gpu.readback(&input_blue).unwrap(), blue);
    }
}
