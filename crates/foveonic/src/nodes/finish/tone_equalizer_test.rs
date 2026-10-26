//! Checks neutral bypass, tonal selectivity, guided texture preservation and regional mask consistency.

use super::*;

#[test]
fn neutral_controls_and_saved_choices_are_independent() {
    let mut c = Controls::default();
    assert!(c.is_neutral());
    c.shadows = 0.5;
    assert!(!c.is_neutral());
    assert_eq!(
        serde_json::from_str::<Controls>(&serde_json::to_string(&c).unwrap()).unwrap(),
        c
    );
    assert_eq!(
        serde_json::from_str::<Controls>("{}").unwrap(),
        Controls::default()
    );
    for value in [f32::NAN, f32::INFINITY, -1.01, 1.01] {
        assert!(Controls { whites: value, ..c }.validate().is_err());
    }
    assert!(Guide::new([0, 4], 1, ColorSpace::Srgb).is_err());
}

#[test]
fn saved_hdr_amounts_migrate_once_and_new_versions_are_explicit() {
    let old = r#"{"mask":"global","shadows":50,"highlights":-100}"#;
    let c: Controls = serde_json::from_str(old).unwrap();
    assert_eq!(c.shadows, 0.5);
    assert_eq!(c.highlights, -1.);
    let saved = serde_json::to_string(&c).unwrap();
    assert!(saved.contains("\"version\":1"));
    assert!(!saved.contains("mask"));
    assert_eq!(serde_json::from_str::<Controls>(&saved).unwrap(), c);
    assert!(serde_json::from_str::<Controls>(r#"{"version":2}"#).is_err());
}

#[test]
fn pivot_follows_the_camera_curve_and_exposure() {
    let curve: Vec<_> = (0..65536).map(|i| i as f32 / 4095.).collect();
    let native = pivot(&curve, 0.).unwrap();
    assert!((native + 2.).abs() < 0.00001);
    assert!((pivot(&curve, 1.5).unwrap() - native + 1.5).abs() < 0.00001);
    assert!(pivot(&[], 0.).is_err());
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_tonal_ranges_match_native_response_and_preserve_signed_samples() {
    let gpu = Gpu::new().unwrap();
    let extent = [40, 2];
    let levels = [-6_f32, -3., 0., 1., 2.];
    let data: Vec<f32> = (0..2)
        .flat_map(|_| levels.into_iter().flat_map(|ev| [4095. * ev.exp2(); 24]))
        .collect();
    let input = gpu.upload("HDR flat exposure steps", &data).unwrap();
    let guide = Guide::new(extent, 4, ColorSpace::Srgb).unwrap();
    let values: Vec<f32> = (0..2)
        .flat_map(|_| levels.into_iter().flat_map(|ev| [(ev + 8.) / 8.; 8]))
        .collect();
    let field = gpu.upload("Constant HDR response bases", &values).unwrap();
    let same = apply(
        &gpu,
        &input,
        &field,
        extent,
        Response {
            controls: Controls::default(),
            pivot: 0.,
            space: ColorSpace::Srgb,
        },
        &mut Profile::default(),
    )
    .unwrap();
    assert_eq!(same.identity(), input.identity());
    // Scalar gains independently checked against the native HDR kernel.
    let cases = [
        (
            Controls {
                blacks: -0.5,
                ..Default::default()
            },
            [
                -0.9999508_f32,
                -0.9646239,
                -0.0379501,
                -0.0172329,
                -0.0097402,
            ],
        ),
        (
            Controls {
                shadows: 0.5,
                ..Default::default()
            },
            [1.3988786, 1.3031439, 0.1304081, 0.0208438, 0.0071360],
        ),
        (
            Controls {
                highlights: -0.5,
                ..Default::default()
            },
            [-0.0005842, -0.0021127, -0.0796507, -0.3514782, -0.7898685],
        ),
        (
            Controls {
                whites: -0.5,
                ..Default::default()
            },
            [-0.0007038, -0.0019805, -0.0179677, -0.1043254, -0.4857178],
        ),
    ];
    for (controls, stops) in cases {
        let output = apply(
            &gpu,
            &input,
            &field,
            extent,
            Response {
                controls,
                pivot: 0.,
                space: ColorSpace::Srgb,
            },
            &mut Profile::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        for (i, ev) in stops.into_iter().enumerate() {
            let p = (i * 8 + 4) * 3;
            let want = data[p] * ev.exp2();
            assert!(
                (actual[p] - want).abs() / want < 0.00002,
                "{controls:?} step {i}: {} vs {want}",
                actual[p]
            );
        }
    }
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&data)
    );
    let signed: Vec<f32> = (0..80)
        .flat_map(|i| [i as f32 - 40., 4095., 16000.])
        .collect();
    let input = gpu.upload("HDR signed headroom", &signed).unwrap();
    let field = mask(&gpu, &input, guide, &mut Profile::default()).unwrap();
    for amount in [-1., 1.] {
        let output = apply(
            &gpu,
            &input,
            &field,
            extent,
            Response {
                controls: Controls {
                    blacks: amount,
                    shadows: amount,
                    highlights: amount,
                    whites: amount,
                },
                pivot: -2.,
                space: ColorSpace::Srgb,
            },
            &mut Profile::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        for (before, after) in signed
            .as_chunks::<3>()
            .0
            .iter()
            .zip(actual.as_chunks::<3>().0)
        {
            let gain = after[1] / before[1];
            assert!(after.iter().all(|v| v.is_finite()));
            for c in [0, 2] {
                assert!((after[c] - before[c] * gain).abs() < 0.01 * gain.max(1.));
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_hdr_preserves_native_control_coupling_and_detail_direction() {
    let gpu = Gpu::new().unwrap();
    let extent = [24, 2];
    let levels = [-5.5_f32, -1., 0.5];
    let data: Vec<f32> = (0..2)
        .flat_map(|_| levels.into_iter().flat_map(|ev| [4095. * ev.exp2(); 24]))
        .collect();
    let input = gpu.upload("HDR response detail fixture", &data).unwrap();
    // Native Metal outputs include all four controls simultaneously and both
    // signs of the original-minus-base detail. These detect accidental changes
    // to gain coupling even when each isolated flat range still looks plausible.
    for (residual, amount, expected) in [
        (
            -0.5_f32,
            -1_f32,
            [0.0000068793406_f32, 0.346_422, 0.5366148],
        ),
        (-0.5, 1., [13.205472, 1.4678612, 2.8237767]),
        (0.5, -1., [0.0000048572033, 0.06143027, 0.77813244]),
        (0.5, 1., [23.457682, 3.044465, 1.7864335]),
    ] {
        let values: Vec<f32> = (0..2)
            .flat_map(|_| {
                levels
                    .into_iter()
                    .flat_map(|ev| [(ev - residual + 8.) / 8.; 8])
            })
            .collect();
        let field = gpu.upload("Independent HDR base", &values).unwrap();
        let output = apply(
            &gpu,
            &input,
            &field,
            extent,
            Response {
                controls: Controls {
                    blacks: amount,
                    shadows: amount,
                    highlights: amount,
                    whites: amount,
                },
                pivot: 0.,
                space: ColorSpace::Srgb,
            },
            &mut Profile::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        for (i, gain) in expected.into_iter().enumerate() {
            let p = (i * 8 + 4) * 3;
            assert!((actual[p] / data[p] - gain).abs() / gain < 0.00002);
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_spatial_mask_smooths_texture_and_keeps_strong_edges() {
    let gpu = Gpu::new().unwrap();
    let extent = [256, 128];
    let data: Vec<f32> = (0..extent[1])
        .flat_map(|y| {
            (0..extent[0]).flat_map(move |x| {
                let ev = if x < 128 { -6. } else { -1. };
                let texture = if ((x / 4) + (y / 4)) % 2 == 0 {
                    -0.125
                } else {
                    0.125
                };
                [4095. * 2_f32.powf(ev + texture); 3]
            })
        })
        .collect();
    let input = gpu.upload("Textured brightness step", &data).unwrap();
    let guide = Guide::new(extent, 5, ColorSpace::AdobeRgb).unwrap();
    let spatial = mask(&gpu, &input, guide, &mut Profile::default()).unwrap();
    let s = gpu.readback(&spatial).unwrap();
    let s: &[f32] = bytemuck::cast_slice(&s);
    let at = |x: usize, y: usize| y * 256 + x;
    assert!((s[at(32, 64)] - s[at(36, 64)]).abs() < (0.25 / 8.) * 0.25);
    assert!((s[at(124, 64)] - s[at(132, 64)]).abs() > 4. / 8.);
    assert!(s.iter().all(|v| v.is_finite()));
    let constant = gpu
        .upload("Black mask fixture", &vec![0_f32; 256 * 128 * 3])
        .unwrap();
    let black = mask(&gpu, &constant, guide, &mut Profile::default()).unwrap();
    let bytes = gpu.readback(&black).unwrap();
    assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
        .iter()
        .all(|v| (*v + 1.).abs() < 0.0001));
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_regional_mask_matches_full_with_aligned_support() {
    let gpu = Gpu::new().unwrap();
    let extent = [257, 193];
    let data: Vec<f32> = (0..extent[1])
        .flat_map(|y| {
            (0..extent[0]).flat_map(move |x| {
                let ev = -8. + (x as f32 / 29.).sin() + (y as f32 / 41.).cos() * 2.;
                [4095. * 2_f32.powf(ev); 3]
            })
        })
        .collect();
    let input = gpu.upload("Full mask field", &data).unwrap();
    let guide = Guide::new(extent, 4, ColorSpace::Srgb).unwrap();
    let full = mask(&gpu, &input, guide, &mut Profile::default()).unwrap();
    let region_extent = [196, 144];
    let mut region = Vec::new();
    for y in 24..168 {
        region.extend_from_slice(&data[((y * 257 + 32) * 3)..((y * 257 + 228) * 3)]);
    }
    let input = gpu.upload("Aligned regional mask field", &region).unwrap();
    let local = mask(
        &gpu,
        &input,
        Guide::new(region_extent, 4, ColorSpace::Srgb).unwrap(),
        &mut Profile::default(),
    )
    .unwrap();
    let full = gpu.readback(&full).unwrap();
    let local = gpu.readback(&local).unwrap();
    let full: &[f32] = bytemuck::cast_slice(&full);
    let local: &[f32] = bytemuck::cast_slice(&local);
    for y in 40..104 {
        for x in 40..156 {
            assert!((local[y * 196 + x] - full[(y + 24) * 257 + x + 32]).abs() < 0.00001);
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_normalized_photographic_hdr_matches_camera_code_units() {
    let gpu = Gpu::new().unwrap();
    let extent = [40, 2];
    let normalized: Vec<f32> = (0..80).flat_map(|i| [0.01 + 0.02 * i as f32; 3]).collect();
    let codes: Vec<f32> = normalized.iter().map(|v| v * 4095.).collect();
    let a = gpu
        .upload("normalized photographic samples", &normalized)
        .unwrap();
    let b = gpu.upload("camera code samples", &codes).unwrap();
    for space in [
        ColorSpace::Srgb,
        ColorSpace::AdobeRgb,
        ColorSpace::ProPhotoRgb,
    ] {
        let ma = mask(
            &gpu,
            &a,
            Guide::photographic(extent, 4, space).unwrap(),
            &mut Profile::default(),
        )
        .unwrap();
        let mb = mask(
            &gpu,
            &b,
            Guide::new(extent, 4, space).unwrap(),
            &mut Profile::default(),
        )
        .unwrap();
        let response = || Response {
            controls: Controls {
                shadows: 0.5,
                highlights: -0.4,
                ..Default::default()
            },
            pivot: -2.,
            space,
        };
        let oa =
            apply_photographic(&gpu, &a, &ma, extent, response(), &mut Profile::default()).unwrap();
        let ob = apply(&gpu, &b, &mb, extent, response(), &mut Profile::default()).unwrap();
        let aa = gpu.readback(&oa).unwrap();
        let bb = gpu.readback(&ob).unwrap();
        for (a, b) in bytemuck::cast_slice::<u8, f32>(&aa)
            .iter()
            .zip(bytemuck::cast_slice::<u8, f32>(&bb))
        {
            assert!(
                (a - b / 4095.).abs() < 0.00001,
                "{space:?}: {a} vs {}",
                b / 4095.
            );
        }
    }
}
