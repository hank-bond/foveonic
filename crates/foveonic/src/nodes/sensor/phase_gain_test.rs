//! Checks resize coordinates, threshold preservation and independently generated blue correction fixtures.

use super::*;

#[test]
fn map_geometry_and_values_are_bounded() {
    for extent in [[0, 1], [1, 2], [u32::MAX, u32::MAX]] {
        assert!(Prepared::new(extent, [2, 2], vec![1.; 4], 14300.).is_err());
    }
    for map_extent in [[1, 2], [3, 2], [2, 3], [4, 4]] {
        assert!(Prepared::new([3, 5], map_extent, vec![1.; 4], 14300.).is_err());
    }
    assert!(Prepared::new([3, 5], [2, 2], vec![f32::NAN; 4], 14300.).is_err());
    assert!(Prepared::new([3, 5], [2, 2], vec![1.; 4], f32::NAN).is_err());
    assert!(Prepared::new([3, 5], [2, 2], vec![1.; 4], 14300.).is_ok());
}

#[test]
fn zero_support_identity_and_reduction_axes_are_normalized() {
    let identity = axis(7, 7);
    for (index, row) in identity.iter().enumerate() {
        assert_eq!(row.index[0], index as u32);
        assert_eq!(row.weight, [1., 0.]);
    }
    for (source, destination) in [(92, 10), (58, 9), (92, 100), (58, 65)] {
        for row in axis(source, destination) {
            assert!(row.index.iter().all(|&i| i < source));
            assert!((row.weight.iter().sum::<f32>() - 1.).abs() <= f32::EPSILON);
        }
    }
}

#[test]
#[ignore = "requires a GPU and PHASE_GAIN_REFERENCE containing independent correction fixtures"]
fn complete_phase_correction_matches_independent_fixtures() {
    let directory = std::path::PathBuf::from(std::env::var("PHASE_GAIN_REFERENCE").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let extent = [
            case["extent"][0].as_u64().unwrap() as u32,
            case["extent"][1].as_u64().unwrap() as u32,
        ];
        let read = |suffix: &str| -> Vec<f32> {
            let bytes = std::fs::read(directory.join(format!("{name}-{suffix}.f32"))).unwrap();
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|v| f32::from_le_bytes(*v))
                .collect()
        };
        let map = read("map");
        let source = read("input");
        let expected = read("output");
        let params = Prepared::new(extent, [184, 116], map, 14300.).unwrap();
        let input = gpu.upload("Phase gain fixture", &source).unwrap();
        let output = apply(&gpu, &input, &params, &mut Default::default()).unwrap();
        let actual = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&actual);
        let mut changed = 0;
        let mut maximum = 0_f32;
        let mut total_error = 0_f64;
        for (index, (&actual, &expected)) in actual.iter().zip(&expected).enumerate() {
            let error = (actual - expected).abs();
            maximum = maximum.max(error);
            total_error += f64::from(error);
            if actual.to_bits() != expected.to_bits() {
                changed += 1;
            }
            assert!(
                actual.to_bits() == expected.to_bits(),
                "{name} sample {index}: {actual} vs {expected}"
            );
            let x = index % extent[0] as usize;
            let y = index / extent[0] as usize;
            if source[index] >= 14300.
                || x >= (extent[0] / 2 * 2) as usize
                || y >= (extent[1] / 2 * 2) as usize
            {
                assert_eq!(
                    actual.to_bits(),
                    source[index].to_bits(),
                    "unchanged {name} {index}"
                );
            }
        }
        eprintln!(
            "{name}: {changed}/{} different, max {maximum}, mean {}",
            actual.len(),
            total_error / actual.len() as f64
        );
    }
}

#[test]
#[ignore = "requires a GPU, PHASE_EXTRACTION_REFERENCE and PHASE_GAIN_REFERENCE for completed-operation timings"]
fn profile_original_cell_extraction_and_phase_gain() {
    let extraction = std::path::PathBuf::from(std::env::var("PHASE_EXTRACTION_REFERENCE").unwrap());
    let gain = std::path::PathBuf::from(std::env::var("PHASE_GAIN_REFERENCE").unwrap());
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(extraction.join("receipt.json")).unwrap()).unwrap();
    let width = receipt["extent"][0].as_u64().unwrap() as u32;
    let height = receipt["extent"][1].as_u64().unwrap() as u32;
    let mut source = std::fs::read(receipt["source"].as_str().unwrap()).unwrap();
    source.truncate(width as usize * height as usize * 24);
    let map = std::fs::read(gain.join("c81-original-map.f32")).unwrap();
    let gpu = Gpu::new().unwrap();
    let input = gpu
        .upload(
            "Original packed phase cells",
            bytemuck::cast_slice::<u8, f32>(&source),
        )
        .unwrap();
    drop(source);
    let extraction =
        super::super::phase_extraction::Params::calibrated(width, height, 2048, 1., 14300.)
            .unwrap();
    let prepared = Prepared::new(
        [width * 2, height * 2],
        [184, 116],
        bytemuck::cast_slice::<u8, f32>(&map).to_vec(),
        14300.,
    )
    .unwrap();
    for iteration in 0..6 {
        let mut extraction_profile = Profile::new(crate::profile::Mode::Gpu);
        let (_, blue) = super::super::phase_extraction::apply(
            &gpu,
            &input,
            &extraction,
            &mut extraction_profile,
        )
        .unwrap();
        let mut gain_profile = Profile::new(crate::profile::Mode::Gpu);
        let _ = apply(&gpu, &blue, &prepared, &mut gain_profile).unwrap();
        eprintln!(
            "iteration {iteration} extraction {:?} phase_gain {:?}",
            extraction_profile.gpu, gain_profile.gpu
        );
    }
}
