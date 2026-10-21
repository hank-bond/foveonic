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
