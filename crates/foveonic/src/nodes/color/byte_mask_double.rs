//! Doubles byte-valued masks with the selected cubic phases and exact integer accumulation.
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
        let [w, h] = extent;
        if extent.contains(&0) || u64::from(w) * u64::from(h) > u64::from(u32::MAX) / 4 {
            return Err("invalid mask doubling extent".into());
        }
        Ok(Self {
            shape: [w, h, w * 2, h * 2],
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.shape[0], self.shape[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(self)
    }
}
/// Input and output contain one u32 per sample, with values restricted to0..255.
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    params: &Params,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h] = params.extent();
    let size = u64::from(w) * u64::from(h) * 4;
    if input.size() != size {
        return Err("byte mask geometry mismatch".into());
    }
    let uniform = gpu.uniform(params);
    let mut output = gpu.storage("Doubled byte mask", size * 4)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::ByteMaskDouble,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
        ],
        [(w * 2).div_ceil(8), (h * 2).div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
#[cfg(test)]
#[path = "byte_mask_double_test.rs"]
mod tests;
