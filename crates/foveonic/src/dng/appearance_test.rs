//! Checks DNG appearance fitting, highlight headroom and original-photo approximations.

use super::*;
#[test]
fn classic_sample_basis_round_trips_and_keeps_tone_in_profile() {
    use crate::{color_space::TransferFunction, nodes::finish::signed_response};
    let mapping = Matrix::diagonal([1. / 8192., 1. / 4096., 1. / 2048.]);
    let base = mapping.inverse().unwrap().gain(1. / 4096.);
    let response = signed_response::prepare(
        signed_response::Controls::default(),
        [1.; 3],
        signed_response::ToneAdjustments::default(),
        TransferFunction::Power(461. / 256.),
    )
    .unwrap();
    let expected = response.evaluate_linear([0.18; 3]);
    let prepared = Prepared::classic(ClassicAppearance {
        mapping: mapping.0,
        neutral: [0.5, 1., 1.],
        camera_to_linear: base.0,
        selected_from_base: Matrix::diagonal([1.; 3]).0,
        response,
    })
    .unwrap();
    let codes = [409.6, 819.2, 1228.8];
    let restored = prepared
        .camera_to_native
        .apply(prepared.mapping.apply(codes));
    for (actual, code) in restored.into_iter().zip(codes) {
        assert!((actual - code / 4096.).abs() < 1e-12);
    }
    assert_ne!(expected, [0.18; 3]);
    let profile = prepared.finish(2.).unwrap();
    assert_eq!(profile.baseline, 0.);
    assert_eq!(profile.curve.len(), (INTERVALS + 1) * 2);
    assert_eq!(
        profile.look.len(),
        DIMS.into_iter().product::<u32>() as usize * 3
    );
    assert!(profile.look.iter().all(|v| v.is_finite()));
}

#[test]
fn classic_neutral_storage_retains_editable_linear_signal() {
    use crate::{color_space::TransferFunction, nodes::finish::signed_response};
    let identity = Matrix::diagonal([1.; 3]);
    let response = signed_response::prepare(
        signed_response::Controls {
            exposure: 0.5,
            contrast: 0.2,
            ..Default::default()
        },
        [1.; 3],
        signed_response::ToneAdjustments::default(),
        TransferFunction::Power(461. / 256.),
    )
    .unwrap();
    let expected = response.evaluate_linear([0.18; 3]);
    let prepared = Prepared::classic(ClassicAppearance {
        // A rank-one storage transform is valid when the samples already
        // contain a monochrome mixture. Reader coordinates remain invertible.
        mapping: [0., 1. / 4096., 0., 0., 1. / 4096., 0., 0., 1. / 4096., 0.],
        neutral: [1.; 3],
        camera_to_linear: identity.0,
        selected_from_base: identity.0,
        response,
    })
    .unwrap();
    assert_eq!(prepared.mapping.apply([700., 737.28, 800.]), [0.18; 3]);
    let reader_signal = prepared.camera_to_romm.apply([0.18; 3]);
    for value in reader_signal {
        assert!((value - 0.18).abs() < 1e-6);
    }
    let profile = prepared.finish(1.).unwrap();
    let rendered = render_profile(&profile, reader_signal, true);
    for (actual, expected) in rendered.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 0.003, "{actual} vs {expected}");
    }
    let profile = prepared.finish(2.).unwrap();
    let reader_signal = prepared.camera_to_romm.apply([0.09; 3]);
    let rendered = render_profile(&profile, reader_signal, true);
    for (actual, expected) in rendered.into_iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 0.003,
            "range compensation: {actual} vs {expected}"
        );
    }
}

// Independent CPU application of the stored trilinear HSV look and the
// reader's hue-preserving common tone. This checks the packaged profile,
// rather than evaluating the fitting closure a second time.
fn render_profile(profile: &ProfileData, rgb: [f64; 3], use_look: bool) -> [f64; 3] {
    let mut rgb = rgb;
    if use_look {
        let [h, s, v] = rgb_to_hsv(rgb);
        let coordinates = [
            h * DIMS[0] as f64,
            s * (DIMS[1] - 1) as f64,
            encode(v).clamp(0., 1.) * (DIMS[2] - 1) as f64,
        ];
        let lower = coordinates.map(|x| x.floor() as usize);
        let fractions = std::array::from_fn::<_, 3, _>(|i| coordinates[i] - lower[i] as f64);
        let mut correction = [0.; 3];
        for dv in 0..2 {
            for ds in 0..2 {
                for dh in 0..2 {
                    let weight = [dh, ds, dv]
                        .into_iter()
                        .enumerate()
                        .map(|(i, d)| {
                            if d == 0 {
                                1. - fractions[i]
                            } else {
                                fractions[i]
                            }
                        })
                        .product::<f64>();
                    let hh = (lower[0] + dh) % DIMS[0] as usize;
                    let ss = (lower[1] + ds).min(DIMS[1] as usize - 1);
                    let vv = (lower[2] + dv).min(DIMS[2] as usize - 1);
                    let index = ((vv * DIMS[0] as usize + hh) * DIMS[1] as usize + ss) * 3;
                    for (c, value) in correction.iter_mut().enumerate() {
                        *value += weight * f64::from(profile.look[index + c]);
                    }
                }
            }
        }
        rgb = hsv_to_rgb(
            (h + correction[0] / 360.).rem_euclid(1.),
            (s * correction[1]).clamp(0., 1.),
            decode((encode(v) * correction[2]).clamp(0., 1.)),
        );
    }
    let low = rgb.into_iter().fold(f64::INFINITY, f64::min);
    let high = rgb.into_iter().fold(0., f64::max);
    let a = curve_at(&profile.curve, exposure_tone(low, profile.baseline));
    let b = curve_at(&profile.curve, exposure_tone(high, profile.baseline));
    if high - low < 1e-12 {
        [a; 3]
    } else {
        rgb.map(|v| a + (b - a) * (v - low) / (high - low))
    }
}

#[test]
fn classic_profile_interpolation_measures_color_and_neutral_residuals() {
    use crate::{color_space::TransferFunction, nodes::finish::signed_response};
    let response = signed_response::prepare(
        signed_response::Controls {
            exposure: 0.4,
            saturation: 0.3,
            contrast: -0.2,
            ..Default::default()
        },
        [1.; 3],
        signed_response::ToneAdjustments::default(),
        TransferFunction::Power(461. / 256.),
    )
    .unwrap();
    // Non-diagonal, neutral-preserving mode approximation.
    let selected = Matrix([1.08, -0.05, -0.03, -0.02, 1.04, -0.02, -0.04, -0.02, 1.06]);
    let target = |rgb| {
        response
            .evaluate_linear(selected.apply(rgb))
            .map(|v| v.clamp(0., 1.))
    };
    let profile = fit_profile(0., TonePlateau::FirstCrossing, target).unwrap();
    let (mut total, mut maximum, mut curve_only, mut count) = (0., 0_f64, 0., 0);
    let mut neutral_maximum = 0_f64;
    for value in [0.003, 0.015, 0.06, 0.18, 0.4, 0.8] {
        for saturation in [0., 0.1, 0.35, 0.7, 0.95] {
            for hue in 0..37 {
                let rgb = hsv_to_rgb((hue as f64 + 0.37) / 37., saturation, value);
                let expected = target(rgb);
                let actual = render_profile(&profile, rgb, true);
                let baseline = render_profile(&profile, rgb, false);
                for c in 0..3 {
                    let error = (actual[c] - expected[c]).abs();
                    total += error;
                    maximum = maximum.max(error);
                    curve_only += (baseline[c] - expected[c]).abs();
                    count += 1;
                    if saturation == 0. {
                        neutral_maximum = neutral_maximum.max(error);
                    }
                }
            }
        }
    }
    let mean = total / f64::from(count);
    eprintln!("Classic fitted profile: {count} components, linear ROMM mean={mean:.8}, max={maximum:.8}, neutral max={neutral_maximum:.8}; curve-only mean={:.8}", curve_only / f64::from(count));
    assert!(
        total < curve_only,
        "residual color look must improve the common curve"
    );
    assert!(maximum.is_finite() && neutral_maximum.is_finite());
}

#[test]
fn profile_curve_compensation_retains_highlights_before_reader_clipping() {
    use crate::{color_space::TransferFunction, nodes::finish::signed_response};
    let response = signed_response::prepare(
        signed_response::Controls {
            exposure: -2.,
            ..Default::default()
        },
        [1.; 3],
        signed_response::ToneAdjustments::default(),
        TransferFunction::Power(461. / 256.),
    )
    .unwrap();
    let profile_curve = fit_profile(0., TonePlateau::FirstCrossing, |rgb| {
        response.evaluate_linear(rgb.map(|v| v * 4.))
    })
    .unwrap();
    let baseline = fit_profile(2., TonePlateau::WhiteEndpoint, |rgb| {
        response.evaluate_linear(rgb)
    })
    .unwrap();
    let expected = response.evaluate_linear([2.; 3]);
    // Stored 0.5 represents linear 2.0 with a common fourfold range. A
    // reader restoring +2EV before its look clips this to linear 1.0.
    let retained = render_profile(&profile_curve, [0.5; 3], true);
    let clipped = render_profile(&baseline, [1.; 3], true);
    let retained_error: f64 = retained
        .into_iter()
        .zip(expected)
        .map(|(a, b)| (a - b).abs())
        .sum();
    let clipped_error: f64 = clipped
        .into_iter()
        .zip(expected)
        .map(|(a, b)| (a - b).abs())
        .sum();
    eprintln!("headroom expected={expected:?} retained={retained:?} clipped={clipped:?} errors={retained_error}/{clipped_error}");
    assert!(retained_error < clipped_error);
    assert!(
        retained_error < 1. / 65535.,
        "gray grid knot should reproduce its response"
    );
}

#[test]
#[ignore = "requires a GPU and CLASSIC_X3F; optionally CLASSIC_DNG_DIR for metrics"]
fn classic_original_profile_appearance_residuals() {
    use crate::{dng, profile::Profile, render::Engine, source::Source};
    let source = Source::open(std::env::var_os("CLASSIC_X3F").unwrap()).unwrap();
    let engine = Engine::new().unwrap();
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    settings.sharpen_amount = Some(1.);
    let mut reports = Vec::new();
    for adjusted in [false, true] {
        if adjusted {
            settings.exposure = 0.75;
            settings.tone.contrast = 0.3;
            settings.tone.shadows = 0.4;
            settings.tone.highlights = -0.3;
            settings.saturation = 0.5;
            if let Some(list) = source.metadata.lists.get("ColorModeCompensations") {
                let name = list
                    .names()
                    .find(|n| n.eq_ignore_ascii_case("Vivid"))
                    .or_else(|| list.names().find(|n| !n.eq_ignore_ascii_case("Standard")));
                if let Some(name) = name {
                    settings.color_mode = name.to_owned();
                }
            }
        }
        let prepared = Prepared::new(&source.metadata, &settings).unwrap();
        let base = dng::prepare_base(&engine, &source, &settings, &mut Profile::default()).unwrap();
        let (packed, scale) = engine
            .encode_dng_base(&base, &prepared, &mut Profile::default())
            .unwrap();
        let profile = prepared.finish(scale).unwrap();
        let photo = engine
            .render(&source, &settings, &mut Profile::default())
            .unwrap();
        assert_eq!(base.view(), photo.view());
        let photo_bytes = engine.readback_image(&photo).unwrap();
        let base_bytes = engine.readback_image(&base).unwrap();
        let photo_values = bytemuck::cast_slice::<u8, f32>(&photo_bytes);
        let base_values = bytemuck::cast_slice::<u8, f32>(&base_bytes);
        let view = photo.view();
        let width = photo.description().extent[0];
        let crop_size = [view.crop[2] - view.crop[0], view.crop[3] - view.crop[1]];
        let response = prepared.signed_response.as_ref().unwrap();
        let storage_inverse = prepared.mapping.inverse().unwrap();
        let mut sums = [0.; 4];
        let mut maxima = [0_f64; 4];
        let mut count = 0;
        let mut pixel_errors = Vec::new();
        let mut worst_profile = serde_json::Value::Null;
        let mut clipped_pixels = 0;
        let mut in_range = [0_f64; 2];
        let mut in_range_components = 0;
        for gy in 0..23 {
            for gx in 0..31 {
                let x = ((gx + 1) * view.extent[0] / 32).min(view.extent[0] - 1);
                let y = ((gy + 1) * view.extent[1] / 24).min(view.extent[1] - 1);
                let [px, py] = match view.rotation {
                    90 => [y, crop_size[1] - 1 - x],
                    180 => [crop_size[0] - 1 - x, crop_size[1] - 1 - y],
                    270 => [crop_size[0] - 1 - y, x],
                    _ => [x, y],
                };
                let physical = ((py + view.crop[1]) * width + px + view.crop[0]) as usize * 3;
                let stored_pixel = (y * view.extent[0] + x) as usize * 8;
                let encoded = std::array::from_fn(|c| {
                    f64::from(u16::from_le_bytes(
                        packed[stored_pixel + c * 2..stored_pixel + c * 2 + 2]
                            .try_into()
                            .unwrap(),
                    )) / 65535.
                });
                let unclipped_reader = prepared
                    .camera_to_romm
                    .apply(encoded)
                    .map(|v| v * profile.baseline.max(0.).exp2());
                let clipped = unclipped_reader.iter().any(|v| *v < 0. || *v > 1.);
                if clipped {
                    clipped_pixels += 1;
                }
                let reader_input = unclipped_reader.map(|v| v.clamp(0., 1.));
                let fitted = render_profile(&profile, reader_input, true);
                let ideal_camera = prepared.mapping.apply(std::array::from_fn(|c| {
                    f64::from(base_values[physical + c])
                }));
                let ideal = response.evaluate_linear(prepared.camera_to_native.apply(ideal_camera));
                let quantized_camera = encoded.map(|v| v * f64::from(scale));
                let quantized =
                    response.evaluate_linear(prepared.camera_to_native.apply(quantized_camera));
                // Also verify camera-coordinate restoration, independently of profile fitting.
                let restored = storage_inverse.apply(ideal_camera);
                let mut pixel_error = 0_f64;
                for c in 0..3 {
                    assert!((restored[c] - f64::from(base_values[physical + c])).abs() < 1e-6);
                    let reference = f64::from(photo_values[physical + c]).clamp(0., 1.);
                    let errors = [
                        (ideal[c].clamp(0., 1.) - reference).abs(),
                        (fitted[c] - quantized[c].clamp(0., 1.)).abs(),
                        (fitted[c] - reference).abs(),
                        (ideal[c].clamp(0., 1.) - quantized[c].clamp(0., 1.)).abs(),
                    ];
                    pixel_error = pixel_error.max(errors[2]);
                    if errors[1] > maxima[1] {
                        worst_profile = serde_json::json!({"xy":[x,y],"reader_rgb":reader_input,
                            "fitted":fitted,"pointwise":quantized,"photographic":&photo_values[physical..physical+3]});
                    }
                    for i in 0..4 {
                        sums[i] += errors[i];
                        maxima[i] = maxima[i].max(errors[i]);
                    }
                    count += 1;
                    if !clipped {
                        in_range[0] += errors[1];
                        in_range[1] = in_range[1].max(errors[1]);
                        in_range_components += 1;
                    }
                }
                pixel_errors.push(pixel_error);
            }
        }
        pixel_errors.sort_by(f64::total_cmp);
        let quantile =
            |q: f64| pixel_errors[((pixel_errors.len() - 1) as f64 * q).round() as usize];
        let report = serde_json::json!({"adjusted":adjusted,"mode":settings.color_mode,
            "sampled_components":count,"scale":scale,"baseline":profile.baseline,
            "reader_clipped_pixels":clipped_pixels,
            "pixel_max_channel_error":{"p95":quantile(0.95),"p99":quantile(0.99),
                "fraction_above_0_01":pixel_errors.iter().filter(|e| **e>0.01).count() as f64/pixel_errors.len() as f64,
                "fraction_above_0_05":pixel_errors.iter().filter(|e| **e>0.05).count() as f64/pixel_errors.len() as f64},
            "worst_profile_sample":worst_profile,
            "in_range_profile":{"components":in_range_components,"mean":in_range[0]/f64::from(in_range_components),"max":in_range[1]},
            "canonical_approximation":{"mean":sums[0]/f64::from(count),"max":maxima[0]},
            "profile_interpolation":{"mean":sums[1]/f64::from(count),"max":maxima[1]},
            "total_vs_photographic":{"mean":sums[2]/f64::from(count),"max":maxima[2]},
            "sample_encoding":{"mean":sums[3]/f64::from(count),"max":maxima[3]}});
        eprintln!("Classic DNG original appearance: {report}");
        assert!(sums.into_iter().chain(maxima).all(|v| v.is_finite()));
        reports.push(report);
    }
    if let Some(directory) = std::env::var_os("CLASSIC_DNG_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let input = std::path::PathBuf::from(std::env::var_os("CLASSIC_X3F").unwrap());
        let name = format!(
            "{}-appearance.json",
            input.file_stem().unwrap().to_string_lossy()
        );
        std::fs::write(
            directory.join(name),
            serde_json::to_vec_pretty(&reports).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn tone_plateau_inverse_uses_first_crossing_without_changing_legacy_endpoint() {
    let tone: Vec<f64> = (0..=4095).map(|i| (i as f64 / 2048.).min(1.)).collect();
    let target = [0.25, 0.6, 1.];
    let first = inverse_rgb_tone(target, &tone, TonePlateau::FirstCrossing);
    let endpoint = inverse_rgb_tone(target, &tone, TonePlateau::WhiteEndpoint);
    assert_eq!(endpoint[2], 1.);
    assert_eq!(first[2], 2048. / 4095.);
    assert!(first[1] < endpoint[1]);
    // Reapply the reader's hue-preserving tone, including its middle-channel
    // interpolation, rather than independently curving all three channels.
    for rgb in [first, endpoint] {
        let low = rgb[0];
        let high = rgb[2];
        let lookup = |v: f64| (v * 4095. / 2048.).min(1.);
        let a = lookup(low);
        let b = lookup(high);
        let restored = rgb.map(|v| a + (b - a) * (v - low) / (high - low));
        for (actual, expected) in restored.into_iter().zip(target) {
            assert!((actual - expected).abs() < 1e-12);
        }
    }
    let approaching = inverse_rgb_tone([0.25, 0.6, 1. - 1e-8], &tone, TonePlateau::FirstCrossing);
    for (a, b) in approaching.into_iter().zip(first) {
        assert!((a - b).abs() < 1e-7);
    }
}

#[test]
fn adjacent_matrices_compose_in_order_without_crossing_tone() {
    let m = Matrix([1., 0.2, -0.1, 0., 1., 0.3, 0.1, 0., 1.]);
    let g = Matrix::diagonal([1.2, 0.8, 1.1]);
    let x = [0.1, 0.4, 0.6];
    for (a, b) in m.then(g).apply(x).into_iter().zip(g.apply(m.apply(x))) {
        assert!((a - b).abs() < 1e-12);
    }
    assert_ne!(m.then(g).apply(x), g.then(m).apply(x));
    let squared = m.apply(x).map(|v| v * v);
    assert_ne!(g.apply(squared), g.apply(m.apply(x)).map(|v| v * v));
    let restored = m.then(m.inverse().unwrap()).apply(x);
    for i in 0..3 {
        assert!((restored[i] - x[i]).abs() < 1e-12);
    }
    assert!(Matrix([0.; 9]).inverse().is_err());
}
#[test]
fn negative_exposure_inverse_and_hsv_coordinates_round_trip() {
    for e in [-3., -0.5, 0., 1.] {
        for x in [0., 0.01, 0.18, 0.7, 1.] {
            assert!((inverse_exposure(exposure_tone(x, e), e) - x).abs() < 1e-12);
        }
    }
    for h in 0..72 {
        for s in [0.01, 0.5, 1.] {
            let hsv = [h as f64 / 72., s, 0.3];
            let r = rgb_to_hsv(hsv_to_rgb(hsv[0], hsv[1], hsv[2]));
            for i in 0..3 {
                assert!((r[i] - hsv[i]).abs() < 1e-12);
            }
        }
    }
}
