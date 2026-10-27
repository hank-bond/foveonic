//! Edits hue, saturation and lightness in linear RGB using an independent cleaned color guide.

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
    fn values(self) -> [f32; 3] {
        [self.hue, self.saturation, self.lightness]
    }
}

/// Red, Orange, Yellow, Green, Cyan, Blue, Purple and Magenta, plus all colors.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
pub struct Controls {
    pub bands: [Adjustment; 8],
    pub master: Adjustment,
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
    /// Split the two new ranges without changing an existing six-color response.
    pub fn from_lightness(v: [f32; 6]) -> Self {
        Self {
            bands: [
                v[0],
                (v[0] + v[1]) * 0.5,
                v[1],
                v[2],
                v[3],
                v[4],
                (v[4] + v[5]) * 0.5,
                v[5],
            ]
            .map(|lightness| Adjustment {
                lightness,
                ..Default::default()
            }),
            ..Default::default()
        }
    }
    pub fn chromatic_active(self) -> bool {
        self.bands
            .into_iter()
            .chain([self.master])
            .any(|v| v.hue != 0. || v.saturation != 0.)
    }
    pub fn is_active(self, monochrome: bool) -> bool {
        self.bands
            .into_iter()
            .chain([self.master])
            .any(|v| v.lightness != 0.)
            || (!monochrome && self.chromatic_active())
    }
    pub(crate) fn validate(self) -> Result<()> {
        if self
            .bands
            .into_iter()
            .chain([self.master])
            .flat_map(Adjustment::values)
            .any(|v| !(-1. ..=1.).contains(&v))
        {
            return Err("Color Editor amounts must be between -1 and 1".into());
        }
        Ok(())
    }
}
impl<'de> serde::Deserialize<'de> for Controls {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Default, serde::Deserialize)]
        #[serde(default, deny_unknown_fields)]
        struct Current {
            bands: [Adjustment; 8],
            master: Adjustment,
        }
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Stored {
            Current(Current),
            Legacy([f32; 6]),
        }
        Ok(match Stored::deserialize(deserializer)? {
            Stored::Current(v) => Self {
                bands: v.bands,
                master: v.master,
            },
            Stored::Legacy(v) => Self::from_lightness(v),
        })
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
        // D50 colorants of the researched linear reference working profile.
        // The profile's Adobe name alone does not identify these exact colorants.
        const REFERENCE_COLORANTS: [f64; 9] = [
            42294., 11865., 9030., 21808., 40115., 3611., 1982., 4520., 47557.,
        ];
        const ICC_FIXED_SCALE: f64 = (1_u32 << 16) as f64;
        let inverse = crate::color_space::inverse(REFERENCE_COLORANTS.map(|v| v / ICC_FIXED_SCALE));
        let source = TiffColorSpace::from(space).to_xyz();
        let guide_matrix = std::array::from_fn(|r| {
            std::array::from_fn(|c| {
                if c < 3 {
                    (0..3)
                        .map(|k| inverse[r * 3 + k] * source[k * 3 + c])
                        .sum::<f64>() as f32
                } else {
                    0.
                }
            })
        });
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
                u32::from(!monochrome && controls.chromatic_active()) as f32,
                white,
                0.,
            ],
            bands: controls.bands.map(pack),
            master: pack(controls.master),
            luma: crate::nodes::finish::photographic::luminance(space),
            guide_matrix,
            to_srgb: TiffColorSpace::Srgb.transform(space),
            from_srgb: TiffColorSpace::from(space).transform(ColorSpace::Srgb),
        }
    }
    pub fn key(self) -> Key {
        Key::pod(&self)
    }
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
