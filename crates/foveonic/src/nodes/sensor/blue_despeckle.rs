//! Repairs immutable blue planes with paired dark-speckle, general-neighborhood, or neighbor-range equations.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Stage {
    DarkPairs,
    General,
    /// Replaces isolated excursions with the eight-neighbor minimum or maximum.
    NeighborRange,
}
/// Enlarged dark-pair neighborhoods can exceed the physical image at its bottom.
/// The reference reads adjacent allocation bytes there. Callers must choose an
/// owned result instead of making those bytes part of the photograph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MissingContext {
    Preserve,
    Reject,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    bounds: [u32; 4],
    noise: [f32; 4],
    controls: [f32; 4],
}
#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
}
impl Prepared {
    /// Bounds are inclusive physical blue coordinates. Diagnostic inputs may own
    /// tail rows. Missing enlarged neighborhoods follow the supplied policy.
    pub(crate) fn new(
        extent: [u32; 2],
        noise: [f32; 3],
        threshold: f32,
        minimum_difference: f32,
        bounds: [u32; 4],
        stage: Stage,
        missing_context: MissingContext,
    ) -> Result<Self> {
        if extent.iter().any(|&v| v < 8)
            || u64::from(extent[0]) * u64::from(extent[1]) + 2 * u64::from(extent[0])
                > u64::from(u32::MAX)
            || noise.iter().any(|v| !v.is_finite())
            || !threshold.is_finite()
            || threshold < 0.
            || !minimum_difference.is_finite()
            || minimum_difference < 0.
            || bounds[0] > bounds[2]
            || bounds[1] > bounds[3]
            || bounds[2] >= extent[0]
            || bounds[3] >= extent[1]
            || matches!(stage, Stage::DarkPairs) && !extent[1].is_multiple_of(2)
        {
            return Err("invalid blue despeckle parameters".into());
        }
        Ok(Self {
            params: Params {
                shape: [extent[0], extent[1], stage as u32, 0],
                bounds,
                noise: [noise[0], noise[1], noise[2], 0.],
                controls: [
                    threshold,
                    minimum_difference,
                    missing_context as u32 as f32,
                    0.,
                ],
            },
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn stage(&self) -> Stage {
        match self.params.shape[2] {
            0 => Stage::DarkPairs,
            1 => Stage::General,
            2 => Stage::NeighborRange,
            _ => unreachable!("validated numerical stage"),
        }
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
    let [width, height] = prepared.extent();
    let minimum = u64::from(width) * u64::from(height) * 4;
    if input.size() < minimum || !input.size().is_multiple_of(u64::from(width) * 4) {
        return Err("blue despeckle input does not match its physical rows".into());
    }
    let mut output = gpu.storage("Despeckled blue", input.size())?;
    gpu.copy(input, output.write(), input.size());
    let mut status = gpu.upload("Unowned blue context", &[0u32])?;
    let mut params = prepared.params;
    params.shape[3] = u32::try_from(input.size() / 4)?;
    let uniform = gpu.uniform(&params);
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::BlueDespeckle,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
            (3, Write(status.write())),
        ],
        [width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    if matches!(prepared.params.shape[2], 0)
        && prepared.params.controls[2] == MissingContext::Reject as u32 as f32
    {
        let bytes = gpu.readback(&status)?;
        if u32::from_le_bytes(bytes[..4].try_into().unwrap()) != 0 {
            return Err("dark-pair despeckling requires additional owned image context".into());
        }
    }
    Ok(output)
}
#[cfg(test)]
#[path = "blue_despeckle_test.rs"]
mod tests;
