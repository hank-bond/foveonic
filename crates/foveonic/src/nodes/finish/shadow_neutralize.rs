//! Selectively reduces green and magenta chroma in dark, nearly neutral regions on GPU.

use crate::{
    color_space::{ColorSpace, TiffColorSpace},
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    source::Result,
};

pub(crate) const NATIVE_RADIUS: u32 = 8;
const IMAGE_WORKGROUP: u32 = 8;
const RGB_COMPONENTS: u64 = 3;
const GUIDE_COMPONENTS: u64 = 4;
const COMPONENT_BYTES: u64 = std::mem::size_of::<f32>() as u64;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Controls {
    pub strength: f32,
    /// Upper shadow boundary in stops below nominal linear white.
    pub shadow_ev: f32,
    /// Regional Oklab chroma divided by lightness. Lower values protect more colors.
    pub color_tolerance: f32,
    /// Angular tolerance around the green/magenta axis, in degrees.
    pub hue_width: f32,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            strength: 0.,
            shadow_ev: -4.,
            color_tolerance: 0.25,
            hue_width: 35.,
        }
    }
}
impl Controls {
    pub const STRENGTH_RANGE: std::ops::RangeInclusive<f32> = 0. ..=1.;
    pub const SHADOW_EV_RANGE: std::ops::RangeInclusive<f32> = -10. ..=-1.;
    pub const COLOR_TOLERANCE_RANGE: std::ops::RangeInclusive<f32> = 0.05..=1.;
    pub const HUE_WIDTH_RANGE: std::ops::RangeInclusive<f32> = 10. ..=75.;
    pub fn is_neutral(self) -> bool {
        self.strength == 0.
    }
    pub fn validate(self) -> Result<()> {
        for (value, range) in [
            (self.strength, Self::STRENGTH_RANGE),
            (self.shadow_ev, Self::SHADOW_EV_RANGE),
            (self.color_tolerance, Self::COLOR_TOLERANCE_RANGE),
            (self.hue_width, Self::HUE_WIDTH_RANGE),
        ] {
            if !value.is_finite() || !range.contains(&value) {
                return Err("invalid Neutralize shadows control".into());
            }
        }
        Ok(())
    }
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    extent: [u32; 2],
    guide_radius: u32,
    filter_axis: u32,
    strength: f32,
    shadow_limit: f32,
    color_tolerance: f32,
    hue_tangent: f32,
    to_srgb: [[f32; 4]; 3],
    from_srgb: [[f32; 4]; 3],
}
impl Params {
    pub(crate) fn new(controls: Controls, space: ColorSpace, scale: f32) -> Self {
        Self {
            extent: [0; 2],
            guide_radius: (NATIVE_RADIUS as f32 * scale)
                .ceil()
                .clamp(1., NATIVE_RADIUS as f32) as u32,
            filter_axis: 0,
            strength: controls.strength,
            shadow_limit: controls.shadow_ev.exp2(),
            color_tolerance: controls.color_tolerance,
            hue_tangent: controls.hue_width.to_radians().tan(),
            to_srgb: TiffColorSpace::Srgb.transform(space),
            from_srgb: TiffColorSpace::from(space).transform(ColorSpace::Srgb),
        }
    }
    pub(crate) fn key(self) -> crate::recipe::Key {
        crate::recipe::Key::pod(&self)
    }
    pub(crate) fn guide_key(self) -> crate::recipe::Key {
        crate::recipe::Key::pod(&self.guide_radius).with(&self.to_srgb)
    }
}
fn dispatch_pass(
    gpu: &Gpu,
    kernel: Kernel,
    input: &ReadGuard,
    aux: Option<&ReadGuard>,
    params: Params,
    channels: u64,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let mut output = gpu.storage(
        "Shadow neutrality",
        u64::from(params.extent[0]) * u64::from(params.extent[1]) * channels * COMPONENT_BYTES,
    )?;
    let uniform = gpu.uniform(&params);
    let mut bindings = vec![
        (0, Read(input)),
        (1, Read(&uniform)),
        (2, Write(output.write())),
    ];
    if let Some(aux) = aux {
        bindings.push((3, Read(aux)));
    }
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        kernel,
        &mut bindings,
        params.extent.map(|length| length.div_ceil(IMAGE_WORKGROUP)),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
pub(crate) fn prepare_guide(
    gpu: &Gpu,
    input: &ReadGuard,
    mut params: Params,
    extent: [u32; 2],
    profile: &mut Profile,
) -> Result<ReadGuard> {
    if input.size()
        != u64::from(extent[0]) * u64::from(extent[1]) * RGB_COMPONENTS * COMPONENT_BYTES
    {
        return Err("Neutralize shadows requires linear RGB".into());
    }
    params.extent = extent;
    let guide = dispatch_pass(
        gpu,
        Kernel::ShadowNeutralAnalyze,
        input,
        None,
        params,
        GUIDE_COMPONENTS,
        profile,
    )?;
    let horizontal = dispatch_pass(
        gpu,
        Kernel::ShadowNeutralFilter,
        &guide,
        Some(&guide),
        params,
        GUIDE_COMPONENTS,
        profile,
    )?;
    params.filter_axis = 1;
    dispatch_pass(
        gpu,
        Kernel::ShadowNeutralFilter,
        &horizontal,
        Some(&guide),
        params,
        GUIDE_COMPONENTS,
        profile,
    )
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    regional: &ReadGuard,
    mut params: Params,
    extent: [u32; 2],
    profile: &mut Profile,
) -> Result<ReadGuard> {
    if params.strength == 0. {
        return Ok(input.clone());
    }
    let pixels = u64::from(extent[0]) * u64::from(extent[1]);
    if input.size() != pixels * RGB_COMPONENTS * COMPONENT_BYTES
        || regional.size() != pixels * GUIDE_COMPONENTS * COMPONENT_BYTES
    {
        return Err("Neutralize shadows image and guide geometry mismatch".into());
    }
    params.extent = extent;
    dispatch_pass(
        gpu,
        Kernel::ShadowNeutralApply,
        input,
        Some(regional),
        params,
        RGB_COMPONENTS,
        profile,
    )
}

#[cfg(test)]
#[path = "shadow_neutralize_test.rs"]
mod tests;
