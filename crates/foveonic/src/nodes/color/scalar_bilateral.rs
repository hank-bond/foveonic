//! Filters scalar codes with continuous noise variance and a quantized range table before adaptive blending.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

const RANGE_LENGTH: usize = 2048;
const INTENSITY_LENGTH: usize = 14383;
const BLEND_LENGTH: usize = 16384;
const RANGE_SCALE: f32 = 255.875;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    controls: [f32; 4],
    noise: [[f32; 4]; 3],
}

#[derive(Clone, Debug)]
pub(crate) struct Prepared {
    params: Params,
    intensity: Vec<f32>,
    range: Vec<f32>,
    adaptive: Vec<f32>,
}

impl Prepared {
    /// Noise coefficients are constant, linear and quadratic variance terms.
    /// Intensity weights are addressed from the bright end of the 14-bit domain.
    /// The worker retains its source outside the complete horizontal window and
    /// its four-row bottom margin. Blend weights use the filtered code value.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        extent: [u32; 2],
        kernel: u32,
        noise: [f64; 3],
        strength: f32,
        cutoff: f32,
        intensity: Vec<f32>,
        adaptive: Vec<f32>,
        code_white: f32,
    ) -> Result<Self> {
        if kernel == 0
            || kernel.is_multiple_of(2)
            || kernel > 31
            || extent.iter().any(|v| *v <= kernel + 4)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX)
            || noise.iter().any(|v| !v.is_finite() || *v < 0.)
            || !strength.is_finite()
            || strength <= 0.
            || !cutoff.is_finite()
            || !(0. ..=1.).contains(&cutoff)
            || !code_white.is_finite()
            || code_white <= 0.
            || intensity.len() < INTENSITY_LENGTH
            || intensity.iter().any(|v| !v.is_finite() || *v <= 0.)
            || adaptive.len() != BLEND_LENGTH
            || adaptive
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        {
            return Err("invalid scalar bilateral geometry, variance or weights".into());
        }
        let noise = noise.map(|v| {
            let high = v as f32;
            [high, (v - f64::from(high)) as f32, 0., 0.]
        });
        let range = (0..RANGE_LENGTH)
            .map(|i| {
                let weight = (i as f32 * -0.0039081583_f32).exp();
                if weight >= cutoff {
                    weight
                } else {
                    0.
                }
            })
            .collect();
        Ok(Self {
            params: Params {
                shape: [extent[0], extent[1], kernel / 2, 4],
                controls: [strength, RANGE_SCALE, BLEND_LENGTH as f32 / code_white, 0.],
                noise,
            },
            intensity,
            range,
            adaptive,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn with_extent(mut self, extent: [u32; 2]) -> Result<Self> {
        if extent.iter().any(|v| *v <= self.params.shape[2] * 2 + 5)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX)
        {
            return Err("invalid scalar bilateral output extent".into());
        }
        self.params.shape[..2].copy_from_slice(&extent);
        Ok(self)
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
            .slice(&self.intensity)
            .slice(&self.range)
            .slice(&self.adaptive)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h] = prepared.extent();
    let bytes = u64::from(w) * u64::from(h) * 4;
    if input.size() != bytes {
        return Err("scalar bilateral input extent mismatch".into());
    }
    let uniform = gpu.uniform(&prepared.params);
    let intensity = gpu.upload("Scalar variance weights", &prepared.intensity)?;
    let range = gpu.upload("Scalar range weights", &prepared.range)?;
    let adaptive = gpu.upload("Scalar blend weights", &prepared.adaptive)?;
    let mut output = gpu.storage("Filtered scalar codes", bytes)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::ScalarBilateral,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&intensity)),
            (3, Read(&range)),
            (4, Read(&adaptive)),
            (5, Write(output.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "scalar_bilateral_test.rs"]
mod tests;
