//! Verifies tonal weights, restrained perceptual saturation, gamut bounds and signed GPU inputs.

use super::*;

#[test]
fn grading_values_round_trip_and_validate_even_when_inactive() {
    let mut c = Controls::default();
    c.bands[1].saturation = 0.7;
    c.contrast = -0.3;
    let decoded: Controls = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
    assert_eq!(decoded, c);
    decoded.validate().unwrap();
    assert!(serde_json::from_str::<Controls>("{}").unwrap() == Controls::default());
    c.saturation = 2.;
    c.bands[0].saturation = 2.;
    c.validate().unwrap();
    for value in [f32::NAN, f32::INFINITY, 2.1, -1.1] {
        c.bands[0].saturation = value;
        assert!(c.validate().is_err());
        c.bands[0].saturation = 0.;
    }
    for value in [f32::NAN, f32::INFINITY, 1.1, -1.1] {
        c.contrast = value;
        assert!(c.validate().is_err());
    }
}

fn samples(gpu: &Gpu, image: &ReadGuard) -> Vec<f32> {
    bytemuck::cast_slice::<u8, f32>(&gpu.readback(image).unwrap()).to_vec()
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_perceptual_grading_preserves_neutral_and_normalized_overlaps() {
    let gpu = Gpu::new().unwrap();
    let extent = [129, 1];
    let mut values: Vec<f32> = (0..128)
        .flat_map(|index| {
            let value = 0.18 * 2_f32.powf(index as f32 / 127. * 12. - 6.);
            [value; 3]
        })
        .collect();
    values.extend([-0.03, 0.1, 2.]);
    let input = gpu
        .upload("perceptual ramp and signed headroom", &values)
        .unwrap();
    let render = |c: Controls, mask: Option<u32>| {
        let mut p = Params::new(extent, ColorSpace::Srgb, 1., 0.18_f32.log2(), c, false);
        if let Some(band) = mask {
            p = p.with_mask(band);
        }
        samples(
            &gpu,
            &apply(&gpu, &input, p, extent, &mut Profile::default()).unwrap(),
        )
    };
    assert_eq!(render(Controls::default(), None), values);
    let masks: Vec<_> = (0..3)
        .map(|band| render(Controls::default(), Some(band)))
        .collect();
    for pixel in 0..129 {
        let sum: f32 = masks.iter().map(|mask| mask[pixel]).sum();
        assert!((sum - 1.).abs() < 1e-6);
        assert!(masks.iter().all(|mask| (0. ..=1.).contains(&mask[pixel])));
    }
    assert!(masks[0][0] > 0.99 && masks[2][127] > 0.99);
    assert_eq!(samples(&gpu, &input), values);
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_perceptual_monochrome_matches_color_editor_and_keeps_signed_values_finite() {
    let gpu = Gpu::new().unwrap();
    let extent = [16, 8];
    let values = [0.3_f32, 0.12, 0.05].repeat(128);
    let input = gpu.upload("flat colorful patch", &values).unwrap();
    let render = |source: &ReadGuard, controls: Controls| {
        apply(
            &gpu,
            source,
            Params::new(
                extent,
                ColorSpace::Srgb,
                1.,
                0.18_f32.log2(),
                controls,
                false,
            ),
            extent,
            &mut Profile::default(),
        )
        .unwrap()
    };
    for saturation in [-1.] {
        let c = Controls {
            saturation,
            ..Default::default()
        };
        let graded = samples(&gpu, &render(&input, c));
        let mut editor = super::super::color_editor::Controls::default();
        editor.master.saturation = saturation;
        let params = super::super::color_editor::Params::new(
            extent,
            editor,
            ColorSpace::Srgb,
            1.,
            1.,
            1.,
            false,
        );
        let edited = super::super::color_editor::apply(
            &gpu,
            &input,
            &input,
            params,
            extent,
            &mut Profile::default(),
        )
        .unwrap();
        for (a, b) in graded.iter().zip(samples(&gpu, &edited)) {
            assert!((a - b).abs() < 2e-5, "grade {a} versus editor {b}");
        }
    }
    let signed = gpu
        .upload("signed colorful samples", &[-0.1_f32, 0.2, 1.8].repeat(128))
        .unwrap();
    let result = render(
        &signed,
        Controls {
            saturation: 0.8,
            contrast: -0.6,
            ..Default::default()
        },
    );
    assert!(samples(&gpu, &result).iter().all(|v| v.is_finite()));
}

fn grade(gpu: &Gpu, values: &[f32], space: ColorSpace, white: f32, c: Controls) -> Vec<f32> {
    grade_with_protection(gpu, values, space, white, c, true)
}

fn grade_with_protection(
    gpu: &Gpu,
    values: &[f32],
    space: ColorSpace,
    white: f32,
    c: Controls,
    prevent: bool,
) -> Vec<f32> {
    let extent = [values.len() as u32 / 3, 1];
    let source = gpu.upload("saturation patches", values).unwrap();
    samples(
        gpu,
        &apply(
            gpu,
            &source,
            Params::new(extent, space, white, 0.18_f32.log2(), c, false)
                .with_clipping_protection(prevent),
            extent,
            &mut Profile::default(),
        )
        .unwrap(),
    )
}

// Independent f64 reference measures perceptual hue, lightness and chroma.
fn lab(rgb: &[f32], space: ColorSpace) -> [f64; 3] {
    let matrix = TiffColorSpace::Srgb.transform(space);
    let rgb: [f64; 3] = std::array::from_fn(|i| {
        (0..3)
            .map(|j| f64::from(matrix[i][j]) * f64::from(rgb[j]))
            .sum()
    });
    let l = (0.4122214708 * rgb[0] + 0.5363325363 * rgb[1] + 0.0514459929 * rgb[2]).cbrt();
    let m = (0.2119034982 * rgb[0] + 0.6806995451 * rgb[1] + 0.1073969566 * rgb[2]).cbrt();
    let s = (0.0883024619 * rgb[0] + 0.2817188376 * rgb[1] + 0.6299787005 * rgb[2]).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_perceptual_boost_stays_in_working_gamut_and_preserves_hue_and_lightness() {
    let gpu = Gpu::new().unwrap();
    let mut patches = Vec::new();
    for r in [0., 0.001, 0.01, 0.1, 0.4, 0.8, 1.] {
        for g in [0., 0.001, 0.01, 0.1, 0.4, 0.8, 1.] {
            for b in [0., 0.001, 0.01, 0.1, 0.4, 0.8, 1.] {
                patches.extend([r, g, b]);
            }
        }
    }
    for space in [
        ColorSpace::Srgb,
        ColorSpace::AdobeRgb,
        ColorSpace::ProPhotoRgb,
    ] {
        for (amount, contrast) in [
            (0.05_f32, 0.),
            (0.5, 0.),
            (1., 0.),
            (2., 0.),
            (4., 1.),
            (4., -1.),
            (0., 1.),
            (0., -1.),
            (-0.5, 1.),
            (-1., -1.),
        ] {
            let c = Controls {
                saturation: amount.min(2.),
                contrast,
                bands: [Band {
                    saturation: if amount > 2. { 2. } else { 0. },
                }; 3],
            };
            let result = grade(&gpu, &patches, space, 1., c);
            for (input, output) in patches
                .as_chunks::<3>()
                .0
                .iter()
                .zip(result.as_chunks::<3>().0.iter())
            {
                assert!(
                    output
                        .iter()
                        .all(|v| v.is_finite() && (-2e-5..=1.00002).contains(v)),
                    "{space:?} amount {amount}: {input:?} -> {output:?}"
                );
                let before = lab(input, space);
                let after = lab(output, space);
                assert!(
                    (after[0] - before[0]).abs() < 2e-5,
                    "lightness {space:?}: {input:?} -> {output:?}"
                );
                let c0 = before[1].hypot(before[2]);
                let c1 = after[1].hypot(after[2]);
                if amount > 0. && contrast == 0. {
                    assert!(
                        c1 + 2e-5 >= c0,
                        "boost reduced chroma {input:?} -> {output:?}"
                    );
                }
                assert!(
                    (before[1] * after[2] - before[2] * after[1]).abs() < 2e-5,
                    "changed hue {space:?}: {input:?} -> {output:?}"
                );
                if input[0] == input[1] && input[1] == input[2] {
                    assert_eq!(input, output, "gray changed in {space:?}");
                }
            }
            let codes: Vec<_> = patches.iter().map(|v| v * 4095.).collect();
            let scaled = grade(&gpu, &codes, space, 4095., c);
            for (index, (actual, expected)) in scaled.iter().zip(&result).enumerate() {
                assert!(
                    (actual / 4095. - expected).abs() < 2e-5,
                    "code-scale dependence {space:?} amount {amount}, pixel {:?}: {} versus {expected}",
                    &patches[index / 3 * 3..index / 3 * 3 + 3], actual / 4095.
                );
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_perceptual_boost_favors_muted_colors_and_combines_controls_once() {
    let gpu = Gpu::new().unwrap();
    let space = ColorSpace::Srgb;
    let patches = [0.3, 0.2, 0.2, 0.6, 0.01, 0.01, 0.6, 0., 0.];
    let mut previous = [0.; 3];
    for amount in [0.01, 0.2, 0.5, 1., 1.5, 2.] {
        let global = Controls {
            saturation: amount,
            ..Default::default()
        };
        let output = grade(&gpu, &patches, space, 1., global);
        let mut gains = Vec::new();
        for (index, (input, result)) in patches
            .as_chunks::<3>()
            .0
            .iter()
            .zip(output.as_chunks::<3>().0.iter())
            .enumerate()
        {
            let before = lab(input, space);
            let after = lab(result, space);
            let chroma = after[1].hypot(after[2]);
            assert!(
                chroma + 1e-6 >= previous[index],
                "chroma must increase monotonically"
            );
            previous[index] = chroma;
            gains.push(chroma / before[1].hypot(before[2]));
        }
        assert!(
            gains[0] > gains[1] && gains[1] >= gains[2],
            "muted/vivid gains: {gains:?}"
        );
        assert!(
            (gains[2] - 1.).abs() < 1e-5,
            "gamut-bound color should remain unchanged"
        );
        for distributed in [
            Controls {
                bands: [Band { saturation: amount }; 3],
                ..Default::default()
            },
            Controls {
                saturation: amount * 0.5,
                bands: [Band {
                    saturation: amount * 0.5,
                }; 3],
                contrast: 0.,
            },
        ] {
            let result = grade(&gpu, &patches, space, 1., distributed);
            for (actual, expected) in result.iter().zip(&output) {
                assert!(
                    (actual - expected).abs() < 2e-6,
                    "combined adjustment differs"
                );
            }
        }
    }
    let extended = [-0.03, 0.1, 2., 2., 0.2, 0.1, -0.2, -0.1, -0.3, 2., 2., 2.];
    let output = grade(
        &gpu,
        &extended,
        space,
        1.,
        Controls {
            saturation: 1.,
            ..Default::default()
        },
    );
    for (input, result) in extended
        .as_chunks::<3>()
        .0
        .iter()
        .zip(output.as_chunks::<3>().0.iter())
    {
        let minimum = input.iter().copied().fold(0., f32::min);
        let maximum = input.iter().copied().fold(1., f32::max);
        assert!(
            result
                .iter()
                .all(|v| v.is_finite() && *v >= minimum - 1e-5 && *v <= maximum + 1e-5),
            "lost signed/headroom envelope {input:?} -> {result:?}"
        );
    }
    assert!(
        output[0] < 0. && output[2] > 1.,
        "signed values and headroom were clipped"
    );
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_saturation_contrast_spreads_or_compresses_colorfulness_without_reviving_gray() {
    let gpu = Gpu::new().unwrap();
    let patches = [0.3, 0.2, 0.2, 0.6, 0.01, 0.01, 0.2, 0.2, 0.2];
    let chroma = |v: &[f32]| {
        let v = lab(v, ColorSpace::Srgb);
        v[1].hypot(v[2])
    };
    let c0 = chroma(&patches[..3]);
    let c1 = chroma(&patches[3..6]);
    for contrast in [-1., -0.5, 0.5, 1.] {
        let output = grade(
            &gpu,
            &patches,
            ColorSpace::Srgb,
            1.,
            Controls {
                contrast,
                ..Default::default()
            },
        );
        if contrast > 0. {
            assert!(
                chroma(&output[..3]) < c0,
                "muted colors must become quieter"
            );
            assert!(
                chroma(&output[3..6]) > c1,
                "vivid colors must become stronger"
            );
        } else {
            assert!(chroma(&output[..3]) > c0, "muted colors must rise");
            assert!(chroma(&output[3..6]) < c1, "vivid colors must soften");
        }
        assert_eq!(&output[6..9], &patches[6..9]);
        for (input, result) in patches
            .as_chunks::<3>()
            .0
            .iter()
            .zip(output.as_chunks::<3>().0)
        {
            assert!(
                (lab(input, ColorSpace::Srgb)[0] - lab(result, ColorSpace::Srgb)[0]).abs() < 2e-5
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_negative_saturation_reduces_vivid_colors_preferentially_and_reaches_gray() {
    let gpu = Gpu::new().unwrap();
    let patches = [0.3, 0.2, 0.2, 0.6, 0.01, 0.01, 0.6, 0., 0.];
    let mut previous = [1.; 3];
    for saturation in [-0.1, -0.5, -0.9, -1.] {
        let output = grade(
            &gpu,
            &patches,
            ColorSpace::Srgb,
            1.,
            Controls {
                saturation,
                ..Default::default()
            },
        );
        let mut gains = Vec::new();
        for (index, (input, result)) in patches
            .as_chunks::<3>()
            .0
            .iter()
            .zip(output.as_chunks::<3>().0)
            .enumerate()
        {
            let a = lab(input, ColorSpace::Srgb);
            let b = lab(result, ColorSpace::Srgb);
            let gain = b[1].hypot(b[2]) / a[1].hypot(a[2]);
            assert!(
                gain <= previous[index] + 1e-6,
                "desaturation strength reversed"
            );
            assert!((a[0] - b[0]).abs() < 2e-5, "lightness changed");
            previous[index] = gain;
            gains.push(gain);
        }
        if saturation > -1. {
            assert!(
                gains[0] > gains[1] && gains[1] >= gains[2] - 1e-6,
                "muted/vivid gains: {gains:?}"
            );
        } else {
            assert!(
                gains.iter().all(|g| *g < 1e-5),
                "did not reach monochrome: {gains:?}"
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_two_slider_curve_keeps_colorfulness_order_and_bounds_neutral_amplification() {
    let gpu = Gpu::new().unwrap();
    let mut patches = Vec::new();
    // Independent inverse Oklab reference creates one fixed-lightness red ray.
    // Vary only chroma so brightness or hue cannot conceal a curve reversal.
    for step in 0..129 {
        let a = 0.18 * f64::from(step) / 128.;
        let l = (0.55 + 0.3963377774 * a).powi(3);
        let m = (0.55 - 0.1055613458 * a).powi(3);
        let s = (0.55 - 0.0894841775 * a).powi(3);
        patches.extend([
            (4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s) as f32,
            (-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s) as f32,
            (-0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s) as f32,
        ]);
    }
    for saturation in [-1., -0.5, 0., 0.5, 1.] {
        for contrast in [-1., -0.5, 0., 0.5, 1.] {
            let result = grade(
                &gpu,
                &patches,
                ColorSpace::Srgb,
                1.,
                Controls {
                    saturation,
                    contrast,
                    ..Default::default()
                },
            );
            let mut previous = 0.;
            for pixel in result.as_chunks::<3>().0 {
                let v = lab(pixel, ColorSpace::Srgb);
                let chroma = v[1].hypot(v[2]);
                assert!(
                    chroma + 1e-6 >= previous,
                    "colorfulness order reversed at {saturation}/{contrast}"
                );
                assert!((v[0] - 0.55).abs() < 2e-5, "lightness changed");
                previous = chroma;
            }
            let quiet = lab(&result[3..6], ColorSpace::Srgb);
            let quiet_chroma = quiet[1].hypot(quiet[2]);
            assert!(
                quiet_chroma <= 0.18 / 128. * 4.01,
                "unbounded amplification near neutral at {saturation}/{contrast}"
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_saturation_policy_is_shared_by_global_basic_and_advanced_ranges() {
    use super::super::{advanced_color, color_editor};
    let gpu = Gpu::new().unwrap();
    let patches = [
        0.3, 0.2, 0.2, 0.7, 0.05, 0.02, 0.6, 0., 0., 0.8, 0.6, -0.01, -0.03, 0.1, 2., 0.2, 0.2,
        0.2, 0., 0., 0.,
    ];
    for space in [
        ColorSpace::Srgb,
        ColorSpace::AdobeRgb,
        ColorSpace::ProPhotoRgb,
    ] {
        for white in [1., 4095.] {
            let values: Vec<_> = patches.iter().map(|v| v * white).collect();
            let extent = [values.len() as u32 / 3, 1];
            let input = gpu.upload("shared saturation patches", &values).unwrap();
            let guide = gpu
                .upload(
                    "full selection independent of source",
                    &[white, 0., 0.].repeat(extent[0] as usize),
                )
                .unwrap();
            for prevent in [true, false] {
                for amount in [-1., -0.5, 0.5, 1., 2.] {
                    let expected = grade_with_protection(
                        &gpu,
                        &values,
                        space,
                        white,
                        Controls {
                            saturation: amount,
                            ..Default::default()
                        },
                        prevent,
                    );
                    let mut basic = color_editor::Controls::default();
                    basic.master.saturation = amount;
                    let edited = color_editor::apply(
                        &gpu,
                        &input,
                        &guide,
                        color_editor::Params::new(extent, basic, space, white, white, 1., false)
                            .with_clipping_protection(prevent),
                        extent,
                        &mut Profile::default(),
                    )
                    .unwrap();
                    for (a, b) in samples(&gpu, &edited).iter().zip(&expected) {
                        assert!(
                            (a - b).abs() / white < 3e-5,
                            "Basic/global mismatch {space:?} {amount} {prevent}: {a} versus {b}"
                        );
                    }
                    let mut advanced = advanced_color::Controls::default();
                    advanced.push(advanced_color::Range {
                        width: 360.,
                        adjustment: color_editor::Adjustment {
                            saturation: amount,
                            ..Default::default()
                        },
                        ..Default::default()
                    });
                    for exposure in [-2., 0., 2.] {
                        let edited = advanced_color::apply(
                            &gpu,
                            &input,
                            &guide,
                            advanced_color::Params::new(
                                extent, advanced, space, white, white, exposure, false, None,
                            )
                            .with_clipping_protection(prevent),
                            extent,
                            &mut Profile::default(),
                        )
                        .unwrap();
                        for (a, b) in samples(&gpu, &edited).iter().zip(&expected) {
                            assert!((a - b).abs() / white < 3e-5, "Advanced/global mismatch {space:?} {amount} {prevent} EV{exposure}: {a} versus {b}");
                        }
                    }
                }
            }
            let protected = grade_with_protection(
                &gpu,
                &values,
                space,
                white,
                Controls {
                    saturation: 2.,
                    ..Default::default()
                },
                true,
            );
            let unlocked = grade_with_protection(
                &gpu,
                &values,
                space,
                white,
                Controls {
                    saturation: 2.,
                    ..Default::default()
                },
                false,
            );
            let before = lab(&patches[6..9], space);
            let after = lab(
                &unlocked[6..9].iter().map(|v| v / white).collect::<Vec<_>>(),
                space,
            );
            assert!(
                after[1].hypot(after[2]) > before[1].hypot(before[2]) * 1.2,
                "gamut-edge colors stayed locked"
            );
            assert!(unlocked.iter().all(|v| v.is_finite()));
            assert_eq!(&unlocked[15..], &values[15..], "grays changed");
            for amount in [-1., -0.5, 0.] {
                let c = Controls {
                    saturation: amount,
                    ..Default::default()
                };
                assert_eq!(
                    grade_with_protection(&gpu, &values, space, white, c, true),
                    grade_with_protection(&gpu, &values, space, white, c, false),
                    "policy changed desaturation/neutral"
                );
            }
            // Repeated protected saturation shares the same ceiling rather than granting a new allowance.
            let repeated = grade_with_protection(
                &gpu,
                &protected,
                space,
                white,
                Controls {
                    saturation: 2.,
                    ..Default::default()
                },
                true,
            );
            for (original, output) in values
                .as_chunks::<3>()
                .0
                .iter()
                .zip(repeated.as_chunks::<3>().0.iter())
            {
                let low = original.iter().copied().fold(0., f32::min);
                let high = original.iter().copied().fold(white, f32::max);
                assert!(output
                    .iter()
                    .all(|v| *v >= low - white * 2e-5 && *v <= high + white * 2e-5));
            }
        }
    }
}

#[test]
fn balanced_control_coordinates_preserve_the_existing_processing_amounts() {
    for (control, amount) in [
        (-1., -1.),
        (-0.5, -0.5),
        (0., 0.),
        (0.25, 0.5),
        (0.5, 1.),
        (1., 2.),
    ] {
        assert_eq!(Controls::saturation_from_control(control), amount);
        assert_eq!(Controls::saturation_to_control(amount), control);
    }
    for amount in [-1., -0.1234567, 0., 0.007, 0.1234567, 1.2345678, 2.] {
        assert_eq!(
            Controls::saturation_from_control(Controls::saturation_to_control(amount)),
            amount
        );
    }
    assert_eq!(*Controls::SATURATION_CONTROL_RANGE.start(), -1.);
    assert_eq!(*Controls::SATURATION_CONTROL_RANGE.end(), 1.);
}
