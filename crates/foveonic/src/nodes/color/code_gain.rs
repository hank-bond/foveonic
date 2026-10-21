//! Applies a bounded Q12 gain to signed linear RGB codes independently of filtering.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    source::Result,
};

const Q12_UNIT: f32 = 4096.;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Prepared {
    /// Width, height, truncated Q12 gain, reserved.
    shape: [u32; 4],
}

impl Prepared {
    /// Gains are positive and at most 64. Input codes must fit a signed 16-bit word.
    pub(crate) fn new(extent: [u32; 2], gain: f32) -> Result<Self> {
        if extent.contains(&0) || !gain.is_finite() || gain <= 0. || gain > 64. {
            return Err("invalid signed code gain".into());
        }
        Ok(Self {
            shape: [extent[0], extent[1], (gain * Q12_UNIT) as u32, 0],
        })
    }

    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::pod(self)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height, _, _] = prepared.shape;
    let mut output = gpu.storage(
        "Gained signed RGB codes",
        u64::from(width) * u64::from(height) * 12,
    )?;
    let uniform = gpu.uniform(prepared);
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::CodeGain,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
        ],
        [width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "code_gain_test.rs"]
mod tests;
