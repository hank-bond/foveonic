//! Prepares coordinated HDR ranges and a reusable multiscale log-brightness base.
//!
//! Response and edge coefficients follow the inspected coordinated spatial HDR operation.
//! The floating GPU filter uses our working-space luminance and box boundaries.

use crate::{
    color_space::ColorSpace,
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    render::ProcessingQuality,
    source::Result,
};

/// Speed uses half-grid gains and fourfold coefficient reduction. Regional
/// crops preserve that lattice. Quality uses each working pixel throughout.
pub(crate) const GUIDE_SCALE: u32 = 2;
pub(crate) const GUIDE_ALIGNMENT: u32 = GUIDE_SCALE * COARSE_SCALE;
const COARSE_SCALE: u32 = 4;
const COARSE_THRESHOLD: u32 = 15;
const NATIVE_RADIUS: u32 = 384;
const NATIVE_RADII: [u32; 8] = [2, 6, 12, 24, 48, 96, 192, 384];
const PIVOT_OUTPUT: f32 = 0.25;
const NOMINAL_WHITE: f32 = 4095.;
const EDGE_SIGMA_EV: f32 = 0.45;
const BROAD_EDGE_SIGMA_EV: f32 = 0.9;
const PASSES_PER_SCALE: u32 = 7;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
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

/// Match spatial support to the actual working grid, including arbitrary output sizing.
pub(crate) fn radius_for_scale(scale: f32) -> u32 {
    (NATIVE_RADIUS as f32 * scale)
        .round()
        .clamp(1., NATIVE_RADIUS as f32) as u32
}

/// Coefficients depend on the unchanged guide, so support is the largest
/// neighborhood, not the sum of every iteration's radii. Include the residual
/// neighborhood, coefficient mean, reduced cell footprint and interpolation.
pub(crate) fn support() -> u32 {
    let r = NATIVE_RADIUS / GUIDE_SCALE / COARSE_SCALE;
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
    pivot_with_white(curve, exposure, NOMINAL_WHITE)
}

pub(crate) fn pivot_with_white(curve: &[f32], exposure: f32, white: f32) -> Result<f32> {
    if !exposure.is_finite() {
        return Err("invalid HDR pivot exposure".into());
    }
    Ok(input_exposure_for_output(curve, PIVOT_OUTPUT, white)? - exposure)
}

/// Invert a metadata tone table at a linear output brightness, in stops from nominal white.
pub(crate) fn input_exposure_for_output(curve: &[f32], output: f32, white: f32) -> Result<f32> {
    if curve.len() < 2
        || white <= 0.
        || !white.is_finite()
        || !output.is_finite()
        || output <= 0.
        || curve.iter().any(|v| !v.is_finite())
    {
        return Err("invalid HDR pivot context".into());
    }
    let upper = curve
        .partition_point(|v| *v < output)
        .clamp(1, curve.len() - 1);
    let lower = upper - 1;
    let span = curve[upper] - curve[lower];
    let fraction = if span > 0. {
        ((output - curve[lower]) / span).clamp(0., 1.)
    } else {
        0.
    };
    Ok(((lower as f32 + fraction) / white)
        .max(2_f32.powi(-16))
        .log2())
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
    /// The maximum radius is measured in working RGB pixels.
    pub(crate) fn new(
        extent: [u32; 2],
        radius: u32,
        space: ColorSpace,
        processing_quality: ProcessingQuality,
    ) -> Result<Self> {
        if extent.contains(&0)
            || radius == 0
            || radius > NATIVE_RADIUS
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
        {
            return Err("invalid HDR mask geometry".into());
        }
        let step = cell_step(processing_quality);
        Ok(Self {
            shape: [
                extent[0],
                extent[1],
                extent[0].div_ceil(step),
                extent[1].div_ceil(step),
            ],
            sampling: [extent[0].div_ceil(step), extent[1].div_ceil(step), step, 0],
            filter: [radius as f32, 0., 0., step as f32],
            luma: luminance(space)?,
        })
    }
    pub(crate) fn with_white(mut self, white: f32) -> Self {
        if white == NOMINAL_WHITE {
            return self;
        }
        self.luma = self.luma.map(|v| v * NOMINAL_WHITE / white);
        self
    }
    /// Keep the established guide lattice when its RGB backing becomes compact.
    pub(crate) fn with_input_bounds(self, bounds: [u32; 4]) -> Result<Self> {
        let alignment = self.alignment();
        if !bounds[0].is_multiple_of(alignment)
            || !bounds[1].is_multiple_of(alignment)
            || bounds[0] >= bounds[2]
            || bounds[1] >= bounds[3]
            || bounds[2] > self.shape[0]
            || bounds[3] > self.shape[1]
        {
            return Err("compact HDR backing does not preserve the guide lattice".into());
        }
        let extent = [bounds[2] - bounds[0], bounds[3] - bounds[1]];
        let step = self.cell_step();
        let half = extent.map(|v| v.div_ceil(step));
        Ok(Self {
            shape: [extent[0], extent[1], half[0], half[1]],
            sampling: [half[0], half[1], step, 0],
            ..self
        })
    }

    pub(crate) fn required_input_bounds(self, bounds: [u32; 4]) -> Result<[u32; 4]> {
        if bounds[0] >= bounds[2] || bounds[1] >= bounds[3] {
            return Err("invalid requested HDR bounds".into());
        }
        let margin = support();
        let alignment = self.alignment();
        Ok([
            bounds[0].saturating_sub(margin) / alignment * alignment,
            bounds[1].saturating_sub(margin) / alignment * alignment,
            bounds[2]
                .saturating_add(margin)
                .div_ceil(alignment)
                .saturating_mul(alignment)
                .min(self.shape[0]),
            bounds[3]
                .saturating_add(margin)
                .div_ceil(alignment)
                .saturating_mul(alignment)
                .min(self.shape[1]),
        ])
    }

    pub(crate) fn storage_extent(self) -> [u32; 2] {
        [self.shape[2], self.shape[3]]
    }
    pub(crate) fn extent(self) -> [u32; 2] {
        [self.shape[0], self.shape[1]]
    }
    pub(crate) fn key(self) -> Key {
        Key::pod(&self)
    }
    pub(crate) fn cell_step(self) -> u32 {
        self.filter[3] as u32
    }
    fn alignment(self) -> u32 {
        if self.cell_step() == 1 {
            1
        } else {
            GUIDE_ALIGNMENT
        }
    }
    fn stages(self) -> Vec<(u32, u32)> {
        let maximum = self.filter[0] as u32;
        if self.cell_step() != 1 {
            let radius = (maximum as f32 / GUIDE_SCALE as f32).round().max(1.) as u32;
            return (0..=radius.ilog2())
                .rev()
                .map(|level| {
                    let r = radius >> level;
                    (r * GUIDE_SCALE, (r / 2) * GUIDE_SCALE)
                })
                .collect();
        }
        let scale = maximum as f32 / NATIVE_RADIUS as f32;
        let mut stages = Vec::with_capacity(NATIVE_RADII.len());
        for native in NATIVE_RADII {
            let radius = (native as f32 * scale).round().max(1.) as u32;
            if stages
                .last()
                .is_some_and(|&(previous, _)| previous == radius)
            {
                continue;
            }
            // Match the original half-grid residual neighborhoods, including
            // their zero-radius sentinel at the two-native-pixel stage.
            let residual = ((native / 4 * 2) as f32 * scale).round() as u32;
            stages.push((radius, residual));
        }
        stages
    }
    fn at_scale(
        mut self,
        coarse: bool,
        radius: u32,
        residual: u32,
        phase: u32,
        largest: bool,
    ) -> Self {
        let step = self.cell_step();
        let sample = step * if coarse { COARSE_SCALE } else { 1 };
        let radius = radius / sample;
        let residual = if coarse {
            radius / 2
        } else {
            residual / sample
        };
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
            residual as f32,
            step as f32,
        ];
        self
    }
}

/// Quality keeps a brightness and gain cell at every working pixel.
pub(crate) fn cell_step(processing_quality: ProcessingQuality) -> u32 {
    match processing_quality {
        ProcessingQuality::Quality => 1,
        ProcessingQuality::Speed => GUIDE_SCALE,
    }
}

/// One encoded Float32 cell represents each covered image block.
pub(crate) fn guide_storage_bytes(extent: [u32; 2], step: u32) -> u64 {
    u64::from(extent[0].div_ceil(step)) * u64::from(extent[1].div_ceil(step)) * 4
}

pub(crate) fn build_guide(
    gpu: &Gpu,
    input: &ReadGuard,
    guide: Guide,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let pixels = u64::from(guide.shape[0]) * u64::from(guide.shape[1]);
    if input.size() != pixels * 12 {
        return Err("HDR mask input extent mismatch".into());
    }
    let compact_pixels = u64::from(guide.shape[2]) * u64::from(guide.shape[3]);
    let mut output = gpu.storage("Compact HDR brightness base", compact_pixels * 4)?;
    let fine_groups = [guide.shape[2].div_ceil(8), guide.shape[3].div_ceil(8)];
    let speed = guide.cell_step() != 1;
    let stages = guide.stages();
    let passes = 2 + u32::from(speed) + PASSES_PER_SCALE * stages.len() as u32;
    let mut trace = gpu.trace(profile.mode.gpu(), passes);
    let bytes = u64::from(guide.shape[2]) * u64::from(guide.shape[3]) * 16;
    let mut fine = gpu.storage("HDR original fine guide", bytes)?;
    let coarse_guide = guide.at_scale(true, 8, 0, 0, false);
    let coarse_bytes =
        u64::from(coarse_guide.sampling[0]) * u64::from(coarse_guide.sampling[1]) * 16;
    let mut coarse = if speed {
        Some(gpu.storage("HDR original coarse guide", coarse_bytes)?)
    } else {
        None
    };
    for (parameters, target) in [(guide, Some(&mut fine)), (coarse_guide, coarse.as_mut())] {
        let Some(target) = target else { continue };
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
    let mut base_a = gpu.storage("HDR accumulated base A", bytes)?;
    let mut base_b = gpu.storage("HDR accumulated base B", bytes)?;
    let mut iterations = 0;
    for (index, &(radius, residual_radius)) in stages.iter().enumerate() {
        let largest = index + 1 == stages.len();
        let reduced = speed && radius / GUIDE_SCALE > COARSE_THRESHOLD;
        let parameters = guide.at_scale(reduced, radius, residual_radius, 0, largest);
        let signal = if reduced {
            coarse.as_ref().expect("Speed coarse guide")
        } else {
            &fine
        };
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
        let residual = guide.at_scale(reduced, radius, residual_radius, 1, largest);
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
        let coefficients = guide.at_scale(reduced, radius, residual_radius, 2, largest);
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
        fine_groups,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

/// Materialize the existing full-image scalar inspection without retaining it in edits.
pub(crate) fn expand_mask(
    gpu: &Gpu,
    input: &ReadGuard,
    guide: Guide,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = guide.extent();
    let [compact_width, compact_height] = guide.storage_extent();
    if input.size() != u64::from(compact_width) * u64::from(compact_height) * 4 {
        return Err("brightness guide does not match its compact extent".into());
    }
    let mut output = gpu.storage(
        "Brightness mask inspection",
        u64::from(width) * u64::from(height) * 4,
    )?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::ToneMaskExpand,
        &mut [
            (1, Read(&gpu.uniform(&guide))),
            (4, Read(input)),
            (5, Write(output.write())),
        ],
        [width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

/// Map RGB sample centers into an independently prepared compact brightness guide.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GuideSampling {
    shape: [u32; 4],
    bounds: [u32; 4],
    mapping: [[f32; 4]; 2],
}
impl GuideSampling {
    pub(crate) fn new(
        extent: [u32; 2],
        bounds: [u32; 4],
        mapping: [[f32; 4]; 2],
        cell_step: u32,
        gain_step: u32,
    ) -> Result<Self> {
        if extent.contains(&0)
            || bounds[0] >= bounds[2]
            || bounds[1] >= bounds[3]
            || bounds[2] > extent[0]
            || bounds[3] > extent[1]
            || ![1, 2].contains(&cell_step)
            || ![1, 2].contains(&gain_step)
            || mapping.iter().flatten().any(|v| !v.is_finite())
        {
            return Err("invalid brightness guide sampling".into());
        }
        Ok(Self {
            shape: [extent[0], extent[1], cell_step, gain_step],
            bounds,
            mapping,
        })
    }
    pub(crate) fn with_input_origin(mut self, origin: [u32; 2]) -> Self {
        for row in &mut self.mapping {
            row[2] += row[0] * origin[0] as f32 + row[1] * origin[1] as f32;
        }
        self
    }
    pub(crate) fn key(self) -> Key {
        Key::pod(&self)
    }
    pub(crate) fn extent(self) -> [u32; 2] {
        [self.shape[0], self.shape[1]]
    }
    pub(crate) fn cell_step(self) -> u32 {
        self.shape[2]
    }
    pub(crate) fn gain_step(self) -> u32 {
        self.shape[3]
    }
    pub(crate) fn is_mapped(self) -> bool {
        self.shape[2] != 0
    }
}

pub(crate) struct Response {
    pub processing_quality: ProcessingQuality,
    pub sampling: Option<GuideSampling>,
    pub controls: Controls,
    pub pivot: f32,
    pub space: ColorSpace,
}

pub(crate) fn apply_with_white(
    gpu: &Gpu,
    input: &ReadGuard,
    mask: &ReadGuard,
    extent: [u32; 2],
    response: Response,
    white: f32,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let Response {
        processing_quality,
        controls,
        pivot,
        space,
        sampling,
    } = response;
    controls.validate()?;
    let pixels = u64::from(extent[0]) * u64::from(extent[1]);
    if extent.contains(&0)
        || input.size() != pixels * 12
        || mask.size()
            != guide_storage_bytes(
                sampling.map_or(extent, GuideSampling::extent),
                sampling.map_or(cell_step(processing_quality), GuideSampling::cell_step),
            )
        || sampling.is_some_and(|s| s.gain_step() != cell_step(processing_quality))
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
    let sample = cell_step(processing_quality);
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
            (12, Read(&gpu.uniform(&sampling.unwrap_or_default()))),
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
