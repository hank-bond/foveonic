//! Shapes gamut-relative colorfulness with saturation, saturation contrast and three overlapping tonal bands.

use crate::{
    color_space::{ColorSpace, TiffColorSpace},
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Band {
    pub saturation: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Controls {
    pub saturation: f32,
    pub contrast: f32,
    pub bands: [Band; 3],
}
impl Controls {
    /// Stored processing amounts retain their established strength and recipe meaning.
    pub const SATURATION_RANGE: std::ops::RangeInclusive<f32> = -1. ..=2.;
    pub const SATURATION_CONTROL_RANGE: std::ops::RangeInclusive<f32> = -1. ..=1.;
    /// Balanced control coordinates preserve negative amounts and halve positive amounts.
    pub fn saturation_to_control(amount: f32) -> f32 {
        if amount > 0. {
            amount * 0.5
        } else {
            amount
        }
    }
    pub fn saturation_from_control(control: f32) -> f32 {
        if control > 0. {
            control * 2.
        } else {
            control
        }
    }
    pub fn chromatic_active(self) -> bool {
        self.saturation != 0.
            || self.contrast != 0.
            || self.bands.iter().any(|b| b.saturation != 0.)
    }
    pub fn is_active(self, mono: bool) -> bool {
        !mono && self.chromatic_active()
    }
    pub(crate) fn validate(self) -> Result<()> {
        if std::iter::once(self.saturation)
            .chain(self.bands.map(|b| b.saturation))
            .any(|v| !Self::SATURATION_RANGE.contains(&v))
            || !(-1. ..=1.).contains(&self.contrast)
        {
            return Err("invalid perceptual grading controls".into());
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    /// Dimensions, optional scalar mask (one-based band), and chroma bypass.
    shape: [u32; 4],
    /// Linear code white, log2 middle-gray pivot, saturation contrast, saturation.
    values: [f32; 4],
    bands: [[f32; 4]; 3],
    luma: [f32; 4],
    to_srgb: [[f32; 4]; 3],
    from_srgb: [[f32; 4]; 3],
    /// Shared gamut protection flag and linear code white.
    protection: [f32; 4],
}
impl Params {
    pub fn new(
        extent: [u32; 2],
        space: ColorSpace,
        white: f32,
        pivot: f32,
        c: Controls,
        mono: bool,
    ) -> Self {
        Self {
            shape: [
                extent[0],
                extent[1],
                0,
                u32::from(mono || !c.chromatic_active()),
            ],
            values: [
                white,
                pivot,
                if mono { 0. } else { c.contrast },
                if mono { 0. } else { c.saturation },
            ],
            bands: c
                .bands
                .map(|b| [0., if mono { 0. } else { b.saturation }, 0., 0.]),
            luma: super::photographic::luminance(space),
            to_srgb: TiffColorSpace::Srgb.transform(space),
            from_srgb: TiffColorSpace::from(space).transform(ColorSpace::Srgb),
            protection: [1., white, 0., 0.],
        }
    }
    pub fn with_clipping_protection(mut self, prevent: bool) -> Self {
        self.protection[0] = u32::from(prevent) as f32;
        self
    }
    pub fn with_mask(mut self, band: u32) -> Self {
        self.shape[2] = band + 1;
        self
    }
    pub fn is_mask(self) -> bool {
        self.shape[2] != 0
    }
    pub fn name(self) -> &'static str {
        match self.shape[2] {
            1 => "perceptual.shadows",
            2 => "perceptual.midtones",
            3 => "perceptual.highlights",
            _ => "perceptual.rgb",
        }
    }
    pub fn key(self) -> Key {
        Key::pod(&self)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    mut p: Params,
    extent: [u32; 2],
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let count = u64::from(extent[0]) * u64::from(extent[1]);
    if input.size() != count * 12 {
        return Err("perceptual grading geometry mismatch".into());
    }
    p.shape[..2].copy_from_slice(&extent);
    let mut output = gpu.storage(p.name(), count * if p.is_mask() { 4 } else { 12 })?;
    let uniform = gpu.uniform(&p);
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::PerceptualGrade,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "perceptual_grade_test.rs"]
mod tests;
