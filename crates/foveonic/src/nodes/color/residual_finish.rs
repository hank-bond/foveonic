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
    pub(crate) fn key(&self) -> Key {
        Key::pod(self)
    }
}
pub(crate) struct Output {
    pub rgb: ReadGuard,
    pub detail: ReadGuard,
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    residual: &ReadGuard,
    mask: Option<&ReadGuard>,
    params: &Params,
    profile: &mut Profile,
) -> Result<Output> {
    let [w, h] = params.extent();
    let n = u64::from(w) * u64::from(h);
    if input.size() != n * 12 || residual.size() != n * 4 || mask.is_some_and(|m| m.size() != n * 4)
    {
        return Err("residual finishing input geometry mismatch".into());
    }
    let mut controls = *params;
    controls.shape[3] = u32::from(mask.is_some());
    let empty_mask;
    let mask = if let Some(mask) = mask {
        mask
    } else {
        empty_mask = gpu.upload("Absent detail mask", &[0_u32])?;
        &empty_mask
    };
    let uniform = gpu.uniform(&controls);
    let mut detail = gpu.storage("Signed residual detail", n * 4)?;
    let mut rgb = gpu.storage("Residual-adjusted code RGB", n * 12)?;
    let mut output_detail = gpu.storage("Damped signed residual detail", n * 4)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    gpu.dispatch(
        Kernel::ResidualSharp,
        &mut [
            (0, Read(input)),
            (1, Read(residual)),
            (3, Read(&uniform)),
            (4, Write(detail.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ResidualFinish,
        &mut [
            (0, Read(input)),
            (1, Read(residual)),
            (2, Read(mask)),
            (3, Read(&uniform)),
            (4, Read(&detail)),
            (5, Write(rgb.write())),
            (6, Write(output_detail.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(Output {
        rgb,
        detail: output_detail,
    })
}
#[cfg(test)]
#[path = "residual_finish_test.rs"]
mod tests;
