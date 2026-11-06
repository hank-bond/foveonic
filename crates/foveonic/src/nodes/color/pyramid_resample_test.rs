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
#[ignore = "requires a working GPU adapter"]
fn half_sampling_preserves_constants_and_filters_alternating_pixels() {
    let gpu = Gpu::new().unwrap();
    let pixels: Vec<f32> = (0..8)
        .flat_map(|_| (0..8).flat_map(|x| [7., if x % 2 == 0 { 0. } else { 16. }, -3.]))
        .collect();
    let input = gpu.upload("Alternating RGB", &pixels).unwrap();
    let prepared = Prepared::new([8, 8], Ratio::Half).unwrap();
    let output = apply(&gpu, &input, &prepared, &mut Profile::default()).unwrap();
    let bytes = gpu.readback(&output).unwrap();
    let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
    for y in 0..4 {
        for x in 1..3 {
            assert_eq!(&actual[(y * 4 + x) * 3..(y * 4 + x + 1) * 3], [7., 8., -3.]);
        }
    }
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

#[test]
fn signed_rounding_is_part_of_the_filter_identity() {
    let float = Prepared::new([8, 8], Ratio::Half).unwrap();
    let signed = float.with_signed_codes();
    assert_ne!(float.key(), signed.key());
    assert_eq!(signed.extent(), float.extent());
    assert_eq!(signed.output_extent(), float.output_extent());
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn signed_pooling_rounds_after_both_axes_without_negative_bias() {
    let gpu = Gpu::new().unwrap();
    let prepared = Prepared::new([8, 8], Ratio::Half)
        .unwrap()
        .with_signed_codes();
    let pixels: Vec<f32> = (0..8)
        .flat_map(|y| {
            (0..8).flat_map(move |x| {
                let v = if x % 2 == 0 && y % 2 == 0 { 1. } else { 0. };
                [v, -2. * v, -32768.]
            })
        })
        .collect();
    let input = gpu.upload("Signed fractional pooling", &pixels).unwrap();
    let output = apply(&gpu, &input, &prepared, &mut Profile::default()).unwrap();
    let bytes = gpu.readback(&output).unwrap();
    let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
    for y in 1..3 {
        for x in 1..3 {
            assert_eq!(
                &actual[(y * 4 + x) * 3..(y * 4 + x + 1) * 3],
                [0., -1., -32768.]
            );
        }
    }
    assert!(actual.iter().all(|v| v.fract() == 0.));
}

#[test]
fn doubling_can_restore_the_dropped_edge_of_an_odd_grid() {
    let half = Prepared::new([33, 35], Ratio::Half).unwrap();
    let restored = Prepared::double_to(half.output_extent(), [33, 35]).unwrap();
    assert_eq!(restored.extent(), [16, 17]);
    assert_eq!(restored.output_extent(), [33, 35]);
    assert!(Prepared::double_to([16, 17], [34, 35]).is_err());
    assert!(Prepared::double_to([16, 17], [31, 35]).is_err());
}

#[test]
fn ceil_half_retains_coverage_of_odd_sensor_edges() {
    assert_eq!(
        Prepared::half_ceil([33, 35]).unwrap().output_extent(),
        [17, 18]
    );
    assert_eq!(Prepared::half_ceil([1, 1]).unwrap().output_extent(), [1, 1]);
    assert!(Prepared::half_ceil([0, 1]).is_err());
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn odd_ceil_half_keeps_replicated_signed_edges() {
    let gpu = Gpu::new().unwrap();
    let pixels: Vec<f32> = (0..9 * 7).flat_map(|_| [32767., -32768., -3.]).collect();
    let input = gpu.upload("Odd signed sensor extent", &pixels).unwrap();
    let prepared = Prepared::half_ceil([9, 7]).unwrap().with_signed_codes();
    let output = apply(&gpu, &input, &prepared, &mut Profile::default()).unwrap();
    let bytes = gpu.readback(&output).unwrap();
    assert_eq!(
        bytemuck::cast_slice::<u8, f32>(&bytes),
        (0..5 * 4)
            .flat_map(|_| [32767., -32768., -3.])
            .collect::<Vec<_>>()
    );
}
