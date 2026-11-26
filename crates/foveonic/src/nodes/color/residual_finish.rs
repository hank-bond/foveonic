//! Applies residual gains and derives signed detail from reconstructed unsigned code RGB.
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
    controls: [f32; 4],
}
impl Params {
    pub(crate) fn new(
        extent: [u32; 2],
        scale: f32,
        threshold: f32,
        mode: u32,
        adaptive: f32,
        mask_control: f32,
    ) -> Result<Self> {
        let [w, h] = extent;
        if w < 3
            || h < 3
            || u64::from(w) * u64::from(h) > u64::from(u32::MAX) / 3
            || ![scale, threshold, adaptive, mask_control]
                .iter()
                .all(|v| v.is_finite())
            || scale <= 0.
            || threshold < 0.
            || adaptive < 0.
        {
            return Err("invalid residual finishing controls".into());
        }
        Ok(Self {
            shape: [w, h, mode, 0],
            controls: [scale, threshold, adaptive, mask_control],
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.shape[0], self.shape[1]]
    }
    pub(crate) fn with_extent(mut self, extent: [u32; 2]) -> Result<Self> {
        if extent.iter().any(|&v| v < 3)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
        {
            return Err("invalid regional residual extent".into());
        }
        self.shape[..2].copy_from_slice(&extent);
        Ok(self)
    }
    pub(crate) fn rgb_key(&self) -> Key {
        let [_, _, adaptive_gain, _] = self.controls;
        Key::pod(&self.extent()).with(&adaptive_gain)
    }
    pub(crate) fn detail_key(&self) -> Key {
        let [scale, threshold, _, mask_control] = self.controls;
        Key::pod(&self.shape).with(&[scale, threshold, mask_control])
    }

    fn check_inputs(&self, rgb: &ReadGuard, residual: &ReadGuard) -> Result<()> {
        let [w, h] = self.extent();
        let pixels = u64::from(w) * u64::from(h);
        if rgb.size() != pixels * 12 || residual.size() != pixels * 4 {
            return Err("residual finishing input geometry mismatch".into());
        }
        Ok(())
    }
}

pub(crate) fn rgb(
    gpu: &Gpu,
    input: &ReadGuard,
    residual: &ReadGuard,
    params: &Params,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    params.check_inputs(input, residual)?;
    let [w, h] = params.extent();
    let uniform = gpu.uniform(params);
    let mut output = gpu.storage("Residual-adjusted code RGB", input.size())?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::ResidualFinish,
        &mut [
            (0, Read(input)),
            (1, Read(residual)),
            (3, Read(&uniform)),
            (4, Write(output.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

pub(crate) fn detail(
    gpu: &Gpu,
    input: &ReadGuard,
    residual: &ReadGuard,
    mask: &ReadGuard,
    params: &Params,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    params.check_inputs(input, residual)?;
    if mask.size() != residual.size() {
        return Err("residual detail mask geometry mismatch".into());
    }
    let [w, h] = params.extent();
    let uniform = gpu.uniform(params);
    let mut output = gpu.storage("Damped signed residual detail", residual.size())?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::ResidualSharp,
        &mut [
            (0, Read(input)),
            (1, Read(residual)),
            (2, Read(mask)),
            (3, Read(&uniform)),
            (4, Write(output.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
#[cfg(test)]
#[path = "residual_finish_test.rs"]
pub(crate) mod tests;
