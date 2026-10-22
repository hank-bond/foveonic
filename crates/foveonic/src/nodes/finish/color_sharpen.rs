//! Sharpens integral RGB channels with wrapped neighborhood sums and signal-dependent thresholds.

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
pub(crate) struct Params {
    shape: [u32; 4],
    controls: [f32; 4],
}

impl Params {
    /// Inputs are unsigned integral RGB codes stored as Float32. Normalization
    /// maps them to the threshold table's 0..16384 signal domain.
    pub(crate) fn new(extent: [u32; 2], normalization: f32, amount: f32) -> Result<Self> {
        let [w, h] = extent;
        if w < 3
            || h < 2
            || u64::from(w) * u64::from(h) > u64::from(u32::MAX) / 3
            || !normalization.is_finite()
            || normalization <= 0.
            || !amount.is_finite()
            || amount < 0.
        {
            return Err("invalid channel sharpening controls".into());
        }
        Ok(Self {
            shape: [w, h, 0, 0],
            controls: [normalization.recip(), amount, 0., 0.],
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
    let [w, h] = params.extent();
    if input.size() != u64::from(w) * u64::from(h) * 12 {
        return Err("channel sharpening input geometry mismatch".into());
    }
    let table: Vec<f32> = (0..=16384)
        .map(|i| {
            // The reference leaves entry 16384 unwritten. Complete the continuous
            // threshold at its 20-code limit instead of using allocator contents.
            ((16384 - i) as f32 / 16384.).powf(4.).mul_add(380., 20.)
        })
        .collect();
    let table = gpu.upload("Channel sharpening thresholds", &table)?;
    let uniform = gpu.uniform(params);
    let mut output = gpu.storage("Channel sharpening", input.size())?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::ColorSharpen,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&table)),
            (3, Write(output.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "color_sharpen_test.rs"]
mod tests;
