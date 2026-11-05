//! Neutralizes sampled white points and applies camera-relative warmth and tint in a shared cone-response basis.

use crate::color_space::{inverse, ColorSpace, TiffColorSpace};
use crate::source::Result;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Controls {
    /// Sampled neutral in D50 XYZ, normalized to Y=1, before this adjustment.
    pub neutral: Option<[f32; 3]>,
    pub warmth: f32,
    pub tint: f32,
}
impl Controls {
    pub fn is_neutral(self) -> bool {
        self.neutral.is_none() && self.warmth == 0. && self.tint == 0.
    }
    pub fn validate(self) -> Result<()> {
        if let Some(xyz) = self.neutral {
            Self::from_sample(xyz)?;
            if (xyz[1] - 1.).abs() > 1e-5 {
                return Err("Sampled white balance must be normalized to Y=1.".into());
            }
        }
        if [self.warmth, self.tint]
            .iter()
            .any(|v| !(-1. ..=1.).contains(v))
        {
            return Err("white-balance warmth and tint must be between -1 and 1".into());
        }
        Ok(())
    }
    /// Neutralize a linear patch without fitting a Kelvin temperature or changing exposure.
    pub fn from_sample(xyz: [f32; 3]) -> Result<Self> {
        if xyz.iter().any(|v| !v.is_finite() || *v <= 0.) {
            return Err("Choose an area with a reliable neutral color.".into());
        }
        let xyz = xyz.map(|v| v / xyz[1]);
        let cone = apply(BRADFORD, xyz.map(f64::from));
        if cone.iter().any(|v| !v.is_finite() || *v <= 0.) {
            return Err("This color cannot define a reliable white balance.".into());
        }
        Ok(Self {
            neutral: Some(xyz),
            ..Default::default()
        })
    }
    /// Positive warmth reduces the short-wave cone response. Positive tint biases magenta.
    /// These are relative offsets, not an estimate of a recorded color temperature.
    pub(crate) fn matrix(self, space: ColorSpace) -> [f64; 9] {
        if self.is_neutral() {
            return [1., 0., 0., 0., 1., 0., 0., 0., 1.];
        }
        let rgb_to_cone = multiply(BRADFORD, TiffColorSpace::from(space).to_xyz());
        let mut gains = [
            (f64::from(self.warmth) * 0.25 + f64::from(self.tint) * 0.12).exp(),
            (-f64::from(self.tint) * 0.12).exp(),
            (-f64::from(self.warmth) * 0.25 + f64::from(self.tint) * 0.12).exp(),
        ];
        if let Some(xyz) = self.neutral {
            let measured = apply(BRADFORD, xyz.map(f64::from));
            let white = apply(rgb_to_cone, [1.; 3]);
            for c in 0..3 {
                gains[c] *= white[c] / measured[c];
            }
        }
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
const BRADFORD: [f64; 9] = [
    0.8951, 0.2664, -0.1614, -0.7502, 1.7135, 0.0367, 0.0389, -0.0685, 1.0296,
];
fn apply(m: [f64; 9], v: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|r| (0..3).map(|c| m[r * 3 + c] * v[c]).sum())
}
fn multiply(a: [f64; 9], b: [f64; 9]) -> [f64; 9] {
    std::array::from_fn(|i| (0..3).map(|k| a[i / 3 * 3 + k] * b[k * 3 + i % 3]).sum())
}

/// Patch sampling is numerical and independent of source metadata and presentation state.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct PickParams {
    shape: [u32; 4],
    crop: [u32; 4],
    xyz: [[f32; 4]; 3],
    white: [f32; 4],
}
impl PickParams {
    pub fn new(
        extent: [u32; 2],
        space: ColorSpace,
        white: f32,
        pixel: [u32; 2],
        crop: [u32; 4],
    ) -> Self {
        let xyz = TiffColorSpace::from(space).to_xyz();
        Self {
            shape: [extent[0], extent[1], pixel[0], pixel[1]],
            crop,
            xyz: std::array::from_fn(|r| {
                std::array::from_fn(|c| if c < 3 { xyz[r * 3 + c] as f32 } else { 0. })
            }),
            white: [white, 0., 0., 0.],
        }
    }
}

pub(crate) fn pick(
    gpu: &crate::gpu::Gpu,
    input: &crate::gpu::ReadGuard,
    params: PickParams,
) -> Result<Controls> {
    use crate::gpu::{
        Binding::{Read, Write},
        Kernel,
    };
    let uniform = gpu.uniform(&params);
    let mut output = gpu.storage("White balance patch", 16)?;
    let mut trace = gpu.trace(false, 1);
    gpu.dispatch(
        Kernel::WhiteBalancePick,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
        ],
        [1, 1],
        &mut trace,
    );
    let bytes = gpu.readback(&output)?;
    let values: &[f32] = bytemuck::cast_slice(&bytes);
    if values[3] < 9. {
        return Err("Choose a brighter neutral area with measurable color in each channel.".into());
    }
    Controls::from_sample([values[0], values[1], values[2]])
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
                neutral: None,
                warmth: 1.,
                tint: 0.,
            }
            .matrix(space);
            let white: [f64; 3] = std::array::from_fn(|r| m[r * 3..r * 3 + 3].iter().sum());
            assert!(white[0] > white[2]);
            assert!(m.iter().all(|v| v.is_finite()));
            let positive = Controls {
                neutral: None,
                warmth: 0.7,
                tint: -0.4,
            }
            .matrix(space);
            let negative = Controls {
                neutral: None,
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
            neutral: None,
            warmth: f32::NAN,
            tint: 0.
        }
        .validate()
        .is_err());
    }
    #[test]
    fn sampled_neutral_preserves_brightness_and_survives_recipe_roundtrip() {
        for space in [
            ColorSpace::Srgb,
            ColorSpace::AdobeRgb,
            ColorSpace::ProPhotoRgb,
        ] {
            let rgb = [0.4, 0.3, 0.2];
            let xyz = apply(TiffColorSpace::from(space).to_xyz(), rgb);
            let controls = Controls::from_sample(xyz.map(|v| v as f32)).unwrap();
            let balanced = apply(controls.matrix(space), rgb);
            for channel in balanced {
                assert!((channel - xyz[1]).abs() < 1e-6);
            }
            let settings = crate::render::Settings {
                white_balance_bias: controls,
                ..Default::default()
            };
            assert_eq!(
                serde_json::from_str::<crate::render::Settings>(
                    &serde_json::to_string(&settings).unwrap()
                )
                .unwrap(),
                settings
            );
            for family in [
                crate::source::ProcessingFamily::Classic,
                crate::source::ProcessingFamily::Merrill,
                crate::source::ProcessingFamily::Quattro,
            ] {
                let mut target = crate::render::Settings::default();
                crate::render::EditScope::Tool(crate::render::EditSection::WhiteBalance).apply(
                    &settings,
                    &mut target,
                    family,
                );
                assert_eq!(target.white_balance_bias, controls);
            }
        }
        for xyz in [
            [0.; 3],
            [f32::NAN, 1., 1.],
            [1., f32::INFINITY, 1.],
            [1., -1., 1.],
        ] {
            assert!(Controls::from_sample(xyz).is_err());
        }
    }
}
