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
    render::ProcessingQuality,
    source::Result,
};

/// Native RGB supports the fine structure probe and half-grid gain interpolation.
/// The broad probe samples an independently prepared whole-photo guide.
pub(crate) const REGIONAL_SUPPORT: u32 = 2 + 2;

pub(crate) fn support() -> u32 {
    tone_equalizer::support() + 8 + 2
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    shape: [u32; 4],
    // Guide step, gain step, dense analysis, same-grid mapped application.
    sampling: [u32; 4],
    bounds: [u32; 4],
    luma: [f32; 4],
    values: [f32; 4],
    analysis: [f32; 4],
    limits: [f32; 4],
    natural_limits: [f32; 4],
    toe: [f32; 4],
    guide: tone_equalizer::GuideSampling,
}
impl Params {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        extent: [u32; 2],
        bounds: [u32; 4],
        white: f32,
        space: ColorSpace,
        response: BaseResponse,
        tuning: Tuning,
        curve: &[f32],
        processing_quality: ProcessingQuality,
    ) -> Result<Self> {
        tuning.validate()?;
        if extent.contains(&0)
            || bounds[0] >= bounds[2]
            || bounds[1] >= bounds[3]
            || bounds[2] > extent[0]
            || bounds[3] > extent[1]
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
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
        let step = tone_equalizer::GUIDE_SCALE;
        let analysis_extent = if processing_quality == ProcessingQuality::Quality {
            [bounds[2] - bounds[0], bounds[3] - bounds[1]]
        } else {
            [256, 256]
        };
        Ok(Self {
            shape: [extent[0], extent[1], analysis_extent[0], analysis_extent[1]],
            sampling: [
                step,
                step,
                u32::from(processing_quality == ProcessingQuality::Quality),
                0,
            ],
            bounds,
            luma: photographic::luminance(space),
            values: [white, pivot, response.strength(), 1.],
            analysis: [0.; 4],
            limits: tuning.for_response(response).values(),
            natural_limits: tuning.natural.values(),
            toe: [toe, toe_slope, 2., 8.],
            guide: Default::default(),
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
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
            || !scale.is_finite()
            || scale <= 0.
        {
            return Err("invalid adaptive analysis geometry".into());
        }
        self.shape[..2].copy_from_slice(&extent);
        self.bounds = bounds;
        if self.sampling[2] != 0 {
            self.shape[2] = bounds[2] - bounds[0];
            self.shape[3] = bounds[3] - bounds[1];
        }
        // A zero-distance probe compares a pixel with itself and erases every
        // structure score. Keep the smallest usable neighborhood on coarse grids.
        self.toe[2] = (2. * scale).round().max(1.);
        self.toe[3] = (8. * scale).round().max(1.);
        Ok(self)
    }
    /// Map main RGB sample centers to sample centers in a separate guide image.
    /// The guide carries its own encoded-cell lattice. Same-grid application
    /// preserves Whole padding anchors and nearest-cell structure probes.
    pub(crate) fn with_application_guide(
        mut self,
        guide: Self,
        mapping: [[f32; 4]; 2],
        same_grid: bool,
    ) -> Result<Self> {
        self.sampling[3] = u32::from(same_grid);
        self.guide = tone_equalizer::GuideSampling::new(
            guide.extent(),
            guide.bounds,
            mapping,
            guide.sampling[0],
            self.sampling[1],
        )?;
        Ok(self)
    }

    fn guide_extent(self) -> [u32; 2] {
        if !self.guide.is_mapped() {
            self.extent()
        } else {
            self.guide.extent()
        }
    }

    pub(crate) fn extent(self) -> [u32; 2] {
        [self.shape[0], self.shape[1]]
    }

    pub(crate) fn aligned_output(self, bounds: [u32; 4]) -> Result<[u32; 4]> {
        let alignment = tone_equalizer::GUIDE_ALIGNMENT;
        if bounds[0] >= bounds[2] || bounds[1] >= bounds[3] {
            return Err("invalid adaptive output bounds".into());
        }
        Ok([
            bounds[0] / alignment * alignment,
            bounds[1] / alignment * alignment,
            bounds[2]
                .div_ceil(alignment)
                .saturating_mul(alignment)
                .min(self.shape[0]),
            bounds[3]
                .div_ceil(alignment)
                .saturating_mul(alignment)
                .min(self.shape[1]),
        ])
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
    for (samples, geometry) in [(&input, &params), (&analysis, &analysis_params)] {
        let extent = [geometry.shape[0], geometry.shape[1]];
        if samples.rgb.size() != u64::from(extent[0]) * u64::from(extent[1]) * 12
            || samples.mask.size()
                != tone_equalizer::guide_storage_bytes(
                    geometry.guide_extent(),
                    if geometry.guide.is_mapped() {
                        geometry.guide.cell_step()
                    } else {
                        geometry.sampling[0]
                    },
                )
        {
            return Err("adaptive RGB or compact brightness guide has an unexpected size".into());
        }
    }
    let Samples { rgb: input, mask } = input;
    let [width, height, _, _] = params.shape;
    let curve = gpu.upload("adaptive tone parameters", curve)?;
    let mut buffers = FitBuffers {
        curve,
        histogram: gpu.storage("adaptive joint histogram", 4096 * 3 * 4)?,
        candidates: gpu.storage("adaptive candidate scores", 96 * 16)?,
        chosen: gpu.storage("adaptive selected response", 5 * 4)?,
        reference: gpu.storage("adaptive reference response", 5 * 4)?,
        global_reference: gpu.storage("adaptive initial response", 5 * 4)?,
        regions: gpu.storage("adaptive region summaries", 1235 * 64)?,
    };
    let analysis_extent = [analysis_params.shape[2], analysis_params.shape[3]];
    let mut samples = gpu.storage(
        "adaptive regional samples",
        u64::from(analysis_extent[0]) * u64::from(analysis_extent[1]) * 16,
    )?;
    let step = params.sampling[1];
    let gain_extent = [width.div_ceil(step), height.div_ceil(step)];
    let mut gain = gpu.storage(
        "adaptive HDR gains",
        u64::from(gain_extent[0]) * u64::from(gain_extent[1]) * 4,
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
        [
            analysis_extent[0].div_ceil(8),
            analysis_extent[1].div_ceil(8),
        ],
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
        [
            analysis_extent[0].div_ceil(8),
            analysis_extent[1].div_ceil(8),
        ],
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
        if params.sampling[3] != 0 {
            Kernel::AdaptiveToneGainSameGrid
        } else {
            Kernel::AdaptiveToneGain
        },
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
        [gain_extent[0].div_ceil(8), gain_extent[1].div_ceil(8)],
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
        shape: [width, height, gain_extent[0], gain_extent[1]],
        amounts: [0.; 4],
        luma: [0.; 4],
        pivot: [params.values[1], step as f32, 0., 0.],
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
