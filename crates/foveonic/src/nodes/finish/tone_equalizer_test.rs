//! Checks neutral bypass, tonal selectivity, guided texture preservation and regional mask consistency.

use super::*;

fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    mask: &ReadGuard,
    extent: [u32; 2],
    response: Response,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    apply_with_white(gpu, input, mask, extent, response, NOMINAL_WHITE, profile)
}

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
    assert!(Guide::new([0, 4], 2, ColorSpace::Srgb).is_err());
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
    let guide = Guide::new(extent, 8, ColorSpace::Srgb).unwrap();
    let values: Vec<f32> = (0..1)
        .flat_map(|_| levels.into_iter().flat_map(|ev| [(ev + 8.) / 8.; 4]))
        .collect();
    let field = gpu.upload("Constant HDR response bases", &values).unwrap();
    let same = apply(
        &gpu,
        &input,
        &field,
        extent,
        Response {
            sampling: None,
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
                sampling: None,
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
    let field = build_guide(&gpu, &input, guide, &mut Profile::default()).unwrap();
    for amount in [-1., 1.] {
        let output = apply(
            &gpu,
            &input,
            &field,
            extent,
            Response {
                sampling: None,
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
        let values: Vec<f32> = (0..1)
            .flat_map(|_| {
                levels
                    .into_iter()
                    .flat_map(|ev| [(ev - residual + 8.) / 8.; 4])
            })
            .collect();
        let field = gpu.upload("Independent HDR base", &values).unwrap();
        let output = apply(
            &gpu,
            &input,
            &field,
            extent,
            Response {
                sampling: None,
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
    let guide = Guide::new(extent, 10, ColorSpace::AdobeRgb).unwrap();
    let spatial = build_guide(&gpu, &input, guide, &mut Profile::default()).unwrap();
    let s = gpu.readback(&spatial).unwrap();
    let s: &[f32] = bytemuck::cast_slice(&s);
    let at = |x: usize, y: usize| (y / 2) * 128 + x / 2;
    assert!((s[at(32, 64)] - s[at(36, 64)]).abs() < (0.25 / 8.) * 0.25);
    assert!((s[at(124, 64)] - s[at(132, 64)]).abs() > 4. / 8.);
    assert!(s.iter().all(|v| v.is_finite()));
    let constant = gpu
        .upload("Black mask fixture", &vec![0_f32; 256 * 128 * 3])
        .unwrap();
    let black = build_guide(&gpu, &constant, guide, &mut Profile::default()).unwrap();
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
    let guide = Guide::new(extent, 8, ColorSpace::Srgb).unwrap();
    let full = build_guide(&gpu, &input, guide, &mut Profile::default()).unwrap();
    let region_extent = [196, 144];
    let mut region = Vec::new();
    for y in 24..168 {
        region.extend_from_slice(&data[((y * 257 + 32) * 3)..((y * 257 + 228) * 3)]);
    }
    let input = gpu.upload("Aligned regional mask field", &region).unwrap();
    let local = build_guide(
        &gpu,
        &input,
        Guide::new(region_extent, 8, ColorSpace::Srgb).unwrap(),
        &mut Profile::default(),
    )
    .unwrap();
    let full = gpu.readback(&full).unwrap();
    let local = gpu.readback(&local).unwrap();
    let full: &[f32] = bytemuck::cast_slice(&full);
    let local: &[f32] = bytemuck::cast_slice(&local);
    for y in 40..104 {
        for x in 40..156 {
            assert!(
                (local[(y / 2) * 98 + x / 2] - full[((y + 24) / 2) * 129 + (x + 32) / 2]).abs()
                    < 0.00001
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_hdr_matches_across_declared_linear_code_units() {
    let gpu = Gpu::new().unwrap();
    let extent = [40, 2];
    let normalized: Vec<f32> = (0..80).flat_map(|i| [0.01 + 0.02 * i as f32; 3]).collect();
    let a = gpu
        .upload("normalized photographic samples", &normalized)
        .unwrap();
    for space in [
        ColorSpace::Srgb,
        ColorSpace::AdobeRgb,
        ColorSpace::ProPhotoRgb,
    ] {
        let ma = build_guide(
            &gpu,
            &a,
            Guide::new(extent, 8, space).unwrap().with_white(1.),
            &mut Profile::default(),
        )
        .unwrap();
        let response = || Response {
            sampling: None,
            controls: Controls {
                shadows: 0.5,
                highlights: -0.4,
                ..Default::default()
            },
            pivot: -2.,
            space,
        };
        let oa = apply_with_white(
            &gpu,
            &a,
            &ma,
            extent,
            response(),
            1.,
            &mut Profile::default(),
        )
        .unwrap();
        let aa = gpu.readback(&oa).unwrap();
        for white in [4095., 4096., 16384.] {
            let codes: Vec<f32> = normalized.iter().map(|v| v * white).collect();
            let b = gpu.upload("camera code samples", &codes).unwrap();
            let mb = build_guide(
                &gpu,
                &b,
                Guide::new(extent, 8, space).unwrap().with_white(white),
                &mut Profile::default(),
            )
            .unwrap();
            let ob = apply_with_white(
                &gpu,
                &b,
                &mb,
                extent,
                response(),
                white,
                &mut Profile::default(),
            )
            .unwrap();
            let bb = gpu.readback(&ob).unwrap();
            for (a, b) in bytemuck::cast_slice::<u8, f32>(&aa)
                .iter()
                .zip(bytemuck::cast_slice::<u8, f32>(&bb))
            {
                assert!(
                    (a - b / white).abs() < 0.00001,
                    "{space:?} white {white}: {a} vs {}",
                    b / white
                );
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_compact_guide_expands_exactly_over_odd_edges() {
    let gpu = Gpu::new().unwrap();
    for extent in [[24, 2], [257, 193], [25, 3], [1, 1]] {
        let [width, height] = extent;
        let data: Vec<f32> = (0..width * height)
            .flat_map(|p| {
                let level = 1. + (p * 37 % 4095) as f32;
                [level, level * 0.7, level * 1.1]
            })
            .collect();
        let input = gpu.upload("Compact guide odd-edge fixture", &data).unwrap();
        let guide = Guide::new(extent, 8, ColorSpace::Srgb).unwrap();
        let compact = build_guide(&gpu, &input, guide, &mut Profile::default()).unwrap();
        assert_eq!(compact.size(), guide_storage_bytes(extent, 2));
        let expanded = expand_mask(&gpu, &compact, guide, &mut Profile::default()).unwrap();
        assert_eq!(expanded.size(), u64::from(width) * u64::from(height) * 4);
        let compact = gpu.readback(&compact).unwrap();
        let expanded = gpu.readback(&expanded).unwrap();
        for y in 0..height {
            for x in 0..width {
                let cell = ((y / 2) * width.div_ceil(2) + x / 2) as usize * 4;
                let pixel = (y * width + x) as usize * 4;
                assert_eq!(&expanded[pixel..pixel + 4], &compact[cell..cell + 4]);
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_mapped_hdr_guide_preserves_identity_and_overlapping_sections() {
    use crate::nodes::finish::image_region;
    let gpu = Gpu::new().unwrap();
    let extent = [80, 64];
    let data: Vec<f32> = (0..extent[0] * extent[1])
        .flat_map(|p| {
            let level = 10. + (p * 37 % 4095) as f32;
            [level, level * 0.7, level * 1.1]
        })
        .collect();
    let input = gpu.upload("Mapped HDR input", &data).unwrap();
    let guide = Guide::new(extent, 16, ColorSpace::Srgb).unwrap();
    let mask = build_guide(&gpu, &input, guide, &mut Profile::default()).unwrap();
    let sampling = GuideSampling::new(
        extent,
        [0, 0, 80, 64],
        [[1., 0., 0., 0.], [0., 1., 0., 0.]],
        2,
        2,
    )
    .unwrap();
    let response = |sampling| Response {
        sampling,
        controls: Controls {
            shadows: 0.5,
            highlights: -0.5,
            blacks: 0.25,
            whites: -0.25,
        },
        pivot: -2.,
        space: ColorSpace::Srgb,
    };
    let whole = apply(
        &gpu,
        &input,
        &mask,
        extent,
        response(None),
        &mut Profile::default(),
    )
    .unwrap();
    let mapped = apply(
        &gpu,
        &input,
        &mask,
        extent,
        response(Some(sampling)),
        &mut Profile::default(),
    )
    .unwrap();
    let whole = gpu.readback(&whole).unwrap();
    assert_eq!(
        whole,
        gpu.readback(&mapped).unwrap(),
        "identity guide changed HDR"
    );
    for bounds in [[16, 16, 64, 48], [24, 16, 72, 48]] {
        let prepared = image_region::Prepared::new(extent, bounds, 3).unwrap();
        let local = image_region::apply(&gpu, &input, &prepared).unwrap();
        let local_extent = [bounds[2] - bounds[0], bounds[3] - bounds[1]];
        let result = apply(
            &gpu,
            &local,
            &mask,
            local_extent,
            response(Some(sampling.with_input_origin([bounds[0], bounds[1]]))),
            &mut Profile::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&result).unwrap();
        for y in 2..local_extent[1] - 2 {
            for x in 2..local_extent[0] - 2 {
                let local = ((y * local_extent[0] + x) * 12) as usize;
                let native = (((y + bounds[1]) * extent[0] + x + bounds[0]) * 12) as usize;
                assert_eq!(
                    &bytes[local..local + 12],
                    &whole[native..native + 12],
                    "shifted HDR at {x},{y}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_scaled_hdr_guide_clamps_padding_and_keeps_fractional_phase() {
    use crate::nodes::finish::image_region;
    let gpu = Gpu::new().unwrap();
    let extent = [24, 20];
    let input: Vec<f32> = (0..extent[0] * extent[1])
        .flat_map(|p| [20. + (p * 37 % 4095) as f32; 3])
        .collect();
    let input = gpu.upload("Scaled HDR RGB", &input).unwrap();
    let mut cells = vec![0.; 42];
    for y in 0..5 {
        for x in 1..6 {
            cells[y * 7 + x] = 0.2 + x as f32 * 0.03 + y as f32 * 0.05;
        }
    }
    let mask = gpu.upload("Odd valid HDR cells", &cells).unwrap();
    let sampling = GuideSampling::new(
        [13, 11],
        [3, 1, 12, 10],
        [[0.5, 0., 0.25, 0.], [0., 0.5, -0.25, 0.]],
        2,
        2,
    )
    .unwrap();
    let response = |sampling| Response {
        sampling: Some(sampling),
        controls: Controls {
            shadows: 0.5,
            highlights: -0.5,
            ..Default::default()
        },
        pivot: -2.,
        space: ColorSpace::Srgb,
    };
    let whole = apply(
        &gpu,
        &input,
        &mask,
        extent,
        response(sampling),
        &mut Profile::default(),
    )
    .unwrap();
    let whole = gpu.readback(&whole).unwrap();
    for y in 0..6 {
        for x in 0..7 {
            if x == 0 || x == 6 || y == 5 {
                cells[y * 7 + x] = 100.;
            }
        }
    }
    let padded = gpu.upload("Inactive HDR padding", &cells).unwrap();
    let result = apply(
        &gpu,
        &input,
        &padded,
        extent,
        response(sampling),
        &mut Profile::default(),
    )
    .unwrap();
    assert_eq!(
        whole,
        gpu.readback(&result).unwrap(),
        "inactive padding affected HDR"
    );
    let bounds = [4, 4, 20, 16];
    let prepared = image_region::Prepared::new(extent, bounds, 3).unwrap();
    let local = image_region::apply(&gpu, &input, &prepared).unwrap();
    let local_extent = [16, 12];
    let result = apply(
        &gpu,
        &local,
        &mask,
        local_extent,
        response(sampling.with_input_origin([4, 4])),
        &mut Profile::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&result).unwrap();
    for y in 2..10 {
        for x in 2..14 {
            let a = ((y * 16 + x) * 12) as usize;
            let b = (((y + 4) * 24 + x + 4) * 12) as usize;
            assert_eq!(
                &bytes[a..a + 12],
                &whole[b..b + 12],
                "fractional guide phase changed"
            );
        }
    }
}

#[test]
fn half_grid_guide_preserves_radii_odd_edges_and_crop_lattice() {
    let create = |radius| Guide::new([257, 193], radius, ColorSpace::Srgb).unwrap();
    let full = create(radius_for_scale(1.));
    assert_eq!(full.storage_extent(), [129, 97]);
    assert_eq!(full.cell_step(), 2);
    assert_eq!(guide_storage_bytes([257, 193], 2), 129 * 97 * 4);
    assert_eq!(
        full.stages(),
        vec![
            (2, 0),
            (6, 2),
            (12, 6),
            (24, 12),
            (48, 24),
            (96, 48),
            (192, 96),
            (384, 192)
        ]
    );
    assert_eq!(
        create(radius_for_scale(0.5)).stages(),
        vec![
            (2, 0),
            (6, 2),
            (12, 6),
            (24, 12),
            (48, 24),
            (96, 48),
            (192, 96)
        ]
    );
    assert_eq!(
        create(radius_for_scale(1. / 128.)).stages(),
        vec![(2, 0), (4, 2)]
    );
    assert_eq!(create(1).stages(), vec![(2, 0)]);
    assert!(full.with_input_bounds([3, 5, 31, 29]).is_err());
    assert_eq!(
        full.with_input_bounds([8, 16, 31, 29])
            .unwrap()
            .storage_extent(),
        [12, 7]
    );
    for (radius, residual) in full.stages() {
        let fine = full.at_scale(false, radius, residual, 0, false);
        assert_eq!(fine.sampling[2], 2);
        assert_eq!(fine.filter[0], (radius / 2) as f32);
        assert_eq!(fine.filter[2], (residual / 2) as f32);
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_half_grid_guide_matches_direct_two_dimensional_moments() {
    let gpu = Gpu::new().unwrap();
    let extent = [7, 5];
    let ev: Vec<f64> = (0..35)
        .map(|i| -6. + f64::from((i * 17) % 23) / 8.)
        .collect();
    let rgb: Vec<f32> = ev
        .iter()
        .flat_map(|v| [4095. * v.exp2() as f32; 3])
        .collect();
    let input = gpu.upload("Half-grid guide moment fixture", &rgb).unwrap();
    let guide = Guide::new(extent, 1, ColorSpace::Srgb).unwrap();
    let result = build_guide(&gpu, &input, guide, &mut Profile::default()).unwrap();
    let bytes = gpu.readback(&result).unwrap();
    let actual: &[f32] = bytemuck::cast_slice(&bytes);
    // Average linear brightness before taking its logarithm. Clamp each
    // contributing RGB sample independently at odd right and bottom edges.
    let cells: Vec<f64> = (0..3)
        .flat_map(|y| {
            let rgb = &rgb;
            (0..4).map(move |x| {
                let mut sum = 0.;
                for dy in 0..2 {
                    for dx in 0..2 {
                        let i = ((2 * y + dy).min(4) * 7 + (2 * x + dx).min(6)) * 3;
                        sum += f64::from(rgb[i]);
                    }
                }
                (sum / (4. * 4095.)).log2()
            })
        })
        .collect();
    // Direct 2D sums are independent of the shader's separable moment passes.
    let box_mean = |values: &[f64], x: i32, y: i32| -> f64 {
        let mut sum = 0.;
        for dy in -1..=1 {
            for dx in -1..=1 {
                sum += values[(y + dy).clamp(0, 2) as usize * 4 + (x + dx).clamp(0, 3) as usize];
            }
        }
        sum / 9.
    };
    let square: Vec<_> = cells.iter().map(|v| v * v).collect();
    let mut slopes = Vec::new();
    let mut offsets = Vec::new();
    for y in 0..3 {
        for x in 0..4 {
            let mean = box_mean(&cells, x, y);
            let variance = (box_mean(&square, x, y) - mean * mean).max(0.);
            let a = (1.125 * variance / (variance + 0.9_f64.powi(2))).min(1.);
            slopes.push(a);
            offsets.push((1. - a) * mean);
        }
    }
    for y in 0..3 {
        for x in 0..4 {
            let i = y as usize * 4 + x as usize;
            let expected =
                (box_mean(&slopes, x, y) * cells[i] + box_mean(&offsets, x, y) + 8.) / 8.;
            assert!(
                (f64::from(actual[i]) - expected).abs() < 0.00002,
                "{x},{y}: {} vs {expected}",
                actual[i]
            );
        }
    }
    let expanded = expand_mask(&gpu, &result, guide, &mut Profile::default()).unwrap();
    let expanded = gpu.readback(&expanded).unwrap();
    let expanded: &[f32] = bytemuck::cast_slice(&expanded);
    for y in 0..5 {
        for x in 0..7 {
            assert_eq!(expanded[y * 7 + x], actual[(y / 2) * 4 + x / 2]);
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_half_grid_gain_interpolates_anchors_and_clamps_odd_edges() {
    let gpu = Gpu::new().unwrap();
    let extent = [9, 3];
    let levels = [-6_f32, -3., 0., 1., 2.];
    let input: Vec<f32> = (0..3)
        .flat_map(|_| (0..9).flat_map(move |x| [4095. * levels[x / 2].exp2(); 3]))
        .collect();
    let mask: Vec<f32> = (0..2)
        .flat_map(|_| levels.into_iter().map(|ev| (ev + 8.) / 8.))
        .collect();
    let rgb = gpu.upload("Half-grid response steps", &input).unwrap();
    let mask = gpu.upload("Half-grid response bases", &mask).unwrap();
    let result = apply(
        &gpu,
        &rgb,
        &mask,
        extent,
        Response {
            sampling: None,
            controls: Controls {
                whites: -0.5,
                ..Default::default()
            },
            pivot: 0.,
            space: ColorSpace::Srgb,
        },
        &mut Profile::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&result).unwrap();
    let actual: &[f32] = bytemuck::cast_slice(&bytes);
    // Independently checked native response gains at the five half-grid anchors.
    let stops = [
        -0.0007038_f32,
        -0.0019805,
        -0.0179677,
        -0.1043254,
        -0.4857178,
    ];
    for y in 0..3 {
        for x in 0..9 {
            let position = (x as f32 + 0.5) / 2. - 0.5;
            let low = position.floor() as i32;
            let phase = position - position.floor();
            let a = stops[low.clamp(0, 4) as usize].exp2();
            let b = stops[(low + 1).clamp(0, 4) as usize].exp2();
            let expected = a * (1. - phase) + b * phase;
            let i = (y * 9 + x) * 3;
            for c in 0..3 {
                assert!((actual[i + c] / input[i + c] - expected).abs() < 0.00002);
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_half_grid_mapped_guide_matches_pixel_center_affines_and_ignores_padding() {
    let gpu = Gpu::new().unwrap();
    let extent = [9, 7];
    let input: Vec<f32> = (0..63)
        .flat_map(|p| [30. + (p * 37 % 4095) as f32; 3])
        .collect();
    let input = gpu.upload("Half-grid affine input", &input).unwrap();
    let mut cells: Vec<f32> = (0..13)
        .flat_map(|y| {
            (0..11).map(move |x| {
                if (2..10).contains(&x) && (3..12).contains(&y) {
                    0.2 + x as f32 * 0.03 + y as f32 * 0.02
                } else {
                    100.
                }
            })
        })
        .collect();
    for mapping in [
        [[0.5, 0., 3.75, 0.], [0., 0.5, 5.75, 0.]],
        [[0., 0.5, 5.75, 0.], [-0.5, 0., 20.25, 0.]],
    ] {
        let sampling = GuideSampling::new([22, 26], [4, 6, 20, 24], mapping, 2, 2).unwrap();
        let expected: Vec<f32> = (0..4)
            .flat_map(|y| {
                (0..5).map(move |x| {
                    let x = (2 * x) as f32 + 0.5;
                    let y = (2 * y) as f32 + 0.5;
                    let u = ((mapping[0][0] * x + mapping[0][1] * y + mapping[0][2] - 0.5) / 2.)
                        .clamp(2., 9.);
                    let v = ((mapping[1][0] * x + mapping[1][1] * y + mapping[1][2] - 0.5) / 2.)
                        .clamp(3., 11.);
                    0.2 + u * 0.03 + v * 0.02
                })
            })
            .collect();
        let expected = gpu.upload("CPU affine half-grid guide", &expected).unwrap();
        let response = |sampling| Response {
            sampling,
            controls: Controls {
                shadows: 0.5,
                highlights: -0.5,
                ..Default::default()
            },
            pivot: -2.,
            space: ColorSpace::Srgb,
        };
        let direct = apply(
            &gpu,
            &input,
            &expected,
            extent,
            response(None),
            &mut Profile::default(),
        )
        .unwrap();
        let direct = gpu.readback(&direct).unwrap();
        for padding in [100., -100.] {
            for y in 0..13 {
                for x in 0..11 {
                    if !(2..10).contains(&x) || !(3..12).contains(&y) {
                        cells[y * 11 + x] = padding;
                    }
                }
            }
            let mask = gpu
                .upload("Half-grid guide with inactive padding", &cells)
                .unwrap();
            let mapped = apply(
                &gpu,
                &input,
                &mask,
                extent,
                response(Some(sampling)),
                &mut Profile::default(),
            )
            .unwrap();
            let mapped = gpu.readback(&mapped).unwrap();
            for (a, b) in bytemuck::cast_slice::<u8, f32>(&mapped)
                .iter()
                .zip(bytemuck::cast_slice::<u8, f32>(&direct))
            {
                assert!((a - b).abs() < 0.00002 * b.abs().max(1.), "{a} vs {b}");
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_half_grid_cascade_matches_whole_with_an_aligned_crop_origin() {
    let gpu = Gpu::new().unwrap();
    let extent = [513_u32, 481];
    let bounds = [16_u32, 24, 504, 472];
    let values: Vec<f32> = (0..extent[1])
        .flat_map(|y| {
            (0..extent[0]).flat_map(move |x| {
                let ev = -6. + (x as f32 / 29.).sin() + (y as f32 / 41.).cos() * 2.;
                [4095. * ev.exp2(); 3]
            })
        })
        .collect();
    let input = gpu
        .upload("Whole half-grid cascade fixture", &values)
        .unwrap();
    let guide = Guide::new(extent, 48, ColorSpace::Srgb).unwrap();
    let whole = build_guide(&gpu, &input, guide, &mut Profile::default()).unwrap();
    let cropped: Vec<f32> = (bounds[1]..bounds[3])
        .flat_map(|y| {
            values[((y * extent[0] + bounds[0]) * 3) as usize
                ..((y * extent[0] + bounds[2]) * 3) as usize]
                .iter()
                .copied()
        })
        .collect();
    let cropped = gpu
        .upload("Aligned half-grid cascade backing", &cropped)
        .unwrap();
    let local_guide = guide.with_input_bounds(bounds).unwrap();
    let local = build_guide(&gpu, &cropped, local_guide, &mut Profile::default()).unwrap();
    let whole = gpu.readback(&whole).unwrap();
    let local = gpu.readback(&local).unwrap();
    let whole: &[f32] = bytemuck::cast_slice(&whole);
    let local: &[f32] = bytemuck::cast_slice(&local);
    let width = bounds[2] - bounds[0];
    let height = bounds[3] - bounds[1];
    // Two radius-6 coefficient means and a radius-3 residual on the
    // eight-pixel coarse lattice need less than 144 working pixels of context.
    for y in 72..height.div_ceil(2) - 72 {
        for x in 72..width.div_ceil(2) - 72 {
            assert_eq!(
                local[(y * width.div_ceil(2) + x) as usize],
                whole[((y + bounds[1] / 2) * extent[0].div_ceil(2) + x + bounds[0] / 2) as usize],
                "half-grid crop differs at {x},{y}"
            );
        }
    }
}

#[test]
fn guide_workspace_keeps_one_scalar_base_and_three_moment_images() {
    for extent in [[1, 1], [25, 3], [257, 193], [4800, 3200]] {
        for radius in [1, 30, 32, radius_for_scale(1.)] {
            let guide = Guide::new(extent, radius, ColorSpace::Srgb).unwrap();
            let storage = GuideStorage::new(guide);
            let cells = u64::from(guide.storage_extent()[0]) * u64::from(guide.storage_extent()[1]);
            assert_eq!(storage.mask, cells * 4);
            assert_eq!(storage.moments, cells * 16);
            let coarse = if radius > 30 {
                let coarse_cells =
                    u64::from(extent[0].div_ceil(8)) * u64::from(extent[1].div_ceil(8));
                Some(coarse_cells * 16)
            } else {
                None
            };
            assert_eq!(storage.coarse_moments, coarse);
            let allocated =
                storage.mask + 3 * storage.moments + storage.coarse_moments.unwrap_or(0);
            assert_eq!(allocated, cells * 52 + coarse.unwrap_or(0));
            assert!(!guide.stages().is_empty());
        }
    }
    assert!(Guide::new([1, 1], 0, ColorSpace::Srgb).is_err());
}
