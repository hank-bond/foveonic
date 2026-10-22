//! Composes white balance, UV classification and fixed-radius range filtering in camera RGB codes.

use super::uv_repair;
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

// Guide thresholds are in white-balanced camera codes. The same ramp controls
// UV classification and the blend that protects the original RGB values.
const GUIDE_BLEND_START: f32 = 200.;
const GUIDE_BLEND_WIDTH: f32 = 25.;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    controls: [f32; 4],
    gains: [f32; 4],
}

pub(crate) struct Prepared {
    params: Params,
    uv: uv_repair::Prepared,
    coefficients: Vec<[f32; 2]>,
    intensity: Vec<f32>,
    range: Vec<f32>,
}

pub(crate) struct Filtered {
    pub(crate) rgb: ReadGuard,
    /// The source after its Float32 WB multiply/divide round trip.
    pub(crate) source_after: ReadGuard,
}

impl Prepared {
    /// Noise coefficients and variance mixing use Double preparation. The
    /// actual RGB gain application and range accumulation retain Float32 order.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        extent: [u32; 2],
        gains: [f64; 3],
        model: [f64; 9],
        variance_mix: [f64; 9],
        intensity: Vec<f32>,
        range: Vec<f32>,
        strength: f32,
        radius: u32,
    ) -> Result<Self> {
        if extent.contains(&0)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
            || gains
                .iter()
                .any(|v| !v.is_finite() || *v <= 0. || !(*v as f32).is_finite() || *v as f32 == 0.)
            || variance_mix
                .iter()
                .any(|v| !v.is_finite() || !(*v as f32).is_finite())
            || range.len() != 2048
            || range.iter().any(|v| !v.is_finite() || *v < 0.)
            || !strength.is_finite()
            || strength <= 0.
            || !(1..=31).contains(&radius)
        {
            return Err("invalid RGB prefilter parameters".into());
        }
        let working_model = std::array::from_fn(|i| match i / 3 {
            0 => model[i] * (gains[i % 3] * gains[i % 3]),
            1 => model[i] * gains[i % 3],
            _ => model[i],
        });
        let maximum_gain = gains.into_iter().fold(f64::NEG_INFINITY, f64::max) as f32;
        let uv = uv_repair::Prepared::new(
            extent,
            working_model,
            intensity.clone(),
            (maximum_gain * GUIDE_BLEND_START) + (maximum_gain * GUIDE_BLEND_WIDTH) * 0.5,
        )?;
        let coefficients = working_model
            .into_iter()
            .chain(variance_mix)
            .map(|v| {
                let high = v as f32;
                [high, (v - f64::from(high)) as f32]
            })
            .collect();
        Ok(Self {
            params: Params {
                shape: [extent[0], extent[1], radius, 0],
                controls: [
                    strength,
                    maximum_gain * GUIDE_BLEND_START,
                    maximum_gain * GUIDE_BLEND_WIDTH,
                    0.,
                ],
                gains: [gains[0] as f32, gains[1] as f32, gains[2] as f32, 0.],
            },
            uv,
            coefficients,
            intensity,
            range,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
            .join(self.uv.key())
            .slice(&self.coefficients)
            .slice(&self.intensity)
            .slice(&self.range)
    }
}

fn gain(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    inverse: bool,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let mut params = prepared.params;
    params.shape[3] = u32::from(inverse);
    let uniform = gpu.uniform(&params);
    let mut output = gpu.storage("Camera RGB gain", input.size())?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::RgbPrefilterGain,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
        ],
        [params.shape[0].div_ceil(8), params.shape[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<Filtered> {
    let extent = prepared.extent();
    if input.size() != u64::from(extent[0]) * u64::from(extent[1]) * 12 {
        return Err("RGB prefilter input does not match its extent".into());
    }
    let balanced = gain(gpu, input, prepared, false, profile)?;
    let yuv = uv_repair::convert(gpu, &balanced, extent, false, profile)?;
    let classified = uv_repair::apply(gpu, &balanced, &yuv, &prepared.uv, profile)?;
    let restored = uv_repair::convert(gpu, &classified.yuv, extent, true, profile)?;
    let uniform = gpu.uniform(&prepared.params);
    let mut repaired = gpu.storage("UV repaired camera RGB", input.size())?;
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    // The wrapper blends into the converted/repaired image; its separate
    // original WB image supplies the complementary contribution.
    gpu.dispatch(
        Kernel::RgbPrefilterBlend,
        &mut [
            (0, Read(&restored)),
            (1, Read(&balanced)),
            (2, Read(&classified.guide)),
            (3, Read(&classified.mask)),
            (4, Read(&uniform)),
            (5, Write(repaired.write())),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    let coefficients = gpu.upload(
        "Range filter model and variance mixing",
        &prepared.coefficients,
    )?;
    let intensity = gpu.upload("Range intensity conditioning", &prepared.intensity)?;
    let range = gpu.upload("Range weights", &prepared.range)?;
    let mut filtered = gpu.storage("Range filtered camera RGB", input.size())?;
    gpu.dispatch(
        Kernel::RgbPrefilterRange,
        &mut [
            (0, Read(&repaired)),
            (1, Read(&classified.mask)),
            (2, Read(&uniform)),
            (3, Read(&coefficients)),
            (4, Read(&intensity)),
            (5, Read(&range)),
            (6, Write(filtered.write())),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(Filtered {
        rgb: gain(gpu, &filtered, prepared, true, profile)?,
        source_after: gain(gpu, &balanced, prepared, true, profile)?,
    })
}

#[cfg(test)]
#[path = "rgb_prefilter_test.rs"]
mod tests;

/// Standalone RGB bilateral filtering with optional intensity-dependent strength.
/// It applies no white-balance conversion or UV classification.
pub(crate) struct RangePrepared {
    params: Params,
    model: Vec<[f32; 2]>,
    intensity: Vec<f32>,
    range: Vec<f32>,
}

impl RangePrepared {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        extent: [u32; 2],
        model: [f64; 9],
        intensity: Vec<f32>,
        range: Vec<f32>,
        strength: f32,
        radius: u32,
        adaptive: Option<Vec<f32>>,
    ) -> Result<Self> {
        if extent.contains(&0)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
            || model
                .iter()
                .any(|v| !v.is_finite() || !(*v as f32).is_finite())
            || intensity.len() != 14385
            || intensity.iter().any(|v| !v.is_finite() || *v <= 0.)
            || range.len() != 2048
            || range.iter().any(|v| !v.is_finite() || *v < 0.)
            || !strength.is_finite()
            || strength <= 0.
            || !(1..=31).contains(&radius)
            || adaptive
                .as_ref()
                .is_some_and(|v| v.len() != 16384 || v.iter().any(|x| !x.is_finite() || *x <= 0.))
        {
            return Err("invalid standalone RGB range filter controls".into());
        }
        let mode = if adaptive.is_some() { 1 } else { 2 };
        let mut tables = intensity;
        if let Some(adaptive) = adaptive {
            tables.extend(adaptive);
        }
        Ok(Self {
            params: Params {
                shape: [extent[0], extent[1], radius, mode],
                controls: [strength, 0., 0., 0.],
                gains: [1., 1., 1., 0.],
            },
            model: model
                .map(|v| {
                    let high = v as f32;
                    [high, (v - f64::from(high)) as f32]
                })
                .to_vec(),
            intensity: tables,
            range,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
            .slice(&self.model)
            .slice(&self.intensity)
            .slice(&self.range)
    }
}

pub(crate) fn apply_range(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &RangePrepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let extent = prepared.extent();
    let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 12;
    if input.size() != bytes {
        return Err("standalone RGB range input size does not match extent".into());
    }
    let uniform = gpu.uniform(&prepared.params);
    let model = gpu.upload("Standalone RGB range noise model", &prepared.model)?;
    let intensity = gpu.upload("Standalone RGB range conditioning", &prepared.intensity)?;
    let range = gpu.upload("Standalone RGB range weights", &prepared.range)?;
    let mut output = gpu.storage("Standalone RGB range filter", bytes)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    // The standalone mode never reads the mask binding; reuse immutable source
    // backing instead of allocating an unused full-image zero mask.
    gpu.dispatch(
        Kernel::RgbPrefilterRange,
        &mut [
            (0, Read(input)),
            (1, Read(input)),
            (2, Read(&uniform)),
            (3, Read(&model)),
            (4, Read(&intensity)),
            (5, Read(&range)),
            (6, Write(output.write())),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
