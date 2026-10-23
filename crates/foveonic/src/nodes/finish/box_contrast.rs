//! Applies ordered box-mean local contrast to integral RGB codes with explicit noise and gain bounds.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

// Largest supported radius is seven. Keep both edges and a nonempty interior.
const MAX_RADIUS: u32 = 7;
const MIN_SIDE: u32 = 2 * MAX_RADIUS + 2;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    controls: [f32; 4],
}

/// Owns the ordered pass controls. Pixels outside each radius retain their source.
/// The native physical-edge scratch race is replaced by deterministic source
/// values. The affected margins lie outside the supported cameras' active crop.
pub struct Prepared {
    passes: [Params; 2],
}

impl Prepared {
    pub(crate) fn new(extent: [u32; 2], passes: [(u32, f32); 2], iso_gain: f32) -> Result<Self> {
        if extent.iter().any(|&v| v < MIN_SIDE)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
            || !iso_gain.is_finite()
            || iso_gain <= 0.
            || passes
                .iter()
                .any(|(radius, amount)| !(1..=MAX_RADIUS).contains(radius) || !amount.is_finite())
        {
            return Err("invalid box local-contrast controls".into());
        }
        Ok(Self {
            passes: passes.map(|(radius, amount)| Params {
                shape: [extent[0], extent[1], radius, 0],
                // Mean coefficient, selected amount, noise deadband and gain bound.
                controls: [
                    1. / (radius * 2 + 1) as f32,
                    amount,
                    3.3333333_f32 * iso_gain,
                    0.1,
                ],
            }),
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.passes[0].shape[0], self.passes[0].shape[1]]
    }
    pub(crate) fn with_extent(mut self, extent: [u32; 2]) -> Result<Self> {
        if extent.iter().any(|&v| v < MIN_SIDE)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
        {
            return Err("invalid regional local contrast extent".into());
        }
        for pass in &mut self.passes {
            pass.shape[..2].copy_from_slice(&extent);
        }
        Ok(self)
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.passes)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let extent = prepared.extent();
    let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 4;
    if input.size() != bytes * 3 {
        return Err("box local-contrast input extent mismatch".into());
    }
    let mut horizontal = gpu.storage("Rounded horizontal luma mean", bytes)?;
    let mut result = None;
    for params in &prepared.passes {
        let source = result.as_ref().unwrap_or(input);
        let uniform = gpu.uniform(params);
        let mut output = gpu.storage("Box local-contrast RGB", bytes * 3)?;
        let mut trace = gpu.trace(profile.mode.gpu(), 2);
        gpu.dispatch(
            Kernel::BoxContrastHorizontal,
            &mut [
                (0, Read(source)),
                (1, Read(&uniform)),
                (2, Write(horizontal.write())),
            ],
            [extent[0].div_ceil(8), extent[1].div_ceil(8)],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::BoxContrastApply,
            &mut [
                (0, Read(source)),
                (1, Read(&horizontal)),
                (2, Read(&uniform)),
                (3, Write(output.write())),
            ],
            [extent[0].div_ceil(8), extent[1].div_ceil(8)],
            &mut trace,
        );
        gpu.finish_trace(&trace, profile)?;
        result = Some(output);
    }
    Ok(result.unwrap())
}

#[cfg(test)]
#[path = "box_contrast_test.rs"]
mod tests;
