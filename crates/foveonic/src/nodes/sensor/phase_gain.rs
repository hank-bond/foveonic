//! Resizes interleaved phase gain maps and corrects blue samples below a supplied threshold.

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
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    controls: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Axis {
    index: [u32; 2],
    weight: [f32; 2],
}

#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    map: Vec<f32>,
    axes: Vec<Axis>,
}

impl Prepared {
    /// Maps contain four interleaved parity phases. Odd trailing image rows and
    /// columns remain unchanged. Threshold equality also preserves the sample.
    pub(crate) fn new(
        extent: [u32; 2],
        map_extent: [u32; 2],
        map: Vec<f32>,
        threshold: f32,
    ) -> Result<Self> {
        if extent.iter().any(|&v| v < 2)
            || map_extent.iter().any(|&v| v < 2 || v % 2 != 0)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX)
            || u64::from(map_extent[0]) * u64::from(map_extent[1]) != map.len() as u64
            || map.iter().any(|v| !v.is_finite() || *v <= 0.)
            || !threshold.is_finite()
        {
            return Err("invalid phase gain map geometry or values".into());
        }
        let mut axes = axis(map_extent[0] / 2, extent[0] / 2);
        axes.extend(axis(map_extent[1] / 2, extent[1] / 2));
        Ok(Self {
            params: Params {
                shape: [extent[0], extent[1], map_extent[0], map_extent[1]],
                controls: [threshold, 0., 0., 0.],
            },
            map,
            axes,
        })
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }

    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params).slice(&self.map).slice(&self.axes)
    }
}

/// The reference's zero-support triangular resize has exactly two candidate taps,
/// including during reduction. Weight centers and normalization round to Float32.
fn axis(source: u32, destination: u32) -> Vec<Axis> {
    let ratio = destination as f32 / source as f32;
    let reduced = ratio < 1.;
    let correction = 1. - 1. / ratio;
    (1..=destination)
        .map(|position| {
            let center = correction.mul_add(0.5, position as f32 / ratio);
            let base = center.floor();
            let mut result = Axis {
                index: [0; 2],
                weight: [0.; 2],
            };
            for tap in 0..2 {
                let coordinate = base + tap as f32;
                let distance = (center - coordinate).abs();
                result.index[tap] = (coordinate - 1.).clamp(0., (source - 1) as f32) as u32;
                result.weight[tap] = if reduced {
                    ((1. - f64::from((ratio * distance).clamp(0., 1.))) * f64::from(ratio)) as f32
                } else if distance <= 1. {
                    1. - distance
                } else {
                    0.
                };
            }
            let total = result.weight[0] + result.weight[1];
            result.weight[0] /= total;
            result.weight[1] /= total;
            result
        })
        .collect()
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = prepared.extent();
    if input.size() != u64::from(width) * u64::from(height) * 4 {
        return Err("phase gain input does not match its extent".into());
    }
    let mut output = gpu.storage("Phase gain corrected blue", input.size())?;
    let uniform = gpu.uniform(&prepared.params);
    let map = gpu.upload("Phase gain map", &prepared.map)?;
    let axes = gpu.upload("Phase gain resize axes", &prepared.axes)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::PhaseGain,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&map)),
            (3, Read(&axes)),
            (4, Write(output.write())),
        ],
        [width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "phase_gain_test.rs"]
mod tests;
