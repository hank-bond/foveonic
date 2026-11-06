//! Resamples RGB by one half or two using the reference triangular pyramid kernels.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Ratio {
    Half,
    Double,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Prepared {
    // Source and destination physical dimensions.
    shape: [u32; 4],
    // Doubling flag, horizontal/vertical pass, signed-code rounding flag, reserved.
    controls: [u32; 4],
}

impl Prepared {
    pub(crate) fn new(extent: [u32; 2], ratio: Ratio) -> Result<Self> {
        if extent.contains(&0) || extent.iter().any(|&v| v > i32::MAX as u32 / 2) {
            return Err("invalid RGB pyramid source extent".into());
        }
        let (output, doubling) = match ratio {
            Ratio::Half => (extent.map(|v| v / 2), 0),
            Ratio::Double => return Self::double_to(extent, extent.map(|v| v * 2)),
        };
        if output.contains(&0)
            || u64::from(output[0]) * u64::from(output[1]) > u64::from(u32::MAX) / 3
        {
            return Err("invalid RGB pyramid destination extent".into());
        }
        Ok(Self {
            shape: [extent[0], extent[1], output[0], output[1]],
            controls: [doubling, 0, 0, 0],
        })
    }

    /// Cover odd native sensor edges with the same replicated-edge half filter.
    pub(crate) fn half_ceil(extent: [u32; 2]) -> Result<Self> {
        if extent.contains(&0) || extent.iter().any(|&v| v > i32::MAX as u32 / 2) {
            return Err("invalid RGB pyramid source extent".into());
        }
        let output = extent.map(|v| v.div_ceil(2));
        if u64::from(output[0]) * u64::from(output[1]) > u64::from(u32::MAX) / 3 {
            return Err("invalid RGB pyramid destination extent".into());
        }
        Ok(Self {
            shape: [extent[0], extent[1], output[0], output[1]],
            controls: [0, 0, 0, 0],
        })
    }

    /// Restore a floor-halved odd grid with the existing replicated-edge phases.
    pub(crate) fn double_to(extent: [u32; 2], output: [u32; 2]) -> Result<Self> {
        if extent.contains(&0) || extent.iter().any(|&v| v > i32::MAX as u32 / 2) {
            return Err("invalid RGB pyramid source extent".into());
        }
        let doubled = extent.map(|v| v * 2);
        if output
            .iter()
            .zip(doubled)
            .any(|(&actual, base)| actual != base && actual != base + 1)
            || u64::from(output[0]) * u64::from(output[1]) > u64::from(u32::MAX) / 3
        {
            return Err("invalid RGB pyramid restored extent".into());
        }
        Ok(Self {
            shape: [extent[0], extent[1], output[0], output[1]],
            controls: [1, 0, 0, 0],
        })
    }

    /// Keep Classic signed linear codes integral after each completed 2D level.
    /// Intermediate horizontal sums retain fractional precision.
    pub(crate) fn with_signed_codes(mut self) -> Self {
        self.controls[2] = 1;
        self
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.shape[0], self.shape[1]]
    }
    pub(crate) fn output_extent(&self) -> [u32; 2] {
        [self.shape[2], self.shape[3]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(self)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = prepared.extent();
    let [out_width, out_height] = prepared.output_extent();
    if input.size() != u64::from(width) * u64::from(height) * 12 {
        return Err("RGB pyramid input does not match its extent".into());
    }
    let mut horizontal = gpu.storage(
        "Horizontal RGB pyramid",
        u64::from(out_width) * u64::from(height) * 12,
    )?;
    let mut output = gpu.storage(
        "Resampled RGB pyramid",
        u64::from(out_width) * u64::from(out_height) * 12,
    )?;
    let mut vertical_params = *prepared;
    vertical_params.controls[1] = 1;
    let horizontal_params = gpu.uniform(prepared);
    let vertical_params = gpu.uniform(&vertical_params);
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    gpu.dispatch(
        Kernel::PyramidResample,
        &mut [
            (0, Read(input)),
            (1, Read(&horizontal_params)),
            (2, Write(horizontal.write())),
        ],
        [out_width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::PyramidResample,
        &mut [
            (0, Read(&horizontal)),
            (1, Read(&vertical_params)),
            (2, Write(output.write())),
        ],
        [out_width.div_ceil(8), out_height.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "pyramid_resample_test.rs"]
mod tests;
