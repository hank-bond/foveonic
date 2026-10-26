//! Defines shared photographic controls and the GPU parameter layout for normalized RGB finishing.

use crate::{
    color_space::{ColorSpace, TiffColorSpace},
    recipe::Key,
    source::Result,
};

// Three Gaussian standard deviations bound the sharpening filter footprint.
pub(super) const GAUSSIAN_SUPPORT_SIGMAS: f32 = 3.;
pub(crate) mod local_contrast;
pub(crate) mod sensitivity;
pub(crate) mod sharpen;
pub(crate) mod tone;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    #[default]
    Reference,
    Universal,
}
impl Method {
    pub fn label(self) -> &'static str {
        match self {
            Self::Reference => "Reference",
            Self::Universal => "Universal",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sharpening {
    pub method: Method,
    pub amount: f32,
    /// Gaussian sigma in native image pixels.
    pub radius: f32,
    /// Minimum high-pass difference in square-root brightness.
    pub threshold: f32,
    pub halo_suppression: f32,
}
impl Default for Sharpening {
    fn default() -> Self {
        Self {
            method: Method::Reference,
            amount: 0.,
            radius: 0.8,
            threshold: 0.01,
            halo_suppression: 0.5,
        }
    }
}
impl Sharpening {
    pub fn is_active(self) -> bool {
        self.method == Method::Universal && self.amount != 0.
    }
    pub(crate) fn validate(self) -> Result<()> {
        if !(0. ..=4.).contains(&self.amount)
            || !(0.3..=4.).contains(&self.radius)
            || !(0. ..=0.2).contains(&self.threshold)
            || !(0. ..=1.).contains(&self.halo_suppression)
        {
            return Err("invalid universal sharpening controls".into());
        }
        Ok(())
    }
    pub(crate) fn support(self, scale: f32) -> u32 {
        (GAUSSIAN_SUPPORT_SIGMAS * self.radius * scale).ceil() as u32 + 1
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Controls {
    pub tone_method: Method,
    /// Red, Yellow, Green, Cyan, Blue and Magenta brightness in normalized amounts.
    pub monochrome_sensitivity: [f32; 6],
    /// Neutral-centered multiscale local contrast, independent of reconstruction.
    pub local_contrast: f32,
    pub sharpening: Sharpening,
}
impl Controls {
    pub fn sensitivity_active(self) -> bool {
        self.monochrome_sensitivity != [0.; 6]
    }
    pub(crate) fn validate(self) -> Result<()> {
        if self
            .monochrome_sensitivity
            .iter()
            .chain([&self.local_contrast])
            .any(|v| !(-1. ..=1.).contains(v))
        {
            return Err("photographic sensitivity and local contrast must be in -1..1".into());
        }
        self.sharpening.validate()
    }
}

/// Finishing works in normalized, linear-light photographic RGB. Colorants
/// preserve brightness and give hue selection a fixed sRGB interpretation.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Params {
    /// Width, height, spatial stride/radius and axis for filter dispatches.
    pub shape: [u32; 4],
    /// Tone: contrast/saturation. Sharpen: amount/threshold/halo/sigma.
    /// Local contrast: amount/native-grid scale. Unused fields stay zero.
    pub values: [f32; 4],
    pub luma: [f32; 4],
    pub hues: [[f32; 4]; 2],
    pub guide_matrix: [[f32; 4]; 3],
}
impl Params {
    pub fn new(extent: [u32; 2], space: ColorSpace) -> Self {
        Self {
            shape: [extent[0], extent[1], 0, 0],
            values: [0.; 4],
            luma: luminance(space),
            hues: [[0.; 4]; 2],
            guide_matrix: TiffColorSpace::Srgb.transform(space),
        }
    }
    pub fn key(self) -> Key {
        Key::pod(&self)
    }

    pub(crate) fn tone(
        extent: [u32; 2],
        space: ColorSpace,
        contrast: f64,
        saturation: f32,
    ) -> Self {
        // UI contrast spans -2..2. The shared cubic uses a normalized -1..1 amount.
        const CONTRAST_CONTROL_SCALE: f32 = 0.5;
        let mut p = Self::new(extent, space);
        p.values = [contrast as f32 * CONTRAST_CONTROL_SCALE, saturation, 0., 0.];
        p
    }

    pub(crate) fn sensitivity(
        extent: [u32; 2],
        space: ColorSpace,
        guide_space: ColorSpace,
        hues: [f32; 6],
    ) -> Self {
        let mut p = Self::new(extent, space);
        p.guide_matrix = TiffColorSpace::Srgb.transform(guide_space);
        for (i, value) in hues.into_iter().enumerate() {
            p.hues[i / 4][i % 4] = value;
        }
        p
    }

    pub(crate) fn sharpening(
        extent: [u32; 2],
        space: ColorSpace,
        controls: Sharpening,
        scale: f32,
    ) -> Self {
        const MIN_SIGMA: f32 = 0.3;
        let mut p = Self::new(extent, space);
        p.values = [
            controls.amount,
            controls.threshold,
            controls.halo_suppression,
            (controls.radius * scale).max(MIN_SIGMA),
        ];
        p
    }

    pub(crate) fn local_contrast(
        extent: [u32; 2],
        space: ColorSpace,
        amount: f32,
        scale: f32,
    ) -> Self {
        let mut p = Self::new(extent, space);
        p.values = [amount, scale, 0., 0.];
        p
    }
}

pub(crate) fn luminance(space: ColorSpace) -> [f32; 4] {
    // D50-adapted Y colorants from the same embedded profiles as output mapping.
    match space {
        ColorSpace::Srgb => [14578., 46985., 3973., 0.],
        ColorSpace::AdobeRgb => [20389., 41004., 4143., 0.],
        ColorSpace::ProPhotoRgb => [18878., 46653., 5., 0.],
    }
    .map(|v| v / 65536.)
}

#[cfg(test)]
#[path = "photographic_test.rs"]
mod tests;

fn validate_geometry(input: &crate::gpu::ReadGuard, extent: [u32; 2]) -> Result<()> {
    if input.size() != u64::from(extent[0]) * u64::from(extent[1]) * 12 {
        return Err("photographic finishing input geometry mismatch".into());
    }
    Ok(())
}
