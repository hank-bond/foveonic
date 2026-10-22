//! Combines full/half-resolution RGB filtering with brightness protection or edge-guided residual blending.

use super::{
    pyramid_resample::{self, Ratio},
    rgb_prefilter,
};
use crate::{
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
struct Params {
    shape: [u32; 4],
    gains: [f32; 4],
}

pub(crate) struct Prepared {
    first: rgb_prefilter::Prepared,
    second: rgb_prefilter::Prepared,
    half: pyramid_resample::Prepared,
    double: pyramid_resample::Prepared,
    params: Params,
    brightness: Vec<f32>,
    adaptive: Option<Adaptive>,
}

struct Adaptive {
    models: [Vec<[f32; 2]>; 2],
    lookup: Vec<f32>,
    minimum: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct AdaptiveParams {
    shape: [u32; 4],
    controls: [f32; 4],
}

pub(crate) struct Output {
    pub(crate) rgb: ReadGuard,
    /// Native Stage1 retains the preprocessing source's first WB round trip.
    pub(crate) source_after: ReadGuard,
}

impl Prepared {
    /// Filtering receives fully calibrated camera RGB before white balance.
    /// The demonstrated half/double route requires each full extent divisible
    /// by four. Tail neighborhoods replicate the final valid row explicitly.
    pub(crate) fn new(
        first: rgb_prefilter::Prepared,
        second: rgb_prefilter::Prepared,
        brightness: Vec<f32>,
        gains: [f32; 3],
    ) -> Result<Self> {
        let extent = first.extent();
        if extent.iter().any(|v| !v.is_multiple_of(4))
            || second.extent() != extent.map(|v| v / 2)
            || brightness.len() != 14344
            || brightness.iter().any(|v| !v.is_finite())
            || gains.iter().any(|v| !v.is_finite() || *v <= 0.)
        {
            return Err("invalid two-scale RGB filter parameters".into());
        }
        let half = pyramid_resample::Prepared::new(extent, Ratio::Half)?;
        let double = pyramid_resample::Prepared::new(second.extent(), Ratio::Double)?;
        Ok(Self {
            first,
            second,
            half,
            double,
            params: Params {
                shape: [extent[0], extent[1], 0, 0],
                gains: [gains[0], gains[1], gains[2], 0.],
            },
            brightness,
            adaptive: None,
        })
    }
    /// Enables two-axis edge-guided means and residual-damped scale blending.
    /// Each noise model is in its corresponding unbalanced RGB code units.
    pub(crate) fn with_adaptive(
        mut self,
        models: [[f64; 9]; 2],
        tanh: Vec<f32>,
        minimum: f32,
    ) -> Result<Self> {
        if models.iter().flatten().any(|v| !v.is_finite())
            || tanh.len() != 1024
            || tanh
                .iter()
                .any(|v| !v.is_finite() || !(-1. ..=0.).contains(v))
            || !minimum.is_finite()
            || !(0. ..=1.).contains(&minimum)
        {
            return Err("invalid adaptive scale-filter controls".into());
        }
        let models = models.map(|model| {
            model
                .into_iter()
                .map(|value| {
                    let high = value as f32;
                    [high, (value - f64::from(high)) as f32]
                })
                .collect()
        });
        let mut lookup = self.brightness.clone();
        lookup.extend(tanh);
        self.adaptive = Some(Adaptive {
            models,
            lookup,
            minimum,
        });
        Ok(self)
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn key(&self) -> Key {
        let key = Key::pod(&self.params)
            .slice(&self.brightness)
            .join(self.first.key())
            .join(self.second.key())
            .join(self.half.key())
            .join(self.double.key());
        if let Some(adaptive) = &self.adaptive {
            key.slice(&adaptive.models[0])
                .slice(&adaptive.models[1])
                .slice(&adaptive.lookup)
                .slice(&[adaptive.minimum])
        } else {
            key
        }
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<Output> {
    let first = rgb_prefilter::apply(gpu, input, &prepared.first, profile)?;
    let current = if let Some(adaptive) = &prepared.adaptive {
        guided_mean(
            gpu,
            &first.rgb,
            &first.source_after,
            prepared.extent(),
            &adaptive.models[0],
            profile,
        )?
    } else {
        first.rgb
    };
    let half = pyramid_resample::apply(gpu, &first.source_after, &prepared.half, profile)?;
    let second = rgb_prefilter::apply(gpu, &half, &prepared.second, profile)?;
    let second = if let Some(adaptive) = &prepared.adaptive {
        guided_mean(
            gpu,
            &second.rgb,
            &second.source_after,
            prepared.second.extent(),
            &adaptive.models[1],
            profile,
        )?
    } else {
        second.rgb
    };
    let candidate = pyramid_resample::apply(gpu, &second, &prepared.double, profile)?;
    let rgb = if let Some(adaptive) = &prepared.adaptive {
        residual_blend(
            gpu,
            &current,
            &candidate,
            prepared.extent(),
            &adaptive.lookup,
            adaptive.minimum,
            profile,
        )?
    } else {
        protect(
            gpu,
            &current,
            &candidate,
            &first.source_after,
            &prepared.params,
            &prepared.brightness,
            profile,
        )?
    };
    Ok(Output {
        rgb,
        source_after: first.source_after,
    })
}

fn guided_mean(
    gpu: &Gpu,
    input: &ReadGuard,
    original: &ReadGuard,
    extent: [u32; 2],
    model: &[[f32; 2]],
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 4;
    let params = AdaptiveParams {
        shape: [extent[0], extent[1], 0, 0],
        controls: [0.9, 0., 0., 0.],
    };
    let uniform = gpu.uniform(&params);
    let model = gpu.upload("Edge-guide variance model", model)?;
    let mut guide = gpu.storage("Edge-guide weights", bytes)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::NoiseEdgeGuide,
        &mut [
            (0, Read(original)),
            (1, Read(&uniform)),
            (2, Read(&model)),
            (3, Write(guide.write())),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    let (horizontal, transposed_guide) = recursive_mean(gpu, input, &guide, extent, profile)?;
    let (result, _) = recursive_mean(
        gpu,
        &horizontal,
        &transposed_guide,
        [extent[1], extent[0]],
        profile,
    )?;
    Ok(result)
}

fn recursive_mean(
    gpu: &Gpu,
    input: &ReadGuard,
    guide: &ReadGuard,
    extent: [u32; 2],
    profile: &mut Profile,
) -> Result<(ReadGuard, ReadGuard)> {
    let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 4;
    let uniform = gpu.uniform(&AdaptiveParams {
        shape: [extent[0], extent[1], 0, 0],
        controls: [0.9, 0., 0., 0.],
    });
    let mut output = gpu.storage("Transposed recursive RGB mean", bytes * 3)?;
    let mut transposed = gpu.storage("Transposed edge guide", bytes)?;
    let mut scratch = gpu.storage("Recursive forward RGB and weight", bytes * 4)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::NoiseRecursiveMean,
        &mut [
            (0, Read(input)),
            (1, Read(guide)),
            (2, Read(&uniform)),
            (3, Write(output.write())),
            (4, Write(transposed.write())),
            (5, Write(scratch.write())),
        ],
        [extent[1].div_ceil(64), 1],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok((output, transposed))
}

fn residual_blend(
    gpu: &Gpu,
    current: &ReadGuard,
    candidate: &ReadGuard,
    extent: [u32; 2],
    lookup: &[f32],
    minimum: f32,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let uniform = gpu.uniform(&AdaptiveParams {
        shape: [extent[0], extent[1], 0, 0],
        controls: [0., minimum, 0., 0.],
    });
    let lookup = gpu.upload("Residual brightness and damping", lookup)?;
    let mut output = gpu.storage("Residual-damped RGB scales", current.size())?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::NoiseResidualBlend,
        &mut [
            (0, Read(current)),
            (1, Read(candidate)),
            (2, Read(&uniform)),
            (3, Read(&lookup)),
            (4, Write(output.write())),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

fn protect(
    gpu: &Gpu,
    current: &ReadGuard,
    candidate: &ReadGuard,
    original: &ReadGuard,
    params: &Params,
    lookup: &[f32],
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let extent = [params.shape[0], params.shape[1]];
    let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 12;
    if [current, candidate, original]
        .iter()
        .any(|v| v.size() != bytes)
    {
        return Err("RGB protection inputs have different extents".into());
    }
    let uniform = gpu.uniform(params);
    let mut weights = gpu.storage("RGB filter protection weights", bytes / 3)?;
    let mut output = gpu.storage("Two-scale filtered RGB", bytes)?;
    let brightness = gpu.upload("Filter brightness protection", lookup)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    gpu.dispatch(
        Kernel::NoiseProtectionMap,
        &mut [
            (0, Read(current)),
            (1, Read(&uniform)),
            (2, Write(weights.write())),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseProtectionBlend,
        &mut [
            (0, Read(current)),
            (1, Read(candidate)),
            (2, Read(original)),
            (3, Read(&weights)),
            (4, Read(&uniform)),
            (5, Read(&brightness)),
            (6, Write(output.write())),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "two_scale_noise_test.rs"]
mod tests;
