//! Checks shared finishing contracts and independently evaluated GPU samples.
use super::*;
#[test]
fn defaults_are_reference_and_neutral() {
    let c = Controls::default();
    assert_eq!(c.tone_method, Method::Reference);
    assert!(!c.sensitivity_active());
    assert!(!c.sharpening.is_active());
    c.validate().unwrap();
}
#[test]
fn invalid_values_are_rejected() {
    for value in [f32::NAN, f32::INFINITY, -1.1, 1.1] {
        let c = Controls {
            local_contrast: value,
            ..Default::default()
        };
        assert!(c.validate().is_err());
    }
    let mut c = Controls::default();
    c.monochrome_sensitivity[3] = f32::NAN;
    assert!(c.validate().is_err());
}

fn samples(gpu: &crate::gpu::Gpu, buffer: &crate::gpu::ReadGuard) -> Vec<f32> {
    bytemuck::cast_slice::<u8, f32>(&gpu.readback(buffer).unwrap()).to_vec()
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_photographic_tone_preserves_neutral_headroom_and_restrains_saturation() {
    use crate::{gpu::Gpu, profile::Profile};
    let gpu = Gpu::new().unwrap();
    let extent = [6, 1];
    let values = [
        -0.1, 0.3, 1.4, 0., 0., 0., 0.2, 0.2, 0.2, 0.3, 0.25, 0.2, 0.8, 0.1, 0.05, 2., 2., 2.,
    ];
    let input = gpu.upload("tone test", &values).unwrap();
    let mut p = Params::new(extent, ColorSpace::Srgb);
    p.values = [0., 1., 0., 0.];
    let neutral = tone::apply(&gpu, &input, p, extent, &mut Profile::default()).unwrap();
    assert_eq!(samples(&gpu, &neutral), values);
    p.values[1] = 1.5;
    let output = tone::apply(&gpu, &input, p, extent, &mut Profile::default()).unwrap();
    let actual = samples(&gpu, &output);
    for (v, w) in values
        .as_chunks::<3>()
        .0
        .iter()
        .zip(actual.as_chunks::<3>().0)
    {
        let y: f32 = v.iter().zip(p.luma).map(|(v, c)| v * c).sum();
        let out: f32 = w.iter().zip(p.luma).map(|(v, c)| v * c).sum();
        assert!((y - out).abs() < 0.00001);
        let q =
            (1. - v.iter().copied().fold(f32::INFINITY, f32::min) / y.max(0.000001)).clamp(0., 1.);
        let gain = 1.8 / (1. + 0.8 * q);
        for c in 0..3 {
            assert!((w[c] - (y + (v[c] - y) * gain)).abs() < 0.00001);
        }
    }
    p.values = [0.7, 1., 0., 0.];
    let output = tone::apply(&gpu, &input, p, extent, &mut Profile::default()).unwrap();
    assert!(samples(&gpu, &output).iter().all(|v| v.is_finite()));
    assert_eq!(samples(&gpu, &input), values);
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_monochrome_sensitivity_targets_original_hues_and_protects_gray() {
    use crate::{gpu::Gpu, profile::Profile};
    let gpu = Gpu::new().unwrap();
    let extent = [8, 1];
    let colors: [f32; 24] = [
        1., 0., 0., 1., 1., 0., 0., 1., 0., 0., 1., 1., 0., 0., 1., 1., 0., 1., 0.4, 0.4, 0.4, 0.,
        0., 0.,
    ];
    let values = [0.2_f32; 24];
    let source = gpu.upload("mono", &values).unwrap();
    let guide = gpu.upload("original colors", &colors).unwrap();
    for hue in 0..6 {
        let mut p = Params::new(extent, ColorSpace::Srgb);
        p.hues[hue / 4][hue % 4] = 0.5;
        let output =
            sensitivity::apply(&gpu, &source, &guide, p, extent, &mut Profile::default()).unwrap();
        let actual = samples(&gpu, &output);
        for i in 0..8 {
            let expected = if i == hue { 0.4 } else { 0.2 };
            for c in 0..3 {
                assert!(
                    (actual[i * 3 + c] - expected).abs() < 0.00001,
                    "hue {hue} pixel {i}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_shared_spatial_operations_preserve_flats_and_keep_sources_immutable() {
    use crate::{gpu::Gpu, profile::Profile};
    let gpu = Gpu::new().unwrap();
    let extent = [48, 32];
    let values_for_flat: Vec<f32> = (0..extent[0] * extent[1])
        .flat_map(|_| [0.3, 0.2, 0.1])
        .collect();
    let source = gpu.upload("flat", &values_for_flat).unwrap();
    type Apply = fn(
        &Gpu,
        &crate::gpu::ReadGuard,
        Params,
        [u32; 2],
        &mut Profile,
    ) -> crate::source::Result<crate::gpu::ReadGuard>;
    for (label, values, apply, dispatches) in [
        ("sharpen", [1., 0.01, 0.5, 0.8], sharpen::apply as Apply, 4),
        (
            "local contrast",
            [1., 1., 0., 0.],
            local_contrast::apply as Apply,
            14,
        ),
    ] {
        let mut p = Params::new(extent, ColorSpace::Srgb);
        p.values = values;
        let mode = if gpu
            .device
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            crate::profile::Mode::Gpu
        } else {
            crate::profile::Mode::Wall
        };
        let before = gpu.activity();
        let mut profile = Profile::new(mode);
        let output = apply(&gpu, &source, p, extent, &mut profile).unwrap();
        assert_eq!(gpu.activity().since(before).dispatches, dispatches);
        if mode.gpu() {
            assert_eq!(profile.gpu.len() as u64, dispatches, "{label} timestamps");
        }
        for (a, b) in samples(&gpu, &output).iter().zip(&values_for_flat) {
            assert!((a - b).abs() < 0.00001, "{label} {a} != {b}");
        }
    }
    assert_eq!(samples(&gpu, &source), values_for_flat);
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_local_contrast_weights_fine_bands_above_broad_bands() {
    use crate::{gpu::Gpu, profile::Profile};
    let gpu = Gpu::new().unwrap();
    let extent = [1024, 1];
    let gains = super::super::wavelet_contrast::photographic_gains();
    let mut responses = Vec::new();
    for period in [4., 16., 64., 256.] {
        let omega = std::f32::consts::TAU / period;
        let amplitude = 0.02;
        let values: Vec<f32> = (0..extent[0])
            .flat_map(|x| [(-2. + amplitude * (omega * x as f32).cos()).exp2(); 3])
            .collect();
        let source = gpu.upload("log brightness sinusoid", &values).unwrap();
        // Independently evaluate the five-tap binomial filter's Fourier
        // response. Increasing steps successively isolate broader bands.
        let mut base_response = 1.;
        let mut response = 0.;
        for level in 0..6 {
            let filter_response = (omega * (1 << level) as f32 * 0.5).cos().powi(4);
            response += (gains[5 - level] - 1.) * base_response * (1. - filter_response);
            base_response *= filter_response;
        }
        for amount in [-0.7, 0.7] {
            let mut p = Params::new(extent, ColorSpace::Srgb);
            p.values = [amount, 1., 0., 0.];
            let output =
                local_contrast::apply(&gpu, &source, p, extent, &mut Profile::default()).unwrap();
            let actual = samples(&gpu, &output);
            for x in 256..768 {
                let phase = (omega * x as f32).cos();
                let expected = (-2. + amplitude * phase * (1. + amount * response)).exp2();
                assert!(
                    (actual[x * 3] - expected).abs() < 0.000002,
                    "period {period}, amount {amount}, pixel {x}: {} != {expected}",
                    actual[x * 3]
                );
            }
        }
        responses.push(response);
    }
    assert!(responses.windows(2).all(|r| r[0] > r[1]), "{responses:?}");
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_contrast_extremes_are_monotone_and_preserve_black_and_white() {
    use crate::{gpu::Gpu, profile::Profile};
    let gpu = Gpu::new().unwrap();
    let extent = [1024, 1];
    let data: Vec<f32> = (0..1024).flat_map(|i| [i as f32 / 1023.; 3]).collect();
    let input = gpu.upload("grayscale ramp", &data).unwrap();
    for contrast in [-1., -0.5, 0.5, 1.] {
        let mut p = Params::new(extent, ColorSpace::Srgb);
        p.values = [contrast, 1., 0., 0.];
        let output = tone::apply(&gpu, &input, p, extent, &mut Profile::default()).unwrap();
        let values = samples(&gpu, &output);
        assert_eq!(values[0], 0.);
        assert!((values[values.len() - 1] - 1.).abs() < 0.000001);
        for window in values.windows(6).step_by(3) {
            assert!(
                window[0] <= window[3],
                "contrast {contrast}: {} > {}",
                window[0],
                window[3]
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_sharpening_threshold_and_halo_controls_have_independent_effects() {
    use crate::{gpu::Gpu, profile::Profile};
    let gpu = Gpu::new().unwrap();
    let extent = [48, 24];
    let values: Vec<f32> = (0..extent[1])
        .flat_map(|y| {
            (0..extent[0]).flat_map(move |x| {
                [if (22..26).contains(&x) && (10..14).contains(&y) {
                    0.5
                } else {
                    0.25
                }; 3]
            })
        })
        .collect();
    let input = gpu.upload("sharp edge patch", &values).unwrap();
    let mut p = Params::new(extent, ColorSpace::Srgb);
    p.values = [2., 0., 0., 1.];
    let output = sharpen::apply(&gpu, &input, p, extent, &mut Profile::default()).unwrap();
    let unbounded = samples(&gpu, &output);
    assert!(unbounded.iter().any(|v| *v > 0.5));
    assert!(unbounded.iter().any(|v| *v < 0.25));
    p.values[2] = 1.;
    let output = sharpen::apply(&gpu, &input, p, extent, &mut Profile::default()).unwrap();
    assert!(samples(&gpu, &output)
        .iter()
        .all(|v| (0.249999..=0.500001).contains(v)));
    p.values[1] = 0.2;
    let output = sharpen::apply(&gpu, &input, p, extent, &mut Profile::default()).unwrap();
    for (a, b) in samples(&gpu, &output).iter().zip(&values) {
        assert!((a - b).abs() < 0.000001);
    }
}
