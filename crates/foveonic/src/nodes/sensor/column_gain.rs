//! Applies per-column affine gains while preserving triplet or six-channel sensor storage.
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
#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    gains: Vec<f32>,
}
impl Prepared {
    /// Gains are interleaved by physical column, then the six packed channels.
    pub(crate) fn new(extent: [u32; 2], offset: u32, gains: Vec<f32>) -> Result<Self> {
        Self::prepare(extent, offset, gains, false)
    }
    pub(crate) fn triplets(extent: [u32; 2], offset: u32, gains: Vec<f32>) -> Result<Self> {
        Self::prepare(extent, offset, gains, true)
    }
    fn prepare(extent: [u32; 2], offset: u32, gains: Vec<f32>, triplets: bool) -> Result<Self> {
        let components = if triplets { 3 } else { 6 };
        if extent.contains(&0)
            || offset > 65535
            || gains.len() != extent[0] as usize * components
            || gains.iter().any(|v| !v.is_finite())
        {
            return Err("invalid column-gain geometry or coefficients".into());
        }
        Ok(Self {
            params: Params {
                shape: [extent[0], extent[1], if triplets { 3 } else { 0 }, 0],
                controls: [offset as f32, 0., 0., 0.],
            },
            gains,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn is_triplets(&self) -> bool {
        self.params.shape[2] == 3
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params).slice(&self.gains)
    }
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    p: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h] = p.extent();
    let components = if p.is_triplets() { 3 } else { 6 };
    if input.size() != u64::from(w) * u64::from(h) * components * 4 {
        return Err("column-gain input does not match its extent".into());
    }
    let mut output = gpu.storage("Column-corrected sensor cells", input.size())?;
    let uniform = gpu.uniform(&p.params);
    let gains = gpu.upload("Column gain coefficients", &p.gains)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::ColumnGain,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&gains)),
            (3, Write(output.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
#[cfg(test)]
#[path = "column_gain_test.rs"]
mod tests;
