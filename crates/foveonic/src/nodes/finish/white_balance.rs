//! Prepares camera-relative warmth and tint in a shared cone-response basis without touching pixels on the CPU.

use crate::color_space::{inverse, ColorSpace, TiffColorSpace};
use crate::source::Result;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Controls {
    pub warmth: f32,
    pub tint: f32,
}
impl Controls {
    pub fn is_neutral(self) -> bool {
        self.warmth == 0. && self.tint == 0.
    }
    pub fn validate(self) -> Result<()> {
        if [self.warmth, self.tint]
            .iter()
            .any(|v| !(-1. ..=1.).contains(v))
        {
            return Err("white-balance warmth and tint must be between -1 and 1".into());
        }
        Ok(())
    }
    /// Positive warmth reduces the short-wave cone response. Positive tint biases magenta.
    /// These are relative offsets, not an estimate of a recorded color temperature.
    pub(crate) fn matrix(self, space: ColorSpace) -> [f64; 9] {
        if self.is_neutral() {
            return [1., 0., 0., 0., 1., 0., 0., 0., 1.];
        }
        let bradford = [
            0.8951, 0.2664, -0.1614, -0.7502, 1.7135, 0.0367, 0.0389, -0.0685, 1.0296,
        ];
        let rgb_to_cone = multiply(bradford, TiffColorSpace::from(space).to_xyz());
        let gains = [
            (f64::from(self.warmth) * 0.25 + f64::from(self.tint) * 0.12).exp(),
            (-f64::from(self.tint) * 0.12).exp(),
            (-f64::from(self.warmth) * 0.25 + f64::from(self.tint) * 0.12).exp(),
        ];
        let scaled = std::array::from_fn(|i| rgb_to_cone[i] * gains[i / 3]);
        multiply(inverse(rgb_to_cone), scaled)
    }
    pub(crate) fn rows(self, space: ColorSpace) -> [[f32; 4]; 3] {
        let m = self.matrix(space);
        std::array::from_fn(|r| {
            std::array::from_fn(|c| if c < 3 { m[r * 3 + c] as f32 } else { 0. })
        })
    }
}
fn multiply(a: [f64; 9], b: [f64; 9]) -> [f64; 9] {
    std::array::from_fn(|i| (0..3).map(|k| a[i / 3 * 3 + k] * b[k * 3 + i % 3]).sum())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn biases_are_neutral_at_zero_and_consistent_across_working_primaries() {
        for space in [
            ColorSpace::Srgb,
            ColorSpace::AdobeRgb,
            ColorSpace::ProPhotoRgb,
        ] {
            assert_eq!(
                Controls::default().matrix(space),
                [1., 0., 0., 0., 1., 0., 0., 0., 1.]
            );
            let m = Controls {
                warmth: 1.,
                tint: 0.,
            }
            .matrix(space);
            let white: [f64; 3] = std::array::from_fn(|r| m[r * 3..r * 3 + 3].iter().sum());
            assert!(white[0] > white[2]);
            assert!(m.iter().all(|v| v.is_finite()));
            let positive = Controls {
                warmth: 0.7,
                tint: -0.4,
            }
            .matrix(space);
            let negative = Controls {
                warmth: -0.7,
                tint: 0.4,
            }
            .matrix(space);
            for (a, b) in multiply(positive, negative)
                .into_iter()
                .zip(Controls::default().matrix(space))
            {
                assert!((a - b).abs() < 1e-10);
            }
        }
        assert!(Controls {
            warmth: f32::NAN,
            tint: 0.
        }
        .validate()
        .is_err());
    }
}
