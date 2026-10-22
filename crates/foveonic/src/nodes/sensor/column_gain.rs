//! Applies six-channel per-column affine gains while preserving packed sensor cells.
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
        if extent.contains(&0)
            || offset > 65535
            || gains.len() != extent[0] as usize * 6
            || gains.iter().any(|v| !v.is_finite())
        {
            return Err("invalid packed column-gain geometry or coefficients".into());
        }
        Ok(Self {
            params: Params {
                shape: [extent[0], extent[1], 0, 0],
                controls: [offset as f32, 0., 0., 0.],
            },
            gains,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
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
    if input.size() != u64::from(w) * u64::from(h) * 24 {
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
