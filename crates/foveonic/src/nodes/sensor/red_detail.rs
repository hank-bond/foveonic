//! Adds signed red-channel detail using a separable five-row binomial support.

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
    /// Width, height, callback strength, reserved.
    pub shape: [u32; 4],
    /// Signed Q14 left/right mixing weights, then reserved values.
    pub weights: [i32; 4],
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    params: &Params,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height, amount, _] = params.shape;
    if amount == 0 {
        return Ok(input.clone());
    }
    let mut horizontal = gpu.storage(
        "Red detail horizontal",
        u64::from(width) * u64::from(height) * 4,
    )?;
    let mut output = gpu.storage(
        "Red detail output",
        u64::from(width) * u64::from(height) * 12,
    )?;
    let uniform = gpu.uniform(params);
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    gpu.dispatch(
        Kernel::RedDetailHorizontal,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(horizontal.write())),
        ],
        [width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::RedDetailApply,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&horizontal)),
            (3, Write(output.write())),
        ],
        [width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "red_detail_test.rs"]
mod tests;
