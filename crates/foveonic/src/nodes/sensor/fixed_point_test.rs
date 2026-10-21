//! Checks ordered flare accumulation across cooperative tiles and signed Q14 rounding.

use super::*;
use crate::gpu::{
    Binding::{Read, Write},
    Gpu, Kernel,
};

#[test]
#[ignore = "requires a working GPU adapter"]
fn flare_tiles_preserve_sequential_float_rounding() {
    for height in [34_u32, 514_u32, 2050_u32] {
        let width = 64_u32;
        let values: Vec<f32> = (0..width * height * 3)
            .map(|i| match i % 3 {
                0 => (30000 - (i * 17 % 47)) as f32,
                1 => -((20000 + (i * 13 % 31)) as f32),
                _ => (1000 + (i * 7 % 113)) as f32,
            })
            .collect();
        let flare = [0.03_f32, 0.07, -0.013, 0.];
        let mut sums = [0_f32; 3];
        let mut exact_sums = [0_i64; 3];
        let mut count = 0;
        for y in (2..height).step_by(4) {
            for x in (2..width).step_by(4) {
                for c in 0..3 {
                    let value = values[((y * width + x) * 3) as usize + c];
                    sums[c] += value;
                    exact_sums[c] += value as i64;
                }
                count += 1;
            }
        }
        let offset = |sum: f32, c: usize| {
            let average = sum / count as f32;
            let factor = flare[c] * 16384.;
            (f64::from(average) * f64::from(factor)) as i32
        };
        let expected: [i32; 3] = std::array::from_fn(|c| offset(sums[c], c));
        let regrouped: [i32; 3] = std::array::from_fn(|c| offset(exact_sums[c] as f32, c));
        if count > 512 {
            assert_ne!(
                expected, regrouped,
                "fixture must distinguish reordered summation"
            );
        }
        let params = CorrectionParams {
            source: [width, height, 2, 0],
            flare,
            final_gain: [0.; 4],
            linear: [0; 4],
            quadratic: [0; 4],
            segment_scale: [0; 4],
            segment_bias: [0; 4],
            controls: [0; 4],
            ..Default::default()
        };
        let gpu = Gpu::new().unwrap();
        let input = gpu.upload("Ordered flare fixture", &values).unwrap();
        let uniform = gpu.uniform(&params);
        let mut output = gpu.storage("Measured flare offsets", 12).unwrap();
        gpu.dispatch(
            Kernel::FixedPointStatistics,
            &mut [
                (0, Read(&input)),
                (3, Read(&uniform)),
                (5, Write(output.write())),
            ],
            [3, 1],
            &mut gpu.trace(false, 1),
        );
        let bytes = gpu.readback(&output).unwrap();
        assert_eq!(bytemuck::cast_slice::<u8, i32>(&bytes), expected);
    }
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn disabled_scaling_matches_unit_multiplier_without_modifying_input() {
    let gpu = Gpu::new().unwrap();
    let values: Vec<f32> = (0..7 * 5 * 3).map(|i| (i - 60) as f32).collect();
    let input = gpu.upload("Integral correction samples", &values).unwrap();
    let mut prepared = Correction {
        params: CorrectionParams {
            source: [7, 5, 2, 0],
            flare: [0.; 4],
            final_gain: [1.; 4],
            linear: [0; 4],
            quadratic: [0; 4],
            segment_scale: [0; 4],
            segment_bias: [0; 4],
            controls: [0; 4],
            ..Default::default()
        },
        knots: vec![1.; 5 * 2 * 3],
    };
    let unchanged = apply_correction(&gpu, &input, &prepared, &mut Default::default()).unwrap();
    prepared.params.source[3] = 16384;
    let unit = apply_correction(&gpu, &input, &prepared, &mut Default::default()).unwrap();
    assert_eq!(
        gpu.readback(&unchanged).unwrap(),
        gpu.readback(&unit).unwrap()
    );
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&values)
    );
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn drift_polynomial_and_fractional_knots_match_signed_scalar() {
    let gpu = Gpu::new().unwrap();
    let width = 17_u32;
    let height = 13_u32;
    let source: Vec<f32> = (0..width * height * 3)
        .map(|i| ((i * 71 % 1201) as i32 - 400) as f32)
        .collect();
    let input = gpu.upload("Signed polynomial fixture", &source).unwrap();
    for multiplier in [0, 31948] {
        let mut p = CorrectionParams {
            source: [width, height, 4, multiplier],
            final_gain: [1., 0.5, 2., 0.],
            linear: [4096, -2048, 8192, 0],
            quadratic: [2048, 4096, -2048, 0],
            segment_scale: [2048, 4096, -2048, 0],
            segment_bias: [49152, -81920, 114688, 0],
            controls: [3, 9, 0, 0],
            drift_controls: [0, height, 0, 1],
            ..Default::default()
        };
        let drift = [
            2.123456789,
            -1.23456789,
            0.333333333,
            3.123456789,
            -2.987654321,
            5.3456789,
            -0.987654321,
            0.87654321,
            -0.22222222,
            2.0123456789,
            1.87654321,
            -4.123456789,
        ];
        for (i, value) in drift.into_iter().enumerate() {
            p.drift[i / 2][i % 2 * 2] = value as f32;
            p.drift[i / 2][i % 2 * 2 + 1] = (value - f64::from(value as f32)) as f32;
        }
        for c in 0..3 {
            for term in 0..9 {
                p.polynomial[c * 3 + term / 3][term % 3] = ((c * 9 + term) as i32 * 7 % 9 - 4) * 64;
            }
        }
        let knots: Vec<f32> = (0..height * 4 * 3)
            .map(|i| 0.75 + (i * 7 % 13) as f32 / 16.)
            .collect();
        let prepared = Correction { params: p, knots };
        let output = apply_correction(&gpu, &input, &prepared, &mut Default::default()).unwrap();
        let actual = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&actual);
        let mut expected = Vec::new();
        let scaled: Vec<i32> = source
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let c = i % 3;
                if multiplier == 0 {
                    *v as i32
                } else {
                    let black = ((drift[c + 3] + drift[c + 9]) * 0.5) as i32;
                    ((((*v as i32 - black) as u32).wrapping_mul(multiplier) >> 14) as i16) as i32
                }
            })
            .collect();
        let spacing = width as f32 / 3.;
        for y in 0..height {
            let fraction = f64::from(y as f32 / (height - 1) as f32);
            let row_drift: [f64; 6] =
                std::array::from_fn(|c| drift[c] + (drift[c + 6] - drift[c]) * fraction);
            let mut boundary = f64::from(spacing);
            let mut segment = 0;
            let gains = |segment: usize| -> ([i32; 3], [i32; 3]) {
                let at = y as usize * 12 + segment * 3;
                let factors = p.final_gain.map(|v| v * 4194304.);
                (
                    std::array::from_fn(|c| (prepared.knots[at + c] * factors[c]) as i32),
                    std::array::from_fn(|c| {
                        (((prepared.knots[at + 3 + c] - prepared.knots[at + c]) / spacing)
                            * factors[c]) as i32
                    }),
                )
            };
            let (mut current, mut increment) = gains(segment);
            for x in 0..width {
                if f64::from(x) >= boundary {
                    segment += 1;
                    boundary += f64::from(spacing);
                    (current, increment) = gains(segment);
                }
                let mut values = [0_i32; 3];
                for c in 0..3 {
                    let sample = scaled[((y * width + x) * 3) as usize + c];
                    let previous = scaled[((y * width + x.saturating_sub(1)) * 3) as usize + c];
                    let delta = sample - previous;
                    let mut z = sample
                        .wrapping_mul(16384)
                        .wrapping_add(delta.wrapping_mul(p.linear[c]))
                        .wrapping_add(
                            ((delta.wrapping_mul(delta) as u32 >> 14) as i32)
                                .wrapping_mul(p.quadratic[c]),
                        );
                    if x % 3 == 0 {
                        z = z
                            .wrapping_add(previous.wrapping_mul(p.segment_scale[c]))
                            .wrapping_add(p.segment_bias[c]);
                    }
                    let step = (row_drift[c] / f64::from(width) * 16384.) as i32;
                    let drift_q = (row_drift[c + 3] * 16384.) as i32 - (width / 2) as i32 * step
                        + x as i32 * step;
                    values[c] = z.wrapping_sub(drift_q) >> 14;
                }
                let [r, g, b] = values;
                let squares = values.map(|v| (v.wrapping_mul(v) as u32 >> 14) as i32);
                let terms = [
                    squares[0],
                    squares[1],
                    squares[2],
                    r.wrapping_mul(g) >> 14,
                    b.wrapping_mul(r) >> 14,
                    b.wrapping_mul(g) >> 14,
                    squares[0].wrapping_mul(r) >> 14,
                    squares[1].wrapping_mul(g) >> 14,
                    squares[2].wrapping_mul(b) >> 14,
                ];
                for c in 0..3 {
                    let sum = terms.iter().enumerate().fold(0_i32, |sum, (term, v)| {
                        sum.wrapping_add(v.wrapping_mul(p.polynomial[c * 3 + term / 3][term % 3]))
                    });
                    let value = values[c].wrapping_add(sum >> 14);
                    let gained = value.wrapping_mul(current[c] >> 8) >> 14;
                    expected.push((gained.min(32000) as i16) as f32);
                    current[c] = current[c].wrapping_add(increment[c]);
                }
            }
        }
        assert_eq!(actual, expected, "multiplier {multiplier}");
    }
}
