//! Separates six-sample cells into RGB and spatial blue codes with affine or channel-gain calibration.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    source::Result,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Params {
    /// Cell width and height, calibration mode, reserved.
    shape: [u32; 4],
    /// Offset, scale, calibrated upper bound, reserved.
    calibration: [f32; 4],
    /// Split Double channel gains. The fourth component is reserved.
    gain_high: [f32; 4],
    gain_low: [f32; 4],
}

const AFFINE_CALIBRATION: u32 = 1;
const CHANNEL_GAIN_CALIBRATION: u32 = 2;
const CHANNEL_CODE_CEILING: f32 = u16::MAX as f32;

impl Params {
    /// Each cell contains R, G, B00, B01, B10, B11 in row-major order.
    /// The blue output has twice the cell width and height. No calibration,
    /// clipping, normalization or interpolation is applied to sensor codes.
    pub(crate) fn new(width: u32, height: u32) -> Result<Self> {
        let cells = u64::from(width) * u64::from(height);
        if width == 0
            || height == 0
            || width > u32::MAX / 2
            || height > u32::MAX / 2
            || cells > u64::from(u32::MAX) / 6
        {
            return Err("invalid six-sample cell extent".into());
        }
        Ok(Self {
            shape: [width, height, 0, 0],
            calibration: [0., 1., f32::MAX, 0.],
            gain_high: [1., 1., 1., 0.],
            gain_low: [0.; 4],
        })
    }

    /// Applies offset removal and scaling, with an independent mono upper bound.
    /// RGB blue is averaged before correction. Negative codes remain signed.
    pub(crate) fn calibrated(
        width: u32,
        height: u32,
        offset: u32,
        scale: f32,
        mono_cap: f32,
    ) -> Result<Self> {
        let mut result = Self::new(width, height)?;
        if offset > 65535
            || !scale.is_finite()
            || scale <= 0.
            || !mono_cap.is_finite()
            || mono_cap <= 0.
        {
            return Err("invalid six-sample calibration".into());
        }
        result.shape[2] = AFFINE_CALIBRATION;
        result.calibration = [offset as f32, scale, mono_cap, 0.];
        Ok(result)
    }

    /// Round offset removal and scale to Float32 before multiplying each channel
    /// by its Double gain. Cap RGB and all blue phases after that multiplication.
    pub(crate) fn channel_calibrated(
        width: u32,
        height: u32,
        offset: u32,
        scale: f32,
        gains: [f64; 3],
    ) -> Result<Self> {
        let mut result = Self::calibrated(width, height, offset, scale, CHANNEL_CODE_CEILING)?;
        if gains
            .iter()
            .any(|&g| !g.is_finite() || g <= 0. || !(g as f32).is_finite())
        {
            return Err("invalid extraction channel gains".into());
        }
        result.shape[2] = CHANNEL_GAIN_CALIBRATION;
        for (channel, gain) in gains.into_iter().enumerate() {
            let high = gain as f32;
            result.gain_high[channel] = high;
            result.gain_low[channel] = (gain - f64::from(high)) as f32;
        }
        Ok(result)
    }

    pub(crate) fn is_calibrated(&self) -> bool {
        self.shape[2] != 0
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.shape[0], self.shape[1]]
    }

    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::pod(self)
    }
}

/// Returns RGB cell codes with mean blue, then the four spatial blue phases.
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    params: &Params,
    profile: &mut Profile,
) -> Result<(ReadGuard, ReadGuard)> {
    let [width, height] = params.extent();
    let cells = u64::from(width) * u64::from(height);
    if input.size() != cells * 24 {
        return Err("six-sample input size does not match its cell extent".into());
    }
    let mut rgb = gpu.storage("Extracted RGB codes", cells * 12)?;
    let mut blue = gpu.storage("Extracted blue codes", cells * 16)?;
    let uniform = gpu.uniform(params);
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::PhaseExtraction,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(rgb.write())),
            (3, Write(blue.write())),
        ],
        [width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok((rgb, blue))
}

#[cfg(test)]
#[path = "phase_extraction_test.rs"]
mod tests;
