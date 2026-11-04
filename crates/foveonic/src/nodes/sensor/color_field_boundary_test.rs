//! Checks boundary scope, immutable evidence, and continuous brightness through matrix cancellation.
use super::*;
#[test]
#[ignore = "requires a GPU"]
fn gpu_color_field_boundary_preserves_distant_samples_and_supported_luminance() {
    let gpu = Gpu::new().unwrap();
    let width = 48usize;
    let height = 32usize;
    let neutral = [0.284206, 0.604436, 1.0];
    let luminance = [-2.6575486, 3.3135306, -0.2476];
    let matrix = [
        4.45774, -1.90728, 0.581922, -4.22818, 4.30012, -0.701463, 3.1376, -5.81127, 3.31683,
    ];
    let mut raw = vec![[0.4_f32; 3]; width * height];
    let mut pixels = vec![[0.27_f32, 0.60, 0.94]; width * height];
    for row in 8..24 {
        for col in 24..32 {
            raw[row * width + col] = [1.; 3];
            pixels[row * width + col] = [0.6, 1.2, 2.0];
        }
    }
    let edge = 16 * width + 23;
    pixels[edge] = [0.9, 0.5, 0.8];
    let positive_edge = 15 * width + 23;
    pixels[positive_edge] = [0.3, 0.6, 0.8];
    let rejected = 16 * width + 10;
    raw[rejected] = [1., 0.5, 0.5];
    pixels[rejected] = [1., 0.4, 0.7];
    let input = gpu.upload("boundary calibrated", &pixels).unwrap();
    let original = gpu.upload("boundary original", &raw).unwrap();
    let black = gpu.upload("boundary black", &[0_f32; 3]).unwrap();
    for brightness in [1, 2] {
        let prepared = Prepared {
            params: Params {
                size: [width as u32, height as u32, (width * height) as u32, 0],
                grid: [6, 4, 24, 8],
                crop: [0, 0, width as u32, height as u32],
                gain: [1.; 4],
                white: [1.; 4],
                neutral: [neutral[0] as f32, neutral[1] as f32, neutral[2] as f32, 0.],
                luminance: [luminance[0], luminance[1], luminance[2], 0.],
                metric: color_metric(neutral, matrix).unwrap(),
                tuning: [0.035, 0.75, 1., 1.],
                options: [brightness, 1, 0, 0],
            },
            repairs: Vec::new(),
        };
        let original_masks =
            super::reliability_tests::original_masks(&gpu, &prepared.params, &original, &black);
        let field_masks = super::reliability_tests::field_masks(
            &gpu,
            &prepared.params,
            &original,
            &black,
            &input,
        );
        assert_eq!(
            original_masks[edge] & 0xffffff,
            0xffffff,
            "original clipping reliability changed"
        );
        assert_ne!(
            field_masks[edge] & 0xffffff,
            0xffffff,
            "unreliable boundary evidence was not rejected"
        );
        let output = apply(
            &gpu,
            Inputs {
                input: &input,
                original: &original,
                black: &black,
                unrepaired: &input,
            },
            &prepared,
            &mut Profile::default(),
            &mut Execution::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let values: &[f32] = bytemuck::cast_slice(&bytes);
        assert!(values.iter().all(|v| v.is_finite()));
        for row in 0..height {
            for col in 0..width {
                let index = row * width + col;
                if index != rejected && (!(20..=35).contains(&col) || !(4..=27).contains(&row)) {
                    assert_eq!(
                        &values[index * 3..index * 3 + 3],
                        &pixels[index],
                        "outside boundary changed at {col},{row}, brightness {brightness}"
                    );
                }
            }
        }
        let before_y: f32 = pixels[positive_edge]
            .iter()
            .zip(luminance)
            .map(|(m, y)| m * y)
            .sum();
        let after_y: f32 = values[positive_edge * 3..positive_edge * 3 + 3]
            .iter()
            .zip(luminance)
            .map(|(m, y)| m * y)
            .sum();
        assert!(
            (before_y - after_y).abs() < 2e-5,
            "supported boundary luminance changed: {before_y} -> {after_y}"
        );
        let got = &values[rejected * 3..rejected * 3 + 3];
        assert_ne!(
            got, &pixels[rejected],
            "negative luminance remained rejected"
        );
        assert_ne!(
            &values[edge * 3..edge * 3 + 3],
            &pixels[edge],
            "boundary was not corrected"
        );
    }
    assert_eq!(
        bytemuck::cast_slice::<u8, f32>(&gpu.readback(&input).unwrap()),
        pixels.as_flattened()
    );
    assert_eq!(
        bytemuck::cast_slice::<u8, f32>(&gpu.readback(&original).unwrap()),
        raw.as_flattened()
    );
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_color_field_boundary_brightness_is_continuous_through_zero() {
    let gpu = Gpu::new().unwrap();
    let width = 48usize;
    let height = 32usize;
    let neutral = [0.284206, 0.604436, 1.0];
    let y = [-2.6575486_f32, 3.3135306, -0.2476];
    let matrix = [
        4.45774, -1.90728, 0.581922, -4.22818, 4.30012, -0.701463, 3.1376, -5.81127, 3.31683,
    ];
    let mut raw = vec![[0.4_f32; 3]; width * height];
    let mut pixels = vec![[0.27_f32, 0.60, 0.94]; width * height];
    for row in 8..24 {
        for col in 24..32 {
            raw[row * width + col] = [1.; 3];
            pixels[row * width + col] = [0.6, 1.2, 2.0];
        }
        let target_y = (row as f32 - 16.) * 0.001;
        pixels[row * width + 23] = [0.9, (target_y - y[0] * 0.9 - y[2] * 0.8) / y[1], 0.8];
    }
    let input = gpu.upload("cross-zero calibrated", &pixels).unwrap();
    let original = gpu.upload("cross-zero original", &raw).unwrap();
    let black = gpu.upload("cross-zero black", &[0_f32; 3]).unwrap();
    for brightness in [1, 2] {
        let prepared = Prepared {
            params: Params {
                size: [width as u32, height as u32, (width * height) as u32, 0],
                grid: [6, 4, 24, 8],
                crop: [0, 0, width as u32, height as u32],
                gain: [1.; 4],
                white: [1.; 4],
                neutral: [neutral[0] as f32, neutral[1] as f32, neutral[2] as f32, 0.],
                luminance: [y[0], y[1], y[2], 0.],
                metric: color_metric(neutral, matrix).unwrap(),
                tuning: [0.035, 0.75, 1., 1.],
                options: [brightness, 1, 0, 0],
            },
            repairs: Vec::new(),
        };
        let output = apply(
            &gpu,
            Inputs {
                input: &input,
                original: &original,
                black: &black,
                unrepaired: &input,
            },
            &prepared,
            &mut Profile::default(),
            &mut Execution::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let values: &[f32] = bytemuck::cast_slice(&bytes);
        assert!(values.iter().all(|v| v.is_finite()));
        let ys: Vec<f32> = (8..24)
            .map(|row| {
                let index = (row * width + 23) * 3;
                values[index..index + 3]
                    .iter()
                    .zip(y)
                    .map(|(m, w)| m * w)
                    .sum()
            })
            .collect();
        let largest_jump = ys
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0_f32, f32::max);
        assert!(
            largest_jump < 0.01,
            "cross-zero discontinuity remains: {ys:?}"
        );
        assert!(ys.iter().all(|v| *v > 0.5), "new black hole: {ys:?}");
        assert_eq!(&values[..3], &pixels[0], "distant healthy sample changed");
    }
}
