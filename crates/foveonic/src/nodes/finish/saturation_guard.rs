//! Samples the native saturation shoulder for DNG appearance fitting.

/// Match saturation_guard.wgsl for sparse profile samples, before RGB transport.
pub(crate) fn protect(input: [f64; 3], candidate: [f64; 3], neutral: f64, white: f64) -> [f64; 3] {
    let ray = input.map(|v| v - neutral);
    let magnitude: f64 = ray.iter().map(|v| v * v).sum();
    if magnitude <= 1e-12 * white * white {
        return candidate;
    }
    let strength: f64 = (0..3)
        .map(|c| (candidate[c] - input[c]) * ray[c])
        .sum::<f64>()
        / magnitude;
    if strength <= 0. {
        return candidate;
    }
    let lower = input.into_iter().fold(neutral.min(0.), f64::min);
    let upper = input.into_iter().fold(neutral.max(white), f64::max);
    let occupancy = ray
        .into_iter()
        .map(|v| {
            if v > 0. {
                v / (upper - neutral).max(1e-12 * white)
            } else if v < 0. {
                -v / (neutral - lower).max(1e-12 * white)
            } else {
                0.
            }
        })
        .fold(0., f64::max)
        .clamp(0., 1.);
    if occupancy == 1. {
        return input;
    }
    let attenuation = (1. - occupancy) / (1. + strength * occupancy);
    std::array::from_fn(|c| input[c] + (candidate[c] - input[c]) * attenuation)
}

#[derive(Clone, Copy)]
pub(crate) struct Reference {
    pub factor: f64,
    pub protect: bool,
}
impl Default for Reference {
    fn default() -> Self {
        Self {
            factor: 1.,
            protect: false,
        }
    }
}
impl Reference {
    pub(crate) fn apply(self, input: [f64; 3]) -> [f64; 3] {
        if self.factor == 1. {
            return input;
        }
        let neutral = input[0] * 0.299 + input[1] * 0.587 + input[2] * 0.114;
        let candidate =
            std::array::from_fn(|c| neutral * (1. - self.factor) + input[c] * self.factor);
        if self.protect && self.factor > 1. {
            protect(input, candidate, neutral, 1.)
        } else {
            candidate
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_guard_shoulders_boosts_preserves_existing_extremes_and_leaves_desaturation_alone() {
        for input in [
            [0.7, 0.3, 0.2],
            [0.99, 0.15, 0.05],
            [0.2; 3],
            [-0.1, 0.3, 1.2],
        ] {
            let unlocked = Reference {
                factor: 2.,
                protect: false,
            }
            .apply(input);
            let protected = Reference {
                factor: 2.,
                protect: true,
            }
            .apply(input);
            let lower = input.into_iter().fold(0., f64::min);
            let upper = input.into_iter().fold(1., f64::max);
            for c in 0..3 {
                assert!(protected[c] >= lower - 1e-12 && protected[c] <= upper + 1e-12);
                assert!((protected[c] - input[c]).abs() <= (unlocked[c] - input[c]).abs() + 1e-12);
            }
            assert_eq!(
                Reference {
                    factor: 0.5,
                    protect: true
                }
                .apply(input),
                Reference {
                    factor: 0.5,
                    protect: false
                }
                .apply(input)
            );
            assert_eq!(
                Reference {
                    factor: 1.,
                    protect: true
                }
                .apply(input),
                input
            );
        }
        let input = [1., 0.2, 0.1];
        assert_eq!(
            Reference {
                factor: 2.,
                protect: true
            }
            .apply(input),
            input
        );
        let muted = [0.5, 0.45, 0.4];
        assert!(
            Reference {
                factor: 2.,
                protect: true
            }
            .apply(muted)[0]
                > muted[0]
        );
    }

    #[test]
    #[ignore = "requires a GPU"]
    fn gpu_reference_guard_matches_profile_samples_in_both_modes() {
        use crate::{
            gpu::{
                Binding::{Read, Write},
                Gpu, Kernel,
            },
            nodes::sensor::calibration::ColorParams,
            profile::Profile,
        };
        let gpu = Gpu::new().unwrap();
        let samples = [
            [0.7_f32, 0.3, 0.2],
            [0.99, 0.15, 0.05],
            [1., 0.2, 0.1],
            [-0.1, 0.3, 1.2],
            [0.4; 3],
        ];
        let values: Vec<f32> = samples.into_iter().flatten().collect();
        let input = gpu.upload("saturation guard samples", &values).unwrap();
        for protect in [false, true] {
            for factor in [0.5_f32, 1., 2.] {
                let params = ColorParams {
                    source: [
                        samples.len() as u32,
                        1,
                        u32::from(protect && factor > 1.),
                        0,
                    ],
                    matrix: std::array::from_fn(|r| {
                        std::array::from_fn(|c| {
                            if c == 3 {
                                0.
                            } else {
                                [0.299, 0.587, 0.114][c] * (1. - factor)
                                    + if r == c { factor } else { 0. }
                            }
                        })
                    }),
                };
                let uniform = gpu.uniform(&params);
                let mut output = gpu
                    .storage("saturation guard result", input.size())
                    .unwrap();
                let mut trace = gpu.trace(false, 1);
                gpu.dispatch(
                    Kernel::ReferenceSaturation,
                    &mut [
                        (0, Read(&input)),
                        (1, Read(&uniform)),
                        (2, Write(output.write())),
                    ],
                    [1, 1],
                    &mut trace,
                );
                gpu.finish_trace(&trace, &mut Profile::default()).unwrap();
                let bytes = gpu.readback(&output).unwrap();
                let actual: &[f32] = bytemuck::cast_slice(&bytes);
                for (i, sample) in samples.into_iter().enumerate() {
                    let expected = Reference {
                        factor: f64::from(factor),
                        protect,
                    }
                    .apply(sample.map(f64::from));
                    for c in 0..3 {
                        assert!((f64::from(actual[i * 3 + c]) - expected[c]).abs() < 2e-6);
                    }
                }
            }
        }
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&values)
        );
    }
}
