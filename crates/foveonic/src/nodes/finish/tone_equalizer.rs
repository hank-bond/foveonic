//! Prepares coordinated HDR ranges and a reusable multiscale log-brightness base.
//!
//! The response and edge coefficients follow research into the reference implementation version 16.6.
//! The floating GPU filter uses our working-space luminance and box boundaries.

use crate::{
    color_space::ColorSpace,
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

/// HDR gains live at half width and height. Large neighborhoods use an
/// additional fourfold reduction. Regional crops must share both grids.
pub(crate) const GUIDE_SCALE: u32 = 2;
pub(crate) const GUIDE_ALIGNMENT: u32 = GUIDE_SCALE * COARSE_SCALE;
const COARSE_SCALE: u32 = 4;
const COARSE_THRESHOLD: u32 = 15;
const NATIVE_RADIUS: u32 = 192;
const PIVOT_OUTPUT: f32 = 0.25;
const NOMINAL_WHITE: f32 = 4095.;
const EDGE_SIGMA_EV: f32 = 0.45;
const BROAD_EDGE_SIGMA_EV: f32 = 0.9;
const PASSES_PER_SCALE: u32 = 7;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "SavedControls", into = "SavedControls")]
pub struct Controls {
    pub blacks: f32,
    pub shadows: f32,
    pub highlights: f32,
    pub whites: f32,
}
impl Controls {
    pub const MAX_AMOUNT: f32 = 1.;
    pub fn is_neutral(self) -> bool {
        self.amounts() == [0.; 4]
    }
    pub(crate) fn amounts(self) -> [f32; 4] {
        [self.blacks, self.shadows, self.highlights, self.whites]
    }
    pub(crate) fn validate(self) -> Result<()> {
        if self
            .amounts()
            .iter()
            .any(|v| !v.is_finite() || v.abs() > Self::MAX_AMOUNT)
        {
            return Err("HDR range adjustments must be between -1 and +1".into());
        }
        Ok(())
    }
}

// The earlier in-app experiment saved percentage amounts and a mask choice.
// Read those once into the normalized spatial-only recipe. New saves carry
// their units explicitly so small old percentages cannot be misinterpreted.
#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SavedControls {
    version: u32,
    blacks: f32,
    shadows: f32,
    highlights: f32,
    whites: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    mask: Option<String>,
}
impl From<Controls> for SavedControls {
    fn from(c: Controls) -> Self {
        Self {
            version: 1,
            blacks: c.blacks,
            shadows: c.shadows,
            highlights: c.highlights,
            whites: c.whites,
            mask: None,
        }
    }
}
impl TryFrom<SavedControls> for Controls {
    type Error = &'static str;
    fn try_from(c: SavedControls) -> std::result::Result<Self, Self::Error> {
        let scale = match c.version {
            0 => 0.01,
            1 => 1.,
            _ => return Err("unsupported HDR settings version"),
        };
        Ok(Self {
            blacks: c.blacks * scale,
            shadows: c.shadows * scale,
            highlights: c.highlights * scale,
            whites: c.whites * scale,
        })
    }
}

/// Render scale changes the spatial radius, rather than the meaning of the
/// selected tone ranges. A regional Full request still uses the native radius.
pub(crate) fn radius(reduced: bool) -> u32 {
    if reduced {
        NATIVE_RADIUS / 2
    } else {
        NATIVE_RADIUS
    }
}

/// Coefficients depend on the unchanged guide, so support is the largest
/// neighborhood, not the sum of every iteration's radii. Include the residual
/// neighborhood, coefficient mean, reduced cell footprint and interpolation.
pub(crate) fn support() -> u32 {
    let r = NATIVE_RADIUS / COARSE_SCALE;
    (2 * r + r / 2 + 2) * GUIDE_ALIGNMENT
}

fn luminance(space: ColorSpace) -> Result<[f32; 4]> {
    match space {
        ColorSpace::Srgb => Ok([0.2126, 0.7152, 0.0722, 0.]),
        ColorSpace::AdobeRgb => Ok([0.297361, 0.627355, 0.075284, 0.]),
        ColorSpace::ProPhotoRgb => Ok([0.28804, 0.71187, 0.00009, 0.]),
    }
}

/// Invert our camera curve at the researched rendered pivot, then undo the
/// exposure applied after HDR. This operates on a numerical table, not pixels.
pub(crate) fn pivot(curve: &[f32], exposure: f32) -> Result<f32> {
    if curve.len() < 2 || !exposure.is_finite() || curve.iter().any(|v| !v.is_finite()) {
        return Err("invalid HDR pivot curve or exposure".into());
    }
    let upper = curve
        .partition_point(|v| *v < PIVOT_OUTPUT)
        .clamp(1, curve.len() - 1);
    let lower = upper - 1;
    let span = curve[upper] - curve[lower];
    let fraction = if span > 0. {
        ((PIVOT_OUTPUT - curve[lower]) / span).clamp(0., 1.)
    } else {
        0.
    };
    Ok(((lower as f32 + fraction) / NOMINAL_WHITE)
        .max(2_f32.powi(-16))
        .log2()
        - exposure)
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Guide {
    shape: [u32; 4],
    sampling: [u32; 4],
    filter: [f32; 4],
    luma: [f32; 4],
}
impl Guide {
    pub(crate) fn new(extent: [u32; 2], radius: u32, space: ColorSpace) -> Result<Self> {
        if extent.contains(&0)
            || radius == 0
            || radius > NATIVE_RADIUS
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
        {
            return Err("invalid HDR mask geometry".into());
        }
        Ok(Self {
            shape: [
                extent[0],
                extent[1],
                extent[0].div_ceil(GUIDE_SCALE),
                extent[1].div_ceil(GUIDE_SCALE),
            ],
            sampling: [
                extent[0].div_ceil(GUIDE_SCALE),
                extent[1].div_ceil(GUIDE_SCALE),
                GUIDE_SCALE,
                0,
            ],
            filter: [radius as f32, 0., 0., 0.],
            luma: luminance(space)?,
        })
    }
    pub(crate) fn photographic(extent: [u32; 2], radius: u32, space: ColorSpace) -> Result<Self> {
        let mut guide = Self::new(extent, radius, space)?;
        guide.luma = guide.luma.map(|v| v * NOMINAL_WHITE);
        Ok(guide)
    }
    pub(crate) fn extent(self) -> [u32; 2] {
        [self.shape[0], self.shape[1]]
    }
    pub(crate) fn key(self) -> Key {
        Key::pod(&self)
    }
    fn at_scale(mut self, coarse: bool, radius: u32, phase: u32, largest: bool) -> Self {
        let sample = GUIDE_SCALE * if coarse { COARSE_SCALE } else { 1 };
        self.sampling = [
            self.shape[0].div_ceil(sample),
            self.shape[1].div_ceil(sample),
            sample,
            phase,
        ];
        // Native sigma codes are 0.9 and 0.45 stops. Use EV² directly rather
        // than quantizing moments into the original unsigned fixed-point grid.
        self.filter = [
            radius as f32,
            if largest {
                BROAD_EDGE_SIGMA_EV * BROAD_EDGE_SIGMA_EV
            } else {
                EDGE_SIGMA_EV * EDGE_SIGMA_EV
            },
            (radius / 2) as f32,
            0.,
        ];
        self
    }
}

pub(crate) fn mask(
    gpu: &Gpu,
    input: &ReadGuard,
    guide: Guide,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let pixels = u64::from(guide.shape[0]) * u64::from(guide.shape[1]);
    if input.size() != pixels * 12 {
        return Err("HDR mask input extent mismatch".into());
    }
    let mut output = gpu.storage("HDR brightness base", pixels * 4)?;
    let full = [guide.shape[0].div_ceil(8), guide.shape[1].div_ceil(8)];
    let fine_groups = [guide.shape[2].div_ceil(8), guide.shape[3].div_ceil(8)];
    // Two guide reductions, seven passes per scale, and the inspectable base.
    let passes = 3 + PASSES_PER_SCALE * ((guide.filter[0] as u32).ilog2() + 1);
    let mut trace = gpu.trace(profile.mode.gpu(), passes);
    let bytes = u64::from(guide.shape[2]) * u64::from(guide.shape[3]) * 16;
    let mut fine = gpu.storage("HDR original half-size guide", bytes)?;
    let coarse_guide = guide.at_scale(true, 1, 0, false);
    let coarse_bytes =
        u64::from(coarse_guide.sampling[0]) * u64::from(coarse_guide.sampling[1]) * 16;
    let mut coarse = gpu.storage("HDR original coarse guide", coarse_bytes)?;
    for (parameters, target) in [(guide, &mut fine), (coarse_guide, &mut coarse)] {
        gpu.dispatch(
            Kernel::ToneMaskReduce,
            &mut [
                (0, Read(input)),
                (1, Read(&gpu.uniform(&parameters))),
                (3, Write(target.write())),
            ],
            [
                parameters.sampling[0].div_ceil(8),
                parameters.sampling[1].div_ceil(8),
            ],
            &mut trace,
        );
    }
    let mut a = gpu.storage("HDR statistics and coefficients", bytes)?;
    let mut b = gpu.storage("HDR horizontal means", bytes)?;
    let maximum = guide.filter[0] as u32;
    let mut base_a = gpu.storage("HDR accumulated base A", bytes)?;
    let mut base_b = gpu.storage("HDR accumulated base B", bytes)?;
    let mut iterations = 0;
    for level in (0..=maximum.ilog2()).rev() {
        let native_radius = maximum >> level;
        let reduced = native_radius > COARSE_THRESHOLD;
        let r = if reduced {
            native_radius / COARSE_SCALE
        } else {
            native_radius
        };
        let parameters = guide.at_scale(reduced, r, 0, level == 0);
        let signal = if reduced { &coarse } else { &fine };
        let groups = [
            parameters.sampling[0].div_ceil(8),
            parameters.sampling[1].div_ceil(8),
        ];
        gpu.dispatch(
            Kernel::ToneMaskHorizontal,
            &mut [
                (1, Read(&gpu.uniform(&parameters))),
                (2, Read(signal)),
                (3, Write(b.write())),
            ],
            groups,
            &mut trace,
        );
        gpu.dispatch(
            Kernel::ToneMaskStatistics,
            &mut [
                (1, Read(&gpu.uniform(&parameters))),
                (2, Read(&b)),
                (3, Write(a.write())),
                (8, Read(signal)),
            ],
            groups,
            &mut trace,
        );
        let residual = guide.at_scale(reduced, r, 1, level == 0);
        gpu.dispatch(
            Kernel::ToneMaskHorizontal,
            &mut [
                (1, Read(&gpu.uniform(&residual))),
                (2, Read(&a)),
                (3, Write(b.write())),
            ],
            groups,
            &mut trace,
        );
        gpu.dispatch(
            Kernel::ToneMaskCoefficients,
            &mut [
                (1, Read(&gpu.uniform(&parameters))),
                (2, Read(&b)),
                (3, Write(a.write())),
            ],
            groups,
            &mut trace,
        );
        let coefficients = guide.at_scale(reduced, r, 2, level == 0);
        gpu.dispatch(
            Kernel::ToneMaskHorizontal,
            &mut [
                (1, Read(&gpu.uniform(&coefficients))),
                (2, Read(&a)),
                (3, Write(b.write())),
            ],
            groups,
            &mut trace,
        );
        gpu.dispatch(
            Kernel::ToneMaskVertical,
            &mut [
                (1, Read(&gpu.uniform(&parameters))),
                (2, Read(&b)),
                (3, Write(a.write())),
            ],
            groups,
            &mut trace,
        );
        let (previous, next) = if iterations == 0 {
            (&fine, &mut base_a)
        } else if iterations % 2 == 0 {
            (&base_b, &mut base_a)
        } else {
            (&base_a, &mut base_b)
        };
        gpu.dispatch(
            Kernel::ToneMaskUpdate,
            &mut [
                (1, Read(&gpu.uniform(&parameters))),
                (2, Read(&a)),
                (3, Write(next.write())),
                (9, Read(previous)),
            ],
            fine_groups,
            &mut trace,
        );
        iterations += 1;
    }
    let previous = if iterations % 2 == 0 {
        &base_b
    } else {
        &base_a
    };
    gpu.dispatch(
        Kernel::ToneMaskFinish,
        &mut [
            (1, Read(&gpu.uniform(&guide))),
            (5, Write(output.write())),
            (9, Read(previous)),
        ],
        full,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

pub(crate) struct Response {
    pub controls: Controls,
    pub pivot: f32,
    pub space: ColorSpace,
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    mask: &ReadGuard,
    extent: [u32; 2],
    response: Response,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    apply_with_white(gpu, input, mask, extent, response, NOMINAL_WHITE, profile)
}

pub(crate) fn apply_photographic(
    gpu: &Gpu,
    input: &ReadGuard,
    mask: &ReadGuard,
    extent: [u32; 2],
    response: Response,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    apply_with_white(gpu, input, mask, extent, response, 1., profile)
}

fn apply_with_white(
    gpu: &Gpu,
    input: &ReadGuard,
    mask: &ReadGuard,
    extent: [u32; 2],
    response: Response,
    white: f32,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let Response {
        controls,
        pivot,
        space,
    } = response;
    controls.validate()?;
    let pixels = u64::from(extent[0]) * u64::from(extent[1]);
    if extent.contains(&0)
        || input.size() != pixels * 12
        || mask.size() != pixels * 4
        || !pivot.is_finite()
    {
        return Err("invalid HDR input geometry or pivot".into());
    }
    if controls.is_neutral() {
        return Ok(input.clone());
    }
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Adjustment {
        shape: [u32; 4],
        amounts: [f32; 4],
        luma: [f32; 4],
        pivot: [f32; 4],
    }
    let sample = GUIDE_SCALE;
    let half = extent.map(|v| v.div_ceil(sample));
    let uniform = gpu.uniform(&Adjustment {
        shape: [extent[0], extent[1], half[0], half[1]],
        amounts: controls.amounts(),
        luma: luminance(space)?.map(|v| v * NOMINAL_WHITE / white),
        pivot: [pivot, sample as f32, 0., 0.],
    });
    let mut gains = gpu.storage(
        "HDR range gains",
        u64::from(half[0]) * u64::from(half[1]) * 4,
    )?;
    let mut output = gpu.storage("HDR linear RGB", pixels * 12)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    gpu.dispatch(
        Kernel::ToneEqualizerGain,
        &mut [
            (0, Read(input)),
            (4, Read(mask)),
            (6, Read(&uniform)),
            (10, Write(gains.write())),
        ],
        [half[0].div_ceil(8), half[1].div_ceil(8)],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ToneEqualizerApply,
        &mut [
            (0, Read(input)),
            (6, Read(&uniform)),
            (7, Write(output.write())),
            (11, Read(&gains)),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "tone_equalizer_test.rs"]
mod tests;
