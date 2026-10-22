//! Refreshes RGB blue from four spatial blue phases while preserving red and green.
//!
//! The reference callback also visits one padding column. This operation processes
//! logical pixels only, so it never relies on accessible memory beyond an image row.

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
    /// RGB width and height, then reserved words. Blue has twice each dimension.
    shape: [u32; 4],
}

impl Params {
    pub(crate) fn new(extent: [u32; 2]) -> Result<Self> {
        let [width, height] = extent;
        if extent.contains(&0)
            || width > u32::MAX / 2
            || height > u32::MAX / 2
            || u64::from(width) * u64::from(height) > u64::from(u32::MAX) / 4
        {
            return Err("invalid blue refresh extent".into());
        }
        Ok(Self {
            shape: [width, height, 0, 0],
        })
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.shape[0], self.shape[1]]
    }

    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::pod(self)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    rgb: &ReadGuard,
    blue: &ReadGuard,
    params: &Params,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = params.extent();
    let cells = u64::from(width) * u64::from(height);
    if rgb.size() != cells * 12 || blue.size() != cells * 16 {
        return Err("blue refresh inputs do not match their extents".into());
    }
    let mut output = gpu.storage("RGB with refreshed blue", rgb.size())?;
    let uniform = gpu.uniform(params);
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::BlueRefresh,
        &mut [
            (0, Read(rgb)),
            (1, Read(blue)),
            (2, Read(&uniform)),
            (3, Write(output.write())),
        ],
        [width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "blue_refresh_test.rs"]
mod tests;
