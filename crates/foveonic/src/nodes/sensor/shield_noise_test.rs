//! Checks robust shield extraction, trained Gaussian predictions and native-sum preservation.

use super::*;

fn median(values: &mut [f32]) -> f32 {
    values.sort_by(f32::total_cmp);
    (values[(values.len() - 1) / 2] + values[values.len() / 2]) * 0.5
}

fn measured_profile(original: &[[f32; 3]], p: &Prepared, axis: usize) -> Vec<[f64; 3]> {
    let r = if axis == 0 {
        p.params.right
    } else {
        p.params.bottom
    };
    let width = p.params.source[0] as usize;
    let mut centers = [0.; 3];
    let mut bounds = [0.; 3];
    for c in 0..3 {
        let mut samples = Vec::new();
        for y in r[1]..r[3] {
            for x in r[0]..r[2] {
                samples.push(original[y as usize * width + x as usize][c]);
            }
        }
        centers[c] = median(&mut samples);
        for v in &mut samples {
            *v = (*v - centers[c]).abs();
        }
        bounds[c] = (8. * 1.4826 * median(&mut samples)).max(8.);
    }
    let extent = if axis == 0 { r[3] - r[1] } else { r[2] - r[0] };
    let count = if axis == 0 { r[2] - r[0] } else { r[3] - r[1] };
    let mut output = vec![[0.; 3]; extent as usize];
    for i in 0..extent {
        for j in 0..count {
            let (x, y) = if axis == 0 {
                (r[0] + j, r[1] + i)
            } else {
                (r[0] + i, r[1] + j)
            };
            let value = original[y as usize * width + x as usize];
            for c in 0..3 {
                output[i as usize][c] +=
                    f64::from(value[c].clamp(centers[c] - bounds[c], centers[c] + bounds[c]))
                        / f64::from(count);
            }
        }
    }
    output
}

fn predicted_profile(original: &[[f32; 3]], p: &Prepared, axis: usize, high: bool) -> Vec<f64> {
    let mut profile = measured_profile(original, p, axis);
    let mut mean = [0.; 3];
    for value in &profile {
        for c in 0..3 {
            mean[c] += value[c] / profile.len() as f64;
        }
    }
    for value in &mut profile {
        for c in 0..3 {
            value[c] -= mean[c];
        }
    }
    let state = usize::from(high);
    let model_index = if p.params.source[3] == 0 {
        0
    } else if axis == 0 {
        1 + state
    } else {
        3 + state
    };
    let weights = model_index * WEIGHTS_PER_MODEL;
    let mut output = vec![0.; profile.len()];
    for i in 0..profile.len() {
        for (c, value) in profile[i].iter().enumerate() {
            output[i] += value * f64::from(p.model[weights + c]);
        }
        let mut kernel = MODEL_COUNT * WEIGHTS_PER_MODEL;
        for (scale, radius) in GAUSSIAN_SIGMAS.map(|v| v * 4).into_iter().enumerate() {
            for offset in -(radius as isize)..=radius as isize {
                let period = 2 * (profile.len() as isize - 1);
                let folded = (i as isize + offset).rem_euclid(period);
                let index = folded.min(period - folded) as usize;
                for (c, value) in profile[index].iter().enumerate() {
                    output[i] += value
                        * f64::from(p.model[kernel + (offset + radius as isize) as usize])
                        * f64::from(p.model[weights + (scale + 1) * 3 + c]);
                }
            }
            kernel += radius * 2 + 1;
        }
        if axis == 1 {
            output[i] -=
                f64::from(p.model[REFERENCE_OFFSET + state * COLUMN_REFERENCE_LENGTH as usize + i]);
        }
    }
    let mean = output.iter().sum::<f64>() / output.len() as f64;
    for v in &mut output {
        *v -= mean;
    }
    output
}

#[test]
fn shield_shader_and_metadata_validate() {
    let module = wgpu::naga::front::wgsl::parse_str(include_str!("shield_noise.wgsl")).unwrap();
    wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    assert!(Prepared::new(
        [32, 32],
        [2, 2, 18, 18],
        [2, 20, 18, 22],
        [20, 2, 24, 18],
        [-1., 1., 0.],
        false
    )
    .is_ok());
    assert!(Prepared::new(
        [32, 32],
        [2, 2, 18, 18],
        [2, 20, 18, 22],
        [20, 2, 24, 18],
        [-1., 1., 0.],
        true
    )
    .is_err());
    assert!(Prepared::new(
        [32, 32],
        [2, 2, 18, 18],
        [2, 20, 18, 22],
        [20, 2, 24, 18],
        [1., 1., 1.],
        false
    )
    .is_err());
    assert!(Prepared::new(
        [32, 32],
        [2, 2, 18, 18],
        [2, 17, 18, 19],
        [20, 2, 24, 18],
        [-1., 1., 0.],
        false
    )
    .is_err());
}

#[test]
#[ignore = "requires a GPU"]
fn native_shield_predictions_match_robust_independent_profiles_in_both_states() {
    let gpu = Gpu::new().unwrap();
    let extent = [4712, 26];
    let active = [2, 2, 4706, 18];
    let bottom = [2, 20, 4706, 24];
    let right = [4708, 2, 4712, 18];
    let mut original = vec![[16.; 3]; (extent[0] * extent[1]) as usize];
    for y in active[1]..active[3] {
        for x in active[0]..active[2] {
            original[(y * extent[0] + x) as usize] =
                [100. + (x % 9) as f32, 80. + (y % 5) as f32, 60.];
        }
    }
    for y in right[1]..right[3] {
        for x in right[0]..right[2] {
            original[(y * extent[0] + x) as usize] =
                [30. + (y % 5) as f32, 30. + ((y * 3 + x) % 7) as f32, 30.];
        }
    }
    for y in bottom[1]..bottom[3] {
        for x in bottom[0]..bottom[2] {
            let phase = ((x / 64) % 9) as f32 - 4.;
            original[(y * extent[0] + x) as usize] =
                [16. + phase, 16. - phase, 16. + (phase * 0.5).floor()];
        }
    }
    // Sparse shield defects exercise the region's own median/MAD guard. The
    // right region's pedestal differs from the bottom's black baseline.
    original[(right[1] * extent[0] + right[0]) as usize] = [4095., 0., 4095.];
    original[(bottom[1] * extent[0] + bottom[0]) as usize] = [0., 4095., 4095.];
    let mut working = original.clone();
    for y in 0..extent[1] {
        for x in 0..extent[0] {
            let i = (y * extent[0] + x) as usize;
            if x >= active[0] && x < active[2] && y >= active[1] && y < active[3] {
                working[i][0] += 2.;
                working[i][1] -= 1.;
                working[i][2] -= 1.;
            } else {
                working[i] = [2000.; 3];
            }
        }
    }
    let original_buffer = gpu
        .upload("Shield immutable synthetic sensor", &original)
        .unwrap();
    let input = gpu
        .upload("Shield working synthetic sensor", &working)
        .unwrap();
    let mut row_results = Vec::new();
    for columns in [false, true] {
        let p = Prepared::new(extent, active, bottom, right, [-0.6, 0.9, -0.3], columns).unwrap();
        for high in [false, true] {
            let black = gpu
                .upload(
                    "Shield synthetic black state",
                    &[if high { 31_f32 } else { 16_f32 }; 3],
                )
                .unwrap();
            let result = apply(
                &gpu,
                &input,
                &original_buffer,
                &black,
                &p,
                &mut Profile::default(),
            )
            .unwrap();
            let bytes = gpu.readback(&result).unwrap();
            let actual = bytemuck::cast_slice::<u8, [f32; 3]>(&bytes);
            let rows = predicted_profile(&original, &p, 0, high);
            let cols = if columns {
                predicted_profile(&original, &p, 1, high)
            } else {
                vec![0.; 4704]
            };
            let mut changed = 0;
            for y in 0..extent[1] {
                for x in 0..extent[0] {
                    let i = (y * extent[0] + x) as usize;
                    if x < active[0] || x >= active[2] || y < active[1] || y >= active[3] {
                        assert_eq!(actual[i], original[i], "immutable margin {x},{y}");
                        continue;
                    }
                    let error = rows[(y - active[1]) as usize] + cols[(x - active[0]) as usize];
                    for c in 0..3 {
                        let expected =
                            f64::from(working[i][c]) - error * f64::from(p.params.direction[c]);
                        assert!(actual[i][c].is_finite());
                        assert!((f64::from(actual[i][c]) - expected).abs() < 0.0005,
                            "columns {columns} high {high} pixel {x},{y} layer {c}: {} versus {expected}", actual[i][c]);
                    }
                    assert!(
                        (actual[i].iter().sum::<f32>() - working[i].iter().sum::<f32>()).abs()
                            < 0.0001
                    );
                    changed += usize::from((actual[i][0] - working[i][0]).abs() > 0.001);
                }
            }
            assert!(
                changed > 100,
                "fixture must exercise learned shield correction"
            );
            if !columns {
                row_results.push(bytes);
            }
        }
    }
    assert_eq!(
        row_results[0], row_results[1],
        "Rows must not depend on black pedestal state"
    );
}
