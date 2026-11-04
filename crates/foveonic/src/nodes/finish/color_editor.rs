//! Edits linear Hue, Saturation and Lightness and picks subject ranges from the same cleaned color guide.

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

/// One selected color range. Zero leaves its hue, saturation and lightness unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Adjustment {
    pub hue: f32,
    pub saturation: f32,
    pub lightness: f32,
}
impl Adjustment {
    pub(crate) fn validate(self) -> Result<()> {
        if !(-1. ..=1.).contains(&self.hue)
            || !super::perceptual_grade::Controls::SATURATION_RANGE.contains(&self.saturation)
            || !(-1. ..=1.).contains(&self.lightness)
        {
            return Err(
                "Color Range requires Hue/Lightness in -1..1 and Saturation in -1..2".into(),
            );
        }
        Ok(())
    }
}

/// Red, Orange, Yellow, Green, Cyan, Blue, Purple and Magenta, plus all colors.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Controls {
    pub bands: [Adjustment; 8],
    pub master: Adjustment,
    pub advanced: super::advanced_color::Controls,
}
impl Controls {
    pub const LABELS: [&'static str; 9] = [
        "Red",
        "Orange",
        "Yellow",
        "Green",
        "Cyan",
        "Blue",
        "Purple",
        "Magenta",
        "All colors",
    ];
    /// Set Lightness independently in the eight displayed color ranges.
    pub fn from_lightness(values: [f32; 8]) -> Self {
        Self {
            bands: values.map(|lightness| Adjustment {
                lightness,
                ..Default::default()
            }),
            ..Default::default()
        }
    }
    pub fn chromatic_active(self) -> bool {
        self.advanced.chromatic_active() || self.basic_chromatic_active()
    }
    fn basic_chromatic_active(self) -> bool {
        self.bands
            .into_iter()
            .chain([self.master])
            .any(|v| v.hue != 0. || v.saturation != 0.)
    }
    pub fn is_active(self, monochrome: bool) -> bool {
        self.advanced.is_active(monochrome) || self.basic_active(monochrome)
    }
    pub fn basic_active(self, monochrome: bool) -> bool {
        self.bands
            .into_iter()
            .chain([self.master])
            .any(|v| v.lightness != 0.)
            || (!monochrome && self.basic_chromatic_active())
    }
    pub(crate) fn validate(self) -> Result<()> {
        self.advanced.validate()?;
        for adjustment in self.bands.into_iter().chain([self.master]) {
            adjustment.validate()?;
        }
        Ok(())
    }
}
/// Native-pixel support of the separable color-adjustment filter.
pub(crate) const NATIVE_RADIUS: u32 = 8;

#[cfg(test)]
#[path = "color_editor_test.rs"]
mod tests;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    /// Native dimensions, filter radius and pass axis.
    shape: [u32; 4],
    /// Guide code white, chromatic-map flag, main RGB code white and reserved.
    values: [f32; 4],
    bands: [[f32; 4]; 8],
    master: [f32; 4],
    luma: [f32; 4],
    guide_matrix: [[f32; 4]; 3],
    to_srgb: [[f32; 4]; 3],
    from_srgb: [[f32; 4]; 3],
    /// Shared gamut protection flag and linear code white.
    protection: [f32; 4],
}
impl Params {
    pub fn new(
        extent: [u32; 2],
        controls: Controls,
        space: ColorSpace,
        white: f32,
        guide_white: f32,
        scale: f32,
        monochrome: bool,
    ) -> Self {
        let pack = |v: Adjustment| {
            [
                if monochrome { 0. } else { v.hue },
                if monochrome { 0. } else { v.saturation },
                v.lightness,
                0.,
            ]
        };
        Self {
            shape: [
                extent[0],
                extent[1],
                (NATIVE_RADIUS as f32 * scale).ceil().max(1.) as u32,
                0,
            ],
            values: [
                guide_white,
                u32::from(!monochrome && controls.basic_chromatic_active()) as f32,
                white,
                0.,
            ],
            bands: controls.bands.map(pack),
            master: pack(controls.master),
            luma: crate::nodes::finish::photographic::luminance(space),
            guide_matrix: guide_matrix(space),
            to_srgb: TiffColorSpace::Srgb.transform(space),
            from_srgb: TiffColorSpace::from(space).transform(ColorSpace::Srgb),
            protection: [1., white, 0., 0.],
        }
    }
    pub fn with_clipping_protection(mut self, prevent: bool) -> Self {
        self.protection[0] = u32::from(prevent) as f32;
        self
    }
    pub fn key(self) -> Key {
        Key::pod(&self)
    }
}

pub(super) fn guide_matrix(space: ColorSpace) -> [[f32; 4]; 3] {
    // D50 colorants of the researched linear reference working profile.
    // The profile's Adobe name alone does not identify these exact colorants.
    const REFERENCE_COLORANTS: [f64; 9] = [
        42294., 11865., 9030., 21808., 40115., 3611., 1982., 4520., 47557.,
    ];
    const ICC_FIXED_SCALE: f64 = (1_u32 << 16) as f64;
    let inverse = crate::color_space::inverse(REFERENCE_COLORANTS.map(|v| v / ICC_FIXED_SCALE));
    let source = TiffColorSpace::from(space).to_xyz();
    std::array::from_fn(|r| {
        std::array::from_fn(|c| {
            if c < 3 {
                (0..3)
                    .map(|k| inverse[r * 3 + k] * source[k * 3 + c])
                    .sum::<f64>() as f32
            } else {
                0.
            }
        })
    })
}

fn pass(
    gpu: &Gpu,
    kernel: Kernel,
    input: &ReadGuard,
    auxiliary: Option<&ReadGuard>,
    params: Params,
    rgb: bool,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let extent = [params.shape[0], params.shape[1]];
    let pixels = u64::from(extent[0]) * u64::from(extent[1]);
    let mut output = gpu.storage(
        kernel.label(),
        pixels
            * if rgb {
                12
            } else if params.values[1] != 0. {
                16
            } else {
                4
            },
    )?;
    let uniform = gpu.uniform(&params);
    let mut bindings = vec![
        (0, Read(input)),
        (1, Read(&uniform)),
        (2, Write(output.write())),
    ];
    if let Some(auxiliary) = auxiliary {
        bindings.push((3, Read(auxiliary)));
    }
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        kernel,
        &mut bindings,
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    guide: &ReadGuard,
    mut p: Params,
    extent: [u32; 2],
    profile: &mut Profile,
) -> Result<ReadGuard> {
    if p.bands == [[0.; 4]; 8] && p.master == [0.; 4] {
        return Ok(input.clone());
    }
    p.shape[..2].copy_from_slice(&extent);
    let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 12;
    if input.size() != bytes || guide.size() != bytes {
        return Err("Color Editor needs matching image and clean color guide".into());
    }
    let mut gain = pass(gpu, Kernel::ColorEditorMap, guide, None, p, false, profile)?;
    for axis in 0..2 {
        p.shape[3] = axis;
        gain = pass(
            gpu,
            Kernel::ColorEditorFilter,
            &gain,
            Some(guide),
            p,
            false,
            profile,
        )?;
    }
    pass(
        gpu,
        Kernel::ColorEditorApply,
        input,
        Some(&gain),
        p,
        true,
        profile,
    )
}

/// Picking has its own uniform contract, separate from photographic adjustments.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct PickParams {
    shape: [u32; 4],
    crop: [u32; 4],
    guide_matrix: [[f32; 4]; 3],
    normalization: [f32; 4],
}
impl PickParams {
    pub fn new(
        extent: [u32; 2],
        space: ColorSpace,
        white: f32,
        pixel: [u32; 2],
        crop: [u32; 4],
    ) -> Self {
        Self {
            shape: [extent[0], extent[1], pixel[0], pixel[1]],
            crop,
            guide_matrix: guide_matrix(space),
            normalization: [white, 0., 0., 0.],
        }
    }
}

/// Small GPU selection record shared by Basic gestures and Advanced picking.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Sample {
    pub weights: [f32; 8],
    /// Hue in degrees, relative saturation and selection confidence.
    pub color: [f32; 4],
}
impl From<[f32; 8]> for Sample {
    fn from(weights: [f32; 8]) -> Self {
        Self {
            weights,
            ..Default::default()
        }
    }
}

/// Full photographic samples remain on the GPU.
pub(crate) fn pick(gpu: &Gpu, guide: &ReadGuard, params: PickParams) -> Result<Sample> {
    let uniform = gpu.uniform(&params);
    let mut output = gpu.storage("Color Range pick", size_of::<Sample>() as u64)?;
    let mut trace = gpu.trace(false, 1);
    gpu.dispatch(
        Kernel::ColorEditorPick,
        &mut [
            (0, Read(guide)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
        ],
        [1, 1],
        &mut trace,
    );
    let bytes = gpu.readback(&output)?;
    Ok(*bytemuck::from_bytes::<Sample>(&bytes))
}
