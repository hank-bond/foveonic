//! Repairs odd-phase blue runs after validating the source counter initialization precondition.
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};
const MAXIMUM_NEIGHBOR_DEFICIT: f32 = 2000.;
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    extent: [u32; 4],
    rectangle: [u32; 4],
    controls: [f32; 4],
}
#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
}
impl Prepared {
    /// Rectangle coordinates are physical samples. Left/top start the selected odd
    /// phase. Right/bottom are exclusive and retain the worker's two-sample step.
    pub(crate) fn new(extent: [u32; 2], rectangle: [u32; 4], threshold: f32) -> Result<Self> {
        let [w, h] = extent;
        let [left, top, right, bottom] = rectangle;
        if w < 7
            || h < 7
            || left < 3
            || top < 1
            || left % 2 != 1
            || top % 2 != 1
            || right > w - 2
            || bottom > h - 1
            || left >= right
            || top >= bottom
            || !threshold.is_finite()
            || threshold <= 0.
            || u64::from(w) * u64::from(h) > u64::from(u32::MAX)
        {
            return Err("invalid blue mismatch scan geometry".into());
        }
        Ok(Self {
            params: Params {
                extent: [w, h, 0, 0],
                rectangle,
                controls: [threshold, MAXIMUM_NEIGHBOR_DEFICIT, 0., 0.],
            },
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.extent[0], self.params.extent[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
    }
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h] = prepared.extent();
    if input.size() != u64::from(w) * u64::from(h) * 4 {
        return Err("blue mismatch input geometry mismatch".into());
    }
    let mut output = gpu.storage("Blue mismatch repair", input.size())?;
    gpu.copy(input, output.write(), input.size());
    let mut status = gpu.storage("Blue mismatch initialization", 4)?;
    let zero = gpu.upload("Zero initialization status", &[0_u32])?;
    gpu.copy(&zero, status.write(), 4);
    let params = gpu.uniform(&prepared.params);
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::BlueMismatch,
        &mut [
            (0, Read(input)),
            (1, Read(&params)),
            (2, Write(output.write())),
            (3, Write(status.write())),
        ],
        [
            ((prepared.params.rectangle[2] - prepared.params.rectangle[0]).div_ceil(2))
                .div_ceil(64),
            1,
        ],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    if gpu.readback(&status)?.iter().any(|&b| b != 0) {
        return Err(
            "blue mismatch first scan row would read an uninitialized reference counter".into(),
        );
    }
    Ok(output)
}
#[cfg(test)]
#[path = "blue_mismatch_test.rs"]
mod tests;
