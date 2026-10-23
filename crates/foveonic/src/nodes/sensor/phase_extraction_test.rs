//! Checks phase placement, signed code preservation and bounded extraction geometry.

use super::*;

#[test]
fn cell_extents_preserve_odd_sizes_and_reject_overflow() {
    for extent in [[1, 1], [3, 5], [8, 6], [2712, 1808]] {
        assert_eq!(Params::new(extent[0], extent[1]).unwrap().extent(), extent);
    }
    for [width, height] in [[0, 1], [1, 0], [u32::MAX, 1], [1, u32::MAX], [65536, 65536]] {
        assert!(Params::new(width, height).is_err());
    }
}

#[test]
#[ignore = "requires a GPU"]
fn packed_cells_preserve_spatial_phases_and_rgb_average() {
    let gpu = Gpu::new().unwrap();
    for [width, height] in [[1, 1], [3, 5], [8, 6], [9, 17]] {
        let mut source = Vec::new();
        let mut expected_rgb = Vec::new();
        let mut expected_blue = vec![0.; (width * height * 4) as usize];
        // Build the oracle in physical coordinates instead of using the shader's
        // destination indexing expression. Distinct phases expose transposes.
        for y in 0..height {
            for x in 0..width {
                let red = x as f32 - 17.;
                let green = y as f32 + 30000.;
                let phases = [
                    (x * 2 + y * 200) as f32 - 100.,
                    (x * 2 + 1 + y * 200) as f32 - 100.,
                    (x * 2 + (y * 2 + 1) * 100) as f32 - 100.,
                    (x * 2 + 1 + (y * 2 + 1) * 100) as f32 - 100.,
                ];
                source.extend([red, green]);
                source.extend(phases);
                expected_rgb.extend([red, green, phases.iter().sum::<f32>() / 4.]);
                for row in 0..2 {
                    for col in 0..2 {
                        let physical_x = x * 2 + col;
                        let physical_y = y * 2 + row;
                        expected_blue[(physical_y * (width * 2) + physical_x) as usize] =
                            phases[(row * 2 + col) as usize];
                    }
                }
            }
        }
        let input = gpu.upload("Phase fixture", &source).unwrap();
        let params = Params::new(width, height).unwrap();
        let (rgb, blue) = apply(&gpu, &input, &params, &mut Default::default()).unwrap();
        for (actual, expected) in [(rgb, expected_rgb), (blue, expected_blue)] {
            let bytes = gpu.readback(&actual).unwrap();
            assert_eq!(bytemuck::cast_slice::<u8, f32>(&bytes), expected);
        }
        let wrong_size = Params::new(width + 1, height).unwrap();
        assert!(apply(&gpu, &input, &wrong_size, &mut Default::default()).is_err());
    }
}

#[test]
fn calibration_parameters_require_finite_positive_scale_and_cap() {
    assert!(!Params::new(3, 5).unwrap().is_calibrated());
    assert!(Params::calibrated(3, 5, 2048, 1., 14300.)
        .unwrap()
        .is_calibrated());
    for (offset, scale, cap) in [
        (65536, 1., 14300.),
        (0, 0., 1.),
        (0, f32::NAN, 1.),
        (0, 1., f32::INFINITY),
        (0, 1., -1.),
    ] {
        assert!(Params::calibrated(3, 5, offset, scale, cap).is_err());
    }
}

#[test]
#[ignore = "requires a GPU and PHASE_EXTRACTION_REFERENCE with native RGB/blue planes and receipt.json"]
fn complete_calibrated_original_matches_native_planes() {
    let directory = std::path::PathBuf::from(std::env::var("PHASE_EXTRACTION_REFERENCE").unwrap());
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("receipt.json")).unwrap()).unwrap();
    let width = receipt["extent"][0].as_u64().unwrap() as u32;
    let height = receipt["extent"][1].as_u64().unwrap() as u32;
    let mut source = std::fs::read(receipt["source"].as_str().unwrap()).unwrap();
    source.truncate(width as usize * height as usize * 24);
    let gpu = Gpu::new().unwrap();
    let input = gpu
        .upload(
            "Native extraction original",
            bytemuck::cast_slice::<u8, f32>(&source),
        )
        .unwrap();
    let params = Params::calibrated(
        width,
        height,
        receipt["offset"].as_u64().unwrap() as u32,
        receipt["scale"].as_f64().unwrap() as f32,
        receipt["cap"].as_f64().unwrap() as f32,
    )
    .unwrap();
    let (rgb, blue) = apply(&gpu, &input, &params, &mut Default::default()).unwrap();
    for (name, output) in [("rgb", rgb), ("blue", blue)] {
        let actual = gpu.readback(&output).unwrap();
        let expected = std::fs::read(directory.join(format!("{name}.f32"))).unwrap();
        assert_eq!(actual.len(), expected.len());
        for (index, (actual, expected)) in actual
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.as_chunks::<4>().0)
            .enumerate()
        {
            assert_eq!(actual, expected, "{name} word {index}");
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn calibrated_signed_values_and_mono_cap_match_scalar_equations() {
    let gpu = Gpu::new().unwrap();
    let samples = [
        0., 65535., -100., 1000., 14300., 40000., 1024., 2048., 20000., 20000., 20000., 20000.,
    ];
    let input = gpu.upload("Calibration extremes", &samples).unwrap();
    for offset in [0, 256, 2048] {
        for scale in [0.25, 0.77, 1., 2.] {
            let cap = 14300. * scale;
            let params = Params::calibrated(2, 1, offset, scale, cap).unwrap();
            let (rgb, blue) = apply(&gpu, &input, &params, &mut Default::default()).unwrap();
            let rgb = gpu.readback(&rgb).unwrap();
            let blue = gpu.readback(&blue).unwrap();
            let corrected = |v: f32| (v - offset as f32) * scale;
            let mut expected_rgb = Vec::new();
            let mut expected_blue = [0.; 8];
            for (x, sample) in samples.as_chunks::<6>().0.iter().enumerate() {
                expected_rgb.extend([
                    corrected(sample[0]),
                    corrected(sample[1]),
                    corrected(sample[2..].iter().sum::<f32>() * 0.25),
                ]);
                for (phase, &v) in sample[2..].iter().enumerate() {
                    expected_blue[(phase / 2) * 4 + x * 2 + phase % 2] = corrected(v).min(cap);
                }
            }
            assert_eq!(bytemuck::cast_slice::<u8, f32>(&rgb), expected_rgb);
            assert_eq!(bytemuck::cast_slice::<u8, f32>(&blue), expected_blue);
        }
    }
}

#[test]
fn channel_gains_require_finite_positive_values() {
    for gains in [
        [0., 1., 1.],
        [-1., 1., 1.],
        [f64::NAN; 3],
        [f64::INFINITY; 3],
        [f64::MAX; 3],
    ] {
        assert!(Params::channel_calibrated(3, 5, 2048, 1., gains).is_err());
    }
    let params = Params::channel_calibrated(3, 5, 2048, 1., [4., 4., 2.]).unwrap();
    assert!(params.is_calibrated());
    assert_eq!(params.gain_high, [4., 4., 2., 0.]);
}

#[test]
#[ignore = "requires a GPU"]
fn channel_calibration_preserves_native_rounding_gain_caps_and_source_samples() {
    let gpu = Gpu::new().unwrap();
    for [width, height] in [[3, 5], [18, 9]] {
        let values = (0..width * height * 6)
            .map(|i| (((i * 71 + i * i * 3) % 12007) as f32) * 19.25 - 1000.)
            .collect::<Vec<_>>();
        let input = gpu.upload("Channel calibration fixture", &values).unwrap();
        for gains in [
            [1.; 3],
            [4., 4., 2.],
            [1.1000000001, 0.7993000149726868, 0.5666800141334534],
        ] {
            for scale in [0.25_f32, 0.77, 1.] {
                let params = Params::channel_calibrated(width, height, 2048, scale, gains).unwrap();
                let (rgb, blue) = apply(&gpu, &input, &params, &mut Default::default()).unwrap();
                let actual_rgb = gpu.readback(&rgb).unwrap();
                let actual_rgb = bytemuck::cast_slice::<u8, f32>(&actual_rgb);
                let actual_blue = gpu.readback(&blue).unwrap();
                let actual_blue = bytemuck::cast_slice::<u8, f32>(&actual_blue);
                let corrected = |v: f32, channel: usize| {
                    (f64::from((v - 2048.) * scale) * gains[channel]).min(65535.) as f32
                };
                for y in 0..height {
                    for x in 0..width {
                        let cell = (y * width + x) as usize;
                        let source = &values[cell * 6..cell * 6 + 6];
                        let mean = ((source[2] * 0.25 + source[3] * 0.25) + source[4] * 0.25)
                            + source[5] * 0.25;
                        for (channel, expected) in [
                            corrected(source[0], 0),
                            corrected(source[1], 1),
                            corrected(mean, 2),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            assert_eq!(
                                actual_rgb[cell * 3 + channel].to_bits(),
                                expected.to_bits(),
                                "cell={cell} channel={channel} gains={gains:?} scale={scale}"
                            );
                        }
                        for phase in 0..4 {
                            let index =
                                ((y * 2 + phase / 2) * width * 2 + x * 2 + phase % 2) as usize;
                            assert_eq!(
                                actual_blue[index].to_bits(),
                                corrected(source[phase as usize + 2], 2).to_bits()
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&values)
        );
    }
}
