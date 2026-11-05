//! Checks haze estimation, signed correction, neutral bypass, persistence and retained GPU inputs.

use super::*;

// These arithmetic fixtures explicitly use sRGB coordinates and brightness weights.
// Camera recipes supply their own calibrated basis and raw brightness proxy.
fn test_basis(space: ColorSpace) -> Basis {
    let rows = TiffColorSpace::from(space).transform(ColorSpace::Srgb);
    Basis::new(
        space,
        std::array::from_fn(|i| f64::from(rows[i / 3][i % 3])),
        [0.2126, 0.7152, 0.0722],
    )
    .unwrap()
}

// Apply the shared shoulder to independently measured correction candidates.
fn protected_samples(input: &[f32], candidates: &[f32], white: f32) -> Vec<f32> {
    input
        .as_chunks::<3>()
        .0
        .iter()
        .zip(candidates.as_chunks::<3>().0.iter())
        .flat_map(|(input, candidate)| {
            let loss = (0..3)
                .map(|c| (input[c] - candidate[c]).max(0.) / input[c].abs().max(1e-6 * white))
                .fold(0., f32::max);
            (0..3).map(move |c| input[c] + (candidate[c] - input[c]) / (1. + loss))
        })
        .collect()
}

#[test]
fn dehaze_settings_validate_roundtrip_and_follow_corrections_scope() {
    use crate::{
        render::{EditScope, EditSection, Settings},
        source::ProcessingFamily,
    };
    let mut settings = Settings::default();
    assert_eq!(settings.dehaze, Controls::default());
    assert!(!settings.dehaze_auto());
    assert!(!settings.dehaze_active());
    assert_eq!(settings.dehaze.shadow, Some([0.5; 3]));
    settings.dehaze = Controls {
        amount: 45.,
        shadow: Some([0.08, 0.1, 0.12]),
    };
    let saved = serde_json::to_string(&settings).unwrap();
    let restored: Settings = serde_json::from_str(&saved).unwrap();
    assert_eq!(settings, restored);
    let mut target = Settings::default();
    EditScope::Tool(EditSection::Tone).apply(&settings, &mut target, ProcessingFamily::Merrill);
    assert_eq!(target.dehaze, Controls::default());
    EditScope::Tool(EditSection::Corrections).apply(
        &settings,
        &mut target,
        ProcessingFamily::Merrill,
    );
    assert_eq!(target.dehaze, settings.dehaze);
    let mut next = settings.clone();
    next.dehaze.amount = 50.;
    assert_eq!(settings.changed_parameters(&next), vec!["dehaze-amount"]);
    next = settings.clone();
    next.dehaze.shadow = None;
    assert_eq!(settings.changed_parameters(&next), vec!["dehaze-shadow"]);
    target.processing_sections.corrections = false;
    assert!(!target.dehaze_active());
    assert_eq!(
        target
            .processing_settings(ProcessingFamily::Merrill)
            .dehaze
            .amount,
        0.
    );
    assert_eq!(target.dehaze, settings.dehaze);
    assert_eq!(settings.dng_settings().dehaze, Controls::default());
    settings.dehaze_estimate = true;
    assert!(!settings.photographic_recipe().dehaze_estimate);
    assert!(!settings.dng_settings().dehaze_estimate);
    let restored: Settings =
        serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
    assert!(!restored.dehaze_estimate);
    for bad in [f32::NAN, f32::INFINITY, -101., 101.] {
        assert!(Controls {
            amount: bad,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
    for rgb in [[0.; 3], [-0.1, 0.1, 0.1], [f32::NAN, 0.1, 0.1], [2.; 3]] {
        assert!(Controls {
            shadow: Some(rgb),
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}

fn floats(gpu: &Gpu, buffer: &ReadGuard) -> Vec<f32> {
    bytemuck::cast_slice::<u8, f32>(&gpu.readback(buffer).unwrap()).to_vec()
}

#[test]
#[ignore = "requires a GPU"]
fn dehaze_estimates_patches_and_bypasses_empty_or_neutral_images_exactly() {
    let gpu = Gpu::new().unwrap();
    let extent = [64, 48];
    for rgb in [[0.; 3], [0.08, 0.1, 0.12], [0.7, 0.8, 0.9]] {
        let values: Vec<f32> = (0..extent[0] * extent[1]).flat_map(|_| rgb).collect();
        let input = gpu.upload("Haze fixture", &values).unwrap();
        let params = Estimate::new(
            extent,
            test_basis(ColorSpace::Srgb),
            1.,
            false,
            [0, 0, 64, 48],
        );
        let analysis = estimate(&gpu, &input, params, &mut Profile::default()).unwrap();
        let stats = floats(&gpu, &analysis);
        assert_eq!(stats[3] > 0., rgb[0] > 0. && rgb[1] < 0.25);
        if stats[3] > 0. {
            for c in 0..3 {
                assert!((stats[c] - rgb[c]).abs() < 0.00002, "{stats:?}");
            }
        }
        let patch = estimate(
            &gpu,
            &input,
            Estimate::patch(
                extent,
                test_basis(ColorSpace::Srgb),
                1.,
                false,
                [0, 0],
                [0, 0, 64, 48],
            ),
            &mut Profile::default(),
        )
        .unwrap();
        assert_eq!(floats(&gpu, &patch)[3] > 0., rgb[0] > 0.);
        for amount in [0., 50., -50.] {
            let output = apply(
                &gpu,
                &input,
                &analysis,
                Params::new(
                    extent,
                    test_basis(ColorSpace::Srgb),
                    1.,
                    false,
                    Controls {
                        amount,
                        shadow: None,
                    },
                ),
                extent,
                &mut Profile::default(),
            )
            .unwrap();
            let actual = floats(&gpu, &output);
            assert!(actual.iter().all(|v| v.is_finite()));
            if amount == 0. || stats[3] == 0. {
                assert_eq!(actual, values);
            }
        }
        assert_eq!(floats(&gpu, &input), values);
    }
}

#[test]
#[ignore = "requires a GPU"]
fn dehaze_removes_or_adds_haze_preserves_headroom_and_monochrome() {
    let gpu = Gpu::new().unwrap();
    let values = [
        0.08, 0.1, 0.12, 0.25, 0.3, 0.35, 1.5, 1.5, 1.5, -0.01, -0.01, -0.01,
    ];
    let input = gpu.upload("Haze response fixture", &values).unwrap();
    let analysis = gpu
        .upload("Known haze", &[0.08f32, 0.1, 0.12, 1., 0., 1., 0., 0.])
        .unwrap();
    let mut outputs = Vec::new();
    for amount in [50., -50.] {
        let result = apply(
            &gpu,
            &input,
            &analysis,
            Params::new(
                [4, 1],
                test_basis(ColorSpace::Srgb),
                1.,
                false,
                Controls {
                    amount,
                    shadow: None,
                },
            ),
            [4, 1],
            &mut Profile::default(),
        )
        .unwrap();
        let actual = floats(&gpu, &result);
        assert!(actual.iter().all(|v| v.is_finite()));
        let expected = if amount > 0. {
            [
                0.067_120_84,
                0.08390106,
                0.10068126,
                0.24151821,
                0.28752476,
                0.3335312,
                1.504567,
                1.504567,
                1.504567,
            ]
        } else {
            [
                0.08, 0.1, 0.12, 0.16499999, 0.20000002, 0.23499998, 0.79859596, 0.79859596,
                0.79859596,
            ]
        };
        // Retained numerical measurements include the positive upper-reference
        // normalization and negative amount scaling, with no input clipping at white.
        let expected = if amount > 0. {
            protected_samples(&values[..9], &expected, 1.)
        } else {
            expected.to_vec()
        };
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.00002,
                "{amount}: {actual} != {expected}"
            );
        }
        assert_eq!(&actual[9..], &values[9..]);
        outputs.push(actual);
    }
    assert!(outputs[0][1] < values[1]);
    assert!((outputs[1][4] - 0.1).abs() < (values[4] - 0.1).abs());
    let mono = apply(
        &gpu,
        &input,
        &analysis,
        Params::new(
            [4, 1],
            test_basis(ColorSpace::Srgb),
            1.,
            true,
            Controls {
                amount: 50.,
                shadow: None,
            },
        ),
        [4, 1],
        &mut Profile::default(),
    )
    .unwrap();
    for pixel in floats(&gpu, &mono).as_chunks::<3>().0 {
        assert_eq!(pixel[0], pixel[1]);
        assert_eq!(pixel[1], pixel[2]);
    }
    assert_eq!(floats(&gpu, &input), values);
}

#[test]
#[ignore = "requires a GPU"]
fn dehaze_candidate_heap_matches_independent_spatial_selection() {
    let gpu = Gpu::new().unwrap();
    let [width, height] = [96usize, 80usize];
    let mut seed = 1031u32;
    let pixels: Vec<[u32; 3]> = (0..width * height)
        .map(|_| {
            std::array::from_fn(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                1000 + seed % 9000
            })
        })
        .collect();
    let values: Vec<_> = pixels
        .iter()
        .flatten()
        .map(|v| *v as f32 / 65535.)
        .collect();
    let input = gpu.upload("Spatial haze fixture", &values).unwrap();
    let params = Estimate::new(
        [width as u32, height as u32],
        test_basis(ColorSpace::Srgb),
        1.,
        false,
        [0, 0, width as u32, height as u32],
    );
    let result = estimate(&gpu, &input, params, &mut Profile::default()).unwrap();
    let actual = floats(&gpu, &result);
    let radius = 16;
    let mut candidates = Vec::new();
    for y in radius..height - radius {
        for x in radius..width - radius {
            let score = (y - radius..=y + radius)
                .flat_map(|yy| (x - radius..=x + radius).map(move |xx| yy * width + xx))
                .map(|i| *pixels[i].iter().min().unwrap())
                .min()
                .unwrap();
            let rgb = pixels[y * width + x];
            let intensity = (rgb[0] >> 2) + (rgb[1] >> 1) + (rgb[2] >> 2);
            candidates.push((std::cmp::Reverse(score), intensity, y * width + x));
        }
    }
    candidates.sort_unstable();
    let selected = &candidates[..100];
    for c in 0..3 {
        let expected = selected
            .iter()
            .map(|(_, _, i)| pixels[*i][c] as f64)
            .sum::<f64>()
            / (100. * 65535.);
        assert!(
            (f64::from(actual[c]) - expected).abs() < 0.000002,
            "{actual:?}, channel {c} expected {expected}"
        );
    }
}

#[test]
#[ignore = "requires a GPU"]
fn dehaze_tiny_chromatic_gates_preserve_saturated_colors() {
    let gpu = Gpu::new().unwrap();
    let input = gpu
        .upload(
            "Saturated haze fixture",
            &[0.0f32, 0.15, 0.65, 0.8, 0.01, 0.4, 0., 0.02, 2.],
        )
        .unwrap();
    let analysis = gpu
        .upload("Known haze", &[0.08f32, 0.1, 0.12, 1., 0., 1., 0., 0.])
        .unwrap();
    for (amount, expected) in [
        (
            35.,
            [
                0.,
                0.15061264,
                0.6526547,
                0.8032674,
                0.010040842,
                0.4016337,
                0.,
                0.020081684,
                2.0081685,
            ],
        ),
        (
            50.,
            [
                0.,
                0.15102147,
                0.6544263,
                0.80544776,
                0.010068097,
                0.40272388,
                0.,
                0.020136194,
                2.0136194,
            ],
        ),
        (
            100.,
            [
                0., 0.15461251, 0.6699875, 0.8246, 0.0103075, 0.4123, 0., 0.020615, 2.0615,
            ],
        ),
    ] {
        let result = apply(
            &gpu,
            &input,
            &analysis,
            Params::new(
                [3, 1],
                test_basis(ColorSpace::Srgb),
                1.,
                false,
                Controls {
                    amount,
                    shadow: None,
                },
            ),
            [3, 1],
            &mut Profile::default(),
        )
        .unwrap();
        for (actual, expected) in floats(&gpu, &result).iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.00002,
                "amount {amount}: {actual} != {expected}"
            );
        }
    }
    let wrong_estimate = gpu
        .upload(
            "Manual color overrides Auto",
            &[0.9f32, 0.8, 0.7, 1., 0., 1., 0., 0.],
        )
        .unwrap();
    for space in [
        ColorSpace::Srgb,
        ColorSpace::AdobeRgb,
        ColorSpace::ProPhotoRgb,
    ] {
        let from = TiffColorSpace::from(space).transform(ColorSpace::Srgb);
        let to = TiffColorSpace::Srgb.transform(space);
        let transform = |v: [f32; 3], rows: [[f32; 4]; 3]| {
            rows.map(|r| (0..3).map(|c| r[c] * v[c]).sum::<f32>())
        };
        for white in [1., 4095.] {
            let original = transform([0., 0.15, 0.65], from).map(|v| v * white);
            let input = gpu.upload("Working-space haze fixture", &original).unwrap();
            let result = apply(
                &gpu,
                &input,
                &wrong_estimate,
                Params::new(
                    [1, 1],
                    test_basis(space),
                    white,
                    false,
                    Controls {
                        amount: 35.,
                        shadow: Some([0.08, 0.1, 0.12]),
                    },
                ),
                [1, 1],
                &mut Profile::default(),
            )
            .unwrap();
            let actual = floats(&gpu, &result);
            let rgb = transform(
                [actual[0] / white, actual[1] / white, actual[2] / white],
                to,
            );
            for (actual, expected) in rgb.into_iter().zip([0., 0.15061264, 0.6526547]) {
                assert!(
                    (actual - expected).abs() < 0.00005,
                    "{space:?}, white {white}: {rgb:?}"
                );
            }
        }
    }
}

#[test]
fn camera_basis_normalizes_gain_and_rejects_invalid_calibration() {
    let camera = [2., -1., 0., -1., 2., 0., 0., -1., 2.];
    let basis = Basis::new(ColorSpace::Srgb, camera, [0.299, 0.587, 0.114]).unwrap();
    let scaled = Basis::new(
        ColorSpace::Srgb,
        camera.map(|v| v * 8.),
        [0.299, 0.587, 0.114],
    )
    .unwrap();
    assert_eq!(bytemuck::bytes_of(&basis), bytemuck::bytes_of(&scaled));
    for m in [
        [0.; 9],
        [f64::NAN; 9],
        [f64::INFINITY; 9],
        [-1., 0., 0., 0., 1., 0., 0., 0., 1.],
    ] {
        assert!(Basis::new(ColorSpace::Srgb, m, [0.299, 0.587, 0.114]).is_err());
    }
}

#[test]
#[ignore = "requires a GPU"]
fn camera_coordinates_control_candidate_selection_and_color_transport() {
    let gpu = Gpu::new().unwrap();
    let matrix = [2., -1., 0., -1., 2., 0., 0., -1., 2.];
    let basis = Basis::new(ColorSpace::Srgb, matrix, [0.299, 0.587, 0.114]).unwrap();
    // The neutral half has the larger minimum in working RGB. The other half
    // has the larger camera-channel minimum and must supply the haze estimate.
    let values: Vec<f32> = (0..96)
        .flat_map(|_| {
            (0..128).flat_map(|x| {
                if x < 64 {
                    [0.13, 0.13, 0.13]
                } else {
                    [0.1, 0.25, 0.3]
                }
            })
        })
        .collect();
    let input = gpu.upload("Camera candidate fixture", &values).unwrap();
    let analysis = estimate(
        &gpu,
        &input,
        Estimate::new([128, 96], basis, 1., false, [0, 0, 128, 96]),
        &mut Profile::default(),
    )
    .unwrap();
    let stats = floats(&gpu, &analysis);
    assert!(stats[3] > 0.);
    for (actual, expected) in stats[..3].iter().zip([0.1, 0.25, 0.3]) {
        assert!((actual - expected).abs() < 0.00005, "{stats:?}");
    }
    for shadow in [None, Some([stats[0], stats[1], stats[2]])] {
        let output = apply(
            &gpu,
            &input,
            &analysis,
            Params::new(
                [128, 96],
                basis,
                1.,
                false,
                Controls {
                    amount: 35.,
                    shadow,
                },
            ),
            [128, 96],
            &mut Profile::default(),
        )
        .unwrap();
        let pixels = floats(&gpu, &output);
        assert!(pixels.iter().all(|v| v.is_finite()));
        if shadow.is_none() {
            // Keep the first result alive for the exact Auto-to-manual check.
            let manual = apply(
                &gpu,
                &input,
                &analysis,
                Params::new(
                    [128, 96],
                    basis,
                    1.,
                    false,
                    Controls {
                        amount: 35.,
                        shadow: Some([stats[0], stats[1], stats[2]]),
                    },
                ),
                [128, 96],
                &mut Profile::default(),
            )
            .unwrap();
            assert_eq!(pixels, floats(&gpu, &manual));
        }
    }
    assert_eq!(values, floats(&gpu, &input));
}

#[test]
#[ignore = "requires a GPU"]
fn camera_coordinate_response_matches_measured_points_and_retains_signed_samples() {
    let gpu = Gpu::new().unwrap();
    let basis = Basis::new(
        ColorSpace::Srgb,
        [2., -1., 0., -1., 2., 0., 0., -1., 2.],
        [0.299, 0.587, 0.114],
    )
    .unwrap();
    let values = [
        0.12, 0.21, 0.26, 0.25, 0.4, 0.45, -0.03, 0.09, 0.35, -0.07, 0.11, 0.35, 1.5, 1.5, 1.5,
    ];
    let input = gpu.upload("Camera response fixture", &values).unwrap();
    let analysis = gpu
        .upload(
            "Canonical haze and camera levels",
            &[0.4793f32, 0.6383, 0.7454, 1., 1. / 255., 1., 0., 0.],
        )
        .unwrap();
    for (amount, expected) in [
        (
            35.,
            [
                0.09087277,
                0.17826002,
                0.22520802,
                0.21328676,
                0.36208934,
                0.40361936,
                -0.03458442,
                0.09215608,
                0.36676046,
                -0.06990925,
                0.1098185,
                0.36676046,
                1.533585,
                1.533585,
                1.533585,
            ],
        ),
        (
            100.,
            [
                0.02983372,
                0.0964794,
                0.12837347,
                0.09639147,
                0.25183288,
                0.26364862,
                -0.04592877,
                0.11654758,
                0.46857968,
                -0.0823888,
                0.13477759,
                0.46857968,
                1.6617016,
                1.6617016,
                1.6617016,
            ],
        ),
    ] {
        let output = apply(
            &gpu,
            &input,
            &analysis,
            Params::new(
                [5, 1],
                basis,
                1.,
                false,
                Controls {
                    amount,
                    shadow: None,
                },
            ),
            [5, 1],
            &mut Profile::default(),
        )
        .unwrap();
        let expected = protected_samples(&values, &expected, 1.);
        for (actual, expected) in floats(&gpu, &output).iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.00002,
                "amount {amount}: {actual} != {expected}"
            );
        }
    }
    assert_eq!(values.to_vec(), floats(&gpu, &input));
}

#[test]
#[ignore = "requires a GPU"]
fn manual_dehaze_skips_color_selection_and_retains_identical_brightness_calibration() {
    let gpu = Gpu::new().unwrap();
    let extent = [64, 48];
    let pixels: Vec<f32> = (0..extent[0] * extent[1])
        .flat_map(|i| {
            let v = 0.04 + (i % 64) as f32 / 256.;
            [v, v * 0.8, v * 0.6]
        })
        .collect();
    let input = gpu.upload("Calibration fixture", &pixels).unwrap();
    let params = Estimate::new(
        extent,
        test_basis(ColorSpace::Srgb),
        1.,
        false,
        [0, 0, 64, 48],
    );
    let automatic = estimate(&gpu, &input, params, &mut Profile::default()).unwrap();
    let manual_params = params.with_color_estimation(false);
    assert!(!manual_params.estimates_color());
    let manual = estimate(&gpu, &input, manual_params, &mut Profile::default()).unwrap();
    let automatic = gpu.readback(&automatic).unwrap();
    let manual = gpu.readback(&manual).unwrap();
    let manual_values: &[f32] = bytemuck::cast_slice(&manual);
    assert_eq!(&manual_values[..4], &[0.; 4]);
    assert_eq!(&manual[16..], &automatic[16..]);
}

#[test]
#[ignore = "requires a GPU"]
fn shadow_protector_eases_channel_loss_with_one_rgb_weight_and_preserves_bypasses() {
    let gpu = Gpu::new().unwrap();
    let samples = [
        [0.015_f32, 0.025, 0.01],
        [0.08, 0.1, 0.12],
        [0.4, 0.5, 0.45],
        [1.5, 1.5, 1.5],
        [-0.01, -0.01, -0.01],
        [0., 0., 0.],
    ];
    for white in [1., 4096., 16384.] {
        let values: Vec<f32> = samples.into_iter().flatten().map(|v| v * white).collect();
        let input = gpu.upload("shadow signal samples", &values).unwrap();
        let analysis = gpu
            .upload(
                "haze shadow calibration",
                &[0.08_f32, 0.1, 0.12, 1., 0.025, 1., 0., 0.],
            )
            .unwrap();
        for amount in [0., 12., 100., -50.] {
            let controls = Controls {
                amount,
                shadow: None,
            };
            let candidate = match amount {
                0. => samples.into_iter().flatten().collect::<Vec<_>>(),
                12. => vec![
                    0.009064734,
                    0.01913667,
                    0.004066076,
                    0.072153606,
                    0.09165459,
                    0.11115557,
                    0.39583826,
                    0.4962084,
                    0.44364896,
                    1.5037729,
                    1.5037729,
                    1.5037729,
                    -0.01,
                    -0.01,
                    -0.01,
                    0.,
                    0.,
                    0.,
                ],
                100. => vec![
                    0.00094662316,
                    0.0043825167,
                    0.00028047775,
                    0.028925896,
                    0.039444413,
                    0.04996292,
                    0.38184318,
                    0.4773641,
                    0.39414397,
                    1.5262438,
                    1.5262438,
                    1.5262438,
                    -0.01,
                    -0.01,
                    -0.01,
                    0.,
                    0.,
                    0.,
                ],
                -50. => vec![
                    0.02,
                    0.030769229,
                    0.018461537,
                    0.08,
                    0.1,
                    0.12,
                    0.24,
                    0.3,
                    0.285,
                    0.79859596,
                    0.79859596,
                    0.79859596,
                    -0.01,
                    -0.01,
                    -0.01,
                    0.,
                    0.,
                    0.,
                ],
                _ => unreachable!(),
            };
            let raw: Vec<_> = candidate.iter().map(|v| v * white).collect();
            let guarded = apply(
                &gpu,
                &input,
                &analysis,
                Params::new([6, 1], test_basis(ColorSpace::Srgb), white, false, controls),
                [6, 1],
                &mut Profile::default(),
            )
            .unwrap();
            let guarded = floats(&gpu, &guarded);
            if amount <= 0. {
                for (a, b) in raw.iter().zip(&guarded) {
                    assert!((a - b).abs() < 0.000003 * white);
                }
                continue;
            }
            for (i, pixel) in samples.into_iter().enumerate() {
                let target: [f32; 3] = std::array::from_fn(|c| raw[i * 3 + c] / white);
                let risk = (0..3)
                    .map(|c| (pixel[c] - target[c]).max(0.) / pixel[c].abs().max(0.000001))
                    .fold(0., f32::max);
                let weight = 1. / (1. + risk);
                for c in 0..3 {
                    let expected = pixel[c] + (target[c] - pixel[c]) * weight;
                    let actual = guarded[i * 3 + c] / white;
                    assert!(actual.is_finite());
                    assert!((actual-expected).abs()<0.000003,"white {white}, amount {amount}, pixel {i}, channel {c}: {actual} vs {expected}");
                    if pixel[c] >= 0. {
                        assert!(actual >= -0.000002);
                    }
                }
            }
            if amount == 100. {
                assert_ne!(raw, guarded);
            }
        }
        let mono = apply(
            &gpu,
            &input,
            &analysis,
            Params::new(
                [6, 1],
                test_basis(ColorSpace::Srgb),
                white,
                true,
                Controls {
                    amount: 100.,
                    shadow: None,
                },
            ),
            [6, 1],
            &mut Profile::default(),
        )
        .unwrap();
        for pixel in floats(&gpu, &mono).as_chunks::<3>().0 {
            assert_eq!(pixel[0], pixel[1]);
            assert_eq!(pixel[1], pixel[2]);
        }
        assert_eq!(floats(&gpu, &input), values);
    }
}

#[test]
#[ignore = "requires a GPU"]
fn shadow_protector_stays_continuous_when_an_input_channel_crosses_zero() {
    let gpu = Gpu::new().unwrap();
    let basis = Basis::new(
        ColorSpace::AdobeRgb,
        [1.4, -0.2, -0.2, -0.1, 1.2, -0.1, -0.2, -0.3, 1.5],
        [0.299, 0.587, 0.114],
    )
    .unwrap();
    let values = [
        0.02_f32, 0.03, -0.0000002, 0.02, 0.03, 0., 0.02, 0.03, 0.0000002,
    ];
    let input = gpu.upload("signed-zero shadow boundary", &values).unwrap();
    let analysis = gpu
        .upload(
            "signed-zero haze",
            &[0.1_f32, 0.2, 0.3, 1., 0.025, 1., 0., 0.],
        )
        .unwrap();
    let controls = Controls {
        amount: 100.,
        shadow: None,
    };
    let protected = apply(
        &gpu,
        &input,
        &analysis,
        Params::new([3, 1], basis, 1., false, controls),
        [3, 1],
        &mut Profile::default(),
    )
    .unwrap();
    let protected = floats(&gpu, &protected);
    // This camera basis previously proposed blue below -0.0019 at zero input.
    assert!(protected[5].abs() < 0.000002);
    for c in 0..3 {
        assert!(
            (protected[c] - protected[c + 6]).abs() < 0.000003,
            "protection jumped at signed zero: {protected:?}"
        );
    }
}

#[test]
#[ignore = "requires a GPU"]
fn partitioned_estimate_matches_serial_for_ties_empty_and_saturation() {
    let gpu = Gpu::new().unwrap();
    // Tiny overviews leave some partitions empty. The uneven overview checks
    // partition boundaries; 512 squared reaches the largest analysis and K.
    for extent in [[1, 1], [7, 3], [257, 193], [512, 512]] {
        for mode in 0..5 {
            let values: Vec<f32> = (0..extent[0] * extent[1])
                .flat_map(|i| match mode {
                    0 => [0.; 3],
                    1 => [0.08, 0.1, 0.12],
                    2 => [1.5, -0.2, 0.9],
                    3 => [0.7, 0.8, 0.9],
                    _ => {
                        let v = ((i * 73 + i / 17) % 256) as f32 / 1024.;
                        [v, v * 0.8, v * 0.6]
                    }
                })
                .collect();
            let input = gpu.upload("partition trial", &values).unwrap();
            for auto in [false, true] {
                for mono in [false, true] {
                    let basis = if mono {
                        Basis::monochrome()
                    } else {
                        Basis::new(
                            ColorSpace::Srgb,
                            [2., -1., 0., -1., 2., 0., 0., -1., 2.],
                            [0.299, 0.587, 0.114],
                        )
                        .unwrap()
                    };
                    let mut p =
                        Estimate::new(extent, basis, 1., mono, [0, 0, extent[0], extent[1]])
                            .with_color_estimation(auto);
                    // Patch estimation permits bright candidates that Auto excludes.
                    if mode == 3 {
                        p.selection[1] = 65535;
                    }
                    let a = estimate_impl(&gpu, &input, p, &mut Profile::default(), false).unwrap();
                    let b = estimate_impl(&gpu, &input, p, &mut Profile::default(), true).unwrap();
                    assert_eq!(
                        gpu.readback(&a).unwrap(),
                        gpu.readback(&b).unwrap(),
                        "extent {extent:?}, mode {mode}, auto {auto}, mono {mono}"
                    );
                }
            }
        }
    }
}
