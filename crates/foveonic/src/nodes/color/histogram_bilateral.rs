//! Filters a scalar code plane with quantized range weights and an adaptive residual blend.
//!
//! Independent neighborhoods replace the reference implementation's serial sliding
//! histogram. Quantization and range weights agree; Float32 accumulation differs.
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

const BINS: usize = 1024;
const BIN_WIDTH: usize = 16;
const ADAPTIVE_LENGTH: usize = 16384;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    controls: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    ranges: Vec<[u32; 4]>,
    weights: Vec<f32>,
    adaptive: Vec<f32>,
}

impl Prepared {
    /// Input samples are Float32 camera codes in the 14-bit range domain.
    /// Whole windows are required; the physical margin retains its input values.
    pub(crate) fn new(
        extent: [u32; 2],
        kernel: u32,
        coefficients: [f64; 3],
        strength: f32,
        cutoff: f32,
        adaptive: Vec<f32>,
        normalization: f32,
    ) -> Result<Self> {
        if kernel == 0
            || kernel.is_multiple_of(2)
            || kernel > 31
            || extent.iter().any(|&v| v < kernel)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 4
            || coefficients.iter().any(|v| !v.is_finite() || *v < 0.)
            || !strength.is_finite()
            || strength <= 0.
            || !cutoff.is_finite()
            || !(0. ..=1.).contains(&cutoff)
            || !normalization.is_finite()
            || normalization <= 0.
            || adaptive.len() != ADAPTIVE_LENGTH
            || adaptive
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        {
            return Err("invalid histogram bilateral controls".into());
        }
        let [a, b, c] = coefficients;
        let cutoff_boundary = if cutoff > 0. {
            // Account for rounding exp to Float32 before the native cutoff.
            let previous = f32::from_bits(cutoff.to_bits() - 1);
            Some((f64::from(cutoff) + f64::from(previous)) * 0.5)
        } else {
            None
        };
        let mut ranges = Vec::with_capacity(BINS);
        let mut weights = Vec::new();
        for center in 0..BINS {
            let x = (center * BIN_WIDTH) as f64;
            let variance = (c * x).mul_add(x, b.mul_add(x, a)) / f64::from(strength);
            if !variance.is_finite() || variance <= 0. {
                return Err("invalid histogram bilateral variance".into());
            }
            let mut span = ((variance.min(i32::MAX as f64) as usize) >> 4).min(BINS);
            if let Some(boundary) = cutoff_boundary {
                // The native initial range can span all bins. Skip only bins
                // whose weights are certainly below cutoff; retain one extra
                // bin and apply the original exp/Float32 comparison below.
                let bound = ((-variance * boundary.ln()).sqrt() / BIN_WIDTH as f64)
                    .ceil()
                    .min(BINS as f64) as usize;
                span = span.min(bound + 1);
            }
            let first = center.saturating_sub(span);
            let last = (center + span).min(BINS - 1);
            let offset = weights.len() as u32;
            let mut begin = first;
            let mut started = false;
            for neighbor in first..=last {
                let difference = (neighbor as f64 - center as f64) * BIN_WIDTH as f64;
                let weight = (-(difference * difference) / variance).exp() as f32;
                if weight == 0. || weight < cutoff {
                    if started {
                        break;
                    }
                    continue;
                }
                if !started {
                    begin = neighbor;
                    started = true;
                }
                weights.push(weight);
            }
            ranges.push([begin as u32, offset, weights.len() as u32 - offset, 0]);
        }
        Ok(Self {
            params: Params {
                shape: [extent[0], extent[1], kernel / 2, 0],
                controls: [ADAPTIVE_LENGTH as f32 / normalization, 0., 0., 0.],
            },
            ranges,
            weights,
            adaptive,
        })
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn with_extent(mut self, extent: [u32; 2]) -> Result<Self> {
        if extent.iter().any(|&v| v <= self.params.shape[2] * 2)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 4
        {
            return Err("invalid regional blue filter extent".into());
        }
        self.params.shape[..2].copy_from_slice(&extent);
        Ok(self)
    }

    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
            .slice(&self.ranges)
            .slice(&self.weights)
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
        return Err("histogram bilateral input geometry mismatch".into());
    }
    let uniform = gpu.uniform(&prepared.params);
    let ranges = gpu.upload("Quantized bilateral ranges", &prepared.ranges)?;
    let weights = gpu.upload("Quantized bilateral weights", &prepared.weights)?;
    let adaptive = gpu.upload("Scalar adaptive blend", &prepared.adaptive)?;
    let mut output = gpu.storage("Filtered scalar codes", bytes)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::HistogramBilateral,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&ranges)),
            (3, Read(&weights)),
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
#[path = "histogram_bilateral_test.rs"]
mod tests;
