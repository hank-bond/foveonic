//! Fits regional visibility and rescues structured near-black detail through one spatial RGB gain.

use super::{
    base_response::{BaseResponse, Tuning},
    photographic, tone_equalizer,
};
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

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    shape: [u32; 4],
    bounds: [u32; 4],
    luma: [f32; 4],
    values: [f32; 4],
    analysis: [f32; 4],
    limits: [f32; 4],
    natural_limits: [f32; 4],
    toe: [f32; 4],
}
impl Params {
    pub(crate) fn new(
        extent: [u32; 2],
        bounds: [u32; 4],
        white: f32,
        space: ColorSpace,
        response: BaseResponse,
        tuning: Tuning,
        curve: &[f32],
    ) -> Result<Self> {
        tuning.validate()?;
        if extent.contains(&0)
            || bounds[0] >= bounds[2]
            || bounds[1] >= bounds[3]
            || bounds[2] > extent[0]
            || bounds[3] > extent[1]
        {
            return Err("invalid adaptive tone geometry".into());
        }
        let pivot = tone_equalizer::pivot_with_white(curve, 0., white)?;
        let encoded_toe = 0.025 + response.strength() * 0.01;
        let linear_toe =
            crate::color_space::TransferFunction::from(crate::render::TiffColorSpace::Srgb)
                .decode(f64::from(encoded_toe)) as f32;
        let toe = tone_equalizer::input_exposure_for_output(curve, linear_toe, white)?;
        let toe_slope = if curve[0] < linear_toe {
            0.3 + 0.1 * response.strength()
        } else {
            0.
        };
        Ok(Self {
            shape: [extent[0], extent[1], 256, 256],
            bounds,
            luma: photographic::luminance(space),
            values: [white, pivot, response.strength(), 1.],
            analysis: [0.; 4],
            limits: tuning.for_response(response).values(),
            natural_limits: tuning.natural.values(),
            toe: [toe, toe_slope, 1., 0.],
        })
    }
    pub(crate) fn with_exposure(mut self, exposure: f32) -> Result<Self> {
        let gain = exposure.exp2();
        if !exposure.is_finite() || !gain.is_finite() || gain <= 0. {
            return Err("invalid adaptive tone exposure".into());
        }
        self.values[1] -= exposure;
        self.values[3] = gain;
        self.toe[0] -= exposure;
        Ok(self)
    }
    pub(crate) fn with_geometry(
        mut self,
        extent: [u32; 2],
        bounds: [u32; 4],
        scale: f32,
    ) -> Result<Self> {
        if extent.contains(&0)
            || bounds[0] >= bounds[2]
            || bounds[1] >= bounds[3]
            || bounds[2] > extent[0]
            || bounds[3] > extent[1]
            || !scale.is_finite()
            || scale <= 0.
        {
            return Err("invalid adaptive analysis geometry".into());
        }
        self.shape[..2].copy_from_slice(&extent);
        self.bounds = bounds;
        self.toe[2] = scale;
        Ok(self)
    }
    pub(crate) fn key(self) -> Key {
        Key::pod(&self)
    }
}

pub(crate) struct Output {
    pub rgb: ReadGuard,
    /// Current fitted amounts for numerical regression checks.
    #[cfg(test)]
    pub amounts: ReadGuard,
}

pub(crate) struct Samples<'a> {
    pub rgb: &'a ReadGuard,
    pub mask: &'a ReadGuard,
}

/// Reuse the numerical RGB resampler for an analysis-only quarter-pixel overview.
pub(crate) fn overview(
    gpu: &Gpu,
    input: &ReadGuard,
    extent: [u32; 2],
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let target = extent.map(|v| v.div_ceil(2));
    let prepared = super::resample::Prepared::new(extent, target, 1, 0.5)?;
    super::resample::apply(gpu, input, &prepared, profile)
}

#[cfg(test)]
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    mask: &ReadGuard,
    curve: &[f32],
    params: Params,
    profile: &mut Profile,
) -> Result<Output> {
    apply_with_analysis(
        gpu,
        Samples { rgb: input, mask },
        Samples { rgb: input, mask },
        curve,
        params,
        params,
        profile,
    )
}

pub(crate) fn apply_with_analysis(
    gpu: &Gpu,
    input: Samples<'_>,
    analysis: Samples<'_>,
    curve: &[f32],
    params: Params,
    analysis_params: Params,
    profile: &mut Profile,
) -> Result<Output> {
    let Samples { rgb: input, mask } = input;
    let [width, height, _, _] = params.shape;
    let curve = gpu.upload("adaptive tone parameters", curve)?;
    let mut buffers = FitBuffers {
        curve,
        histogram: gpu.storage("adaptive joint histogram", 4096 * 2 * 4)?,
        candidates: gpu.storage("adaptive candidate scores", 96 * 16)?,
        chosen: gpu.storage("adaptive selected response", 5 * 4)?,
        reference: gpu.storage("adaptive reference response", 5 * 4)?,
        global_reference: gpu.storage("adaptive initial response", 5 * 4)?,
        regions: gpu.storage("adaptive region summaries", 1235 * 64)?,
    };
    let mut samples = gpu.storage("adaptive regional samples", 256 * 256 * 16)?;
    let mut gain = gpu.storage(
        "adaptive HDR gains",
        u64::from(width.div_ceil(2)) * u64::from(height.div_ceil(2)) * 4,
    )?;
    let mut output = gpu.storage(
        "adaptive visible RGB",
        u64::from(width) * u64::from(height) * 12,
    )?;
    let context = gpu.uniform(&params);
    let analysis_context = gpu.uniform(&analysis_params);
    let mut trace = gpu.trace(profile.mode.gpu(), 32);
    gpu.dispatch(
        Kernel::AdaptiveToneHistogram,
        &mut [
            (0, Read(analysis.rgb)),
            (1, Read(analysis.mask)),
            (3, Read(&analysis_context)),
            (4, Write(buffers.histogram.write())),
        ],
        [32, 32],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::AdaptiveToneSampleRegions,
        &mut [
            (0, Read(analysis.rgb)),
            (1, Read(analysis.mask)),
            (3, Read(&analysis_context)),
            (8, Write(samples.write())),
        ],
        [32, 32],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::AdaptiveToneRegionSummary,
        &mut [
            (2, Read(&buffers.curve)),
            (3, Read(&analysis_context)),
            (8, Read(&samples)),
            (9, Write(buffers.regions.write())),
        ],
        [1235_u32.div_ceil(64), 1],
        &mut trace,
    );
    let natural = Params {
        values: [params.values[0], params.values[1], 0., params.values[3]],
        limits: params.natural_limits,
        ..params
    };
    fit(gpu, &mut buffers, natural, false, &mut trace);
    gpu.copy(&buffers.chosen, buffers.global_reference.write(), 20);
    checkpoint(gpu, &mut buffers);
    fit(gpu, &mut buffers, natural, true, &mut trace);
    if params.values[2] > 0. {
        checkpoint(gpu, &mut buffers);
        fit(gpu, &mut buffers, params, true, &mut trace);
    }
    gpu.dispatch(
        Kernel::AdaptiveToneGain,
        &mut [
            (0, Read(input)),
            (1, Read(mask)),
            (2, Read(&buffers.curve)),
            (3, Read(&context)),
            (6, Read(&buffers.chosen)),
            (10, Read(&buffers.reference)),
            (11, Read(&buffers.global_reference)),
            (7, Write(gain.write())),
        ],
        [
            width.div_ceil(2).div_ceil(8),
            height.div_ceil(2).div_ceil(8),
        ],
        &mut trace,
    );
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Adjustment {
        shape: [u32; 4],
        amounts: [f32; 4],
        luma: [f32; 4],
        pivot: [f32; 4],
    }
    let adjustment = gpu.uniform(&Adjustment {
        shape: [width, height, width.div_ceil(2), height.div_ceil(2)],
        amounts: [0.; 4],
        luma: [0.; 4],
        pivot: [params.values[1], 2., 0., 0.],
    });
    gpu.dispatch(
        Kernel::ToneEqualizerApply,
        &mut [
            (0, Read(input)),
            (6, Read(&adjustment)),
            (7, Write(output.write())),
            (11, Read(&gain)),
        ],
        [width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(Output {
        rgb: output,
        #[cfg(test)]
        amounts: buffers.chosen,
    })
}

struct FitBuffers {
    curve: ReadGuard,
    histogram: ReadGuard,
    candidates: ReadGuard,
    chosen: ReadGuard,
    reference: ReadGuard,
    global_reference: ReadGuard,
    regions: ReadGuard,
}
fn checkpoint(gpu: &Gpu, buffers: &mut FitBuffers) {
    gpu.copy(&buffers.chosen, buffers.reference.write(), 20);
}

fn fit(
    gpu: &Gpu,
    buffers: &mut FitBuffers,
    mut params: Params,
    regional: bool,
    trace: &mut crate::gpu::GpuTrace,
) {
    params.analysis[3] = f32::from(regional);
    for step in [0., 0.16, 0.06] {
        params.analysis[2] = step;
        let context = gpu.uniform(&params);
        let mut bindings = vec![
            (2, Read(&buffers.curve)),
            (3, Read(&context)),
            (4, Read(&buffers.histogram)),
            (5, Write(buffers.candidates.write())),
            (6, Read(&buffers.chosen)),
            (10, Read(&buffers.reference)),
        ];
        if regional || step > 0. {
            bindings.push((9, Read(&buffers.regions)));
        }
        gpu.dispatch(
            if step > 0. {
                Kernel::AdaptiveToneRefine
            } else if regional {
                Kernel::AdaptiveToneRegionFit
            } else {
                Kernel::AdaptiveToneFit
            },
            &mut bindings,
            [if step > 0. { 8 } else { 24 }, 1],
            trace,
        );
        gpu.dispatch(
            Kernel::AdaptiveToneSelect,
            &mut [
                (3, Read(&context)),
                (5, Read(&buffers.candidates)),
                (6, Write(buffers.chosen.write())),
            ],
            [1, 1],
            trace,
        );
    }
}

#[cfg(test)]
#[path = "adaptive_tone_test.rs"]
mod tests;
