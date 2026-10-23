//! Converts integer processing flags into a binary floating image for inspection.

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
pub struct Params {
    shape: [u32; 4],
}

impl Params {
    pub(crate) fn new(extent: [u32; 2]) -> Result<Self> {
        if extent.contains(&0) || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX)
        {
            return Err("invalid mask image extent".into());
        }
        Ok(Self {
            shape: [extent[0], extent[1], 0, 0],
        })
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.shape[0], self.shape[1]]
    }

    pub(crate) fn key(&self) -> Key {
        Key::pod(self)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    params: &Params,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = params.extent();
    let bytes = u64::from(width) * u64::from(height) * size_of::<u32>() as u64;
    if input.size() != bytes {
        return Err("mask image geometry mismatch".into());
    }
    let uniform = gpu.uniform(params);
    let mut output = gpu.storage("Binary mask image", bytes)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::MaskImage,
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
