//! Applies bounded soft color selections and perceptual edits from an independent clean guide.

use super::color_editor::Adjustment;
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

pub const MAX_RANGES: usize = 30;
pub const MAX_NAME_CHARS: usize = 64;

fn empty_name(name: &arrayvec::ArrayString<256>) -> bool {
    name.is_empty()
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Range {
    /// Up to 64 Unicode characters without heap ownership in the copied control table.
    #[serde(skip_serializing_if = "empty_name")]
    pub name: arrayvec::ArrayString<256>,
    pub enabled: bool,
    /// Center and full interior width in degrees. Width 360 selects all hues.
    pub hue: f32,
    pub width: f32,
    /// Relative saturation in the cleaned guide, from neutral to fully colorful.
    pub saturation: [f32; 2],
    pub smoothness: f32,
    pub adjustment: Adjustment,
}
impl Default for Range {
    fn default() -> Self {
        Self {
            name: Default::default(),
            enabled: true,
            hue: 240.,
            width: 40.,
            saturation: [0., 1.],
            smoothness: 0.2,
            adjustment: Adjustment::default(),
        }
    }
}
impl Range {
    pub fn is_active(self, mono: bool) -> bool {
        self.enabled
            && (self.adjustment.lightness != 0.
                || (!mono && (self.adjustment.hue != 0. || self.adjustment.saturation != 0.)))
    }
}

/// Fixed storage keeps photographic controls cheap to copy and GPU uniforms bounded.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    pub ranges: [Range; MAX_RANGES],
    pub count: usize,
}
impl serde::Serialize for Controls {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serde::Serialize::serialize(self.entries(), serializer)
    }
}
impl<'de> serde::Deserialize<'de> for Controls {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let entries = <Vec<Range> as serde::Deserialize>::deserialize(deserializer)?;
        if entries.len() > MAX_RANGES {
            return Err(serde::de::Error::custom(
                "too many Advanced Color Range edits",
            ));
        }
        let mut controls = Self::default();
        for range in entries {
            controls.push(range);
        }
        controls.validate().map_err(serde::de::Error::custom)?;
        Ok(controls)
    }
}
impl Controls {
    pub fn entries(&self) -> &[Range] {
        &self.ranges[..self.count.min(MAX_RANGES)]
    }
    pub fn is_active(self, mono: bool) -> bool {
        self.entries().iter().any(|r| r.is_active(mono))
    }
    /// Labels are authored metadata and do not change the rendered image.
    pub fn clear_names(&mut self) {
        for range in &mut self.ranges {
            range.name.clear();
        }
    }
    pub fn chromatic_active(self) -> bool {
        // Oklab Lightness at fixed a/b also changes RGB ratios. DNG must bake
        // this color transform rather than transfer only a scalar gain.
        self.is_active(false)
    }
    pub fn push(&mut self, range: Range) -> Option<usize> {
        if self.count >= MAX_RANGES {
            return None;
        }
        let i = self.count;
        self.ranges[i] = range;
        self.count += 1;
        Some(i)
    }
    pub fn remove(&mut self, index: usize) {
        if index < self.count {
            self.ranges.copy_within(index + 1..self.count, index);
            self.count -= 1;
            self.ranges[self.count] = Range::default();
        }
    }
    pub(crate) fn validate(self) -> Result<()> {
        if self.count > MAX_RANGES {
            return Err("too many Advanced Color Range edits".into());
        }
        for r in self.entries() {
            r.adjustment.validate()?;
            if r.name.chars().count() > MAX_NAME_CHARS {
                return Err("Color Range names must be at most 64 characters".into());
            }
            if !(0. ..360.).contains(&r.hue)
                || !(0. ..=360.).contains(&r.width)
                || !(0. ..=1.).contains(&r.smoothness)
                || r.saturation.iter().any(|v| !(0. ..=1.).contains(v))
                || r.saturation[0] > r.saturation[1]
            {
                return Err("invalid Advanced Color Range controls".into());
            }
        }
        Ok(())
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PackedRange {
    selection: [f32; 4],
    adjustment: [f32; 4],
    options: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    shape: [u32; 4],
    normalization: [f32; 4],
    guide_matrix: [[f32; 4]; 3],
    to_srgb: [[f32; 4]; 3],
    from_srgb: [[f32; 4]; 3],
    ranges: [PackedRange; MAX_RANGES],
    /// Shared gamut protection flag and linear code white before Exposure.
    protection: [f32; 4],
}
impl Params {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        extent: [u32; 2],
        controls: Controls,
        space: ColorSpace,
        white: f32,
        guide_white: f32,
        exposure: f32,
        mono: bool,
        preview: Option<usize>,
    ) -> Self {
        Self {
            shape: [
                extent[0],
                extent[1],
                controls.count.min(MAX_RANGES) as u32,
                u32::from(mono),
            ],
            normalization: [
                exposure.exp2() / white,
                white / exposure.exp2(),
                guide_white,
                preview
                    .filter(|i| *i < controls.count)
                    .map_or(-1., |i| i as f32),
            ],
            guide_matrix: super::color_editor::guide_matrix(space),
            to_srgb: TiffColorSpace::Srgb.transform(space),
            from_srgb: TiffColorSpace::from(space).transform(ColorSpace::Srgb),
            ranges: controls.ranges.map(|r| PackedRange {
                selection: [r.hue, r.width, r.saturation[0], r.saturation[1]],
                adjustment: [
                    if mono { 0. } else { r.adjustment.hue },
                    if mono { 0. } else { r.adjustment.saturation },
                    r.adjustment.lightness,
                    r.smoothness,
                ],
                options: [u32::from(r.enabled) as f32, 0., 0., 0.],
            }),
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
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    guide: &ReadGuard,
    mut params: Params,
    extent: [u32; 2],
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 12;
    if input.size() != bytes || guide.size() != bytes {
        return Err("Advanced Color Range guide geometry mismatch".into());
    }
    params.shape[..2].copy_from_slice(&extent);
    let uniform = gpu.uniform(&params);
    let mut output = gpu.storage("Advanced Color Range", bytes)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::AdvancedColor,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
            (3, Read(guide)),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "advanced_color_test.rs"]
mod tests;
