//! Suppresses channel noise by blending RGB ratios toward calibrated quadratic color curves.
//!
//! The finite positive-RGB contract follows the reference implementation's CSR worker.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

const INTENSITY_ENTRIES: usize = 14_385;
const INTENSITY_STRENGTH: f32 = 52.;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    extent: [u32; 2],
    padding: [u32; 2],
    gains: [f32; 4],
}

pub(crate) struct Prepared {
    params: Params,
    coefficients: Vec<[f32; 2]>,
    intensity: Vec<f32>,
}

impl Prepared {
    /// Model order is constant RGB, linear RGB, quadratic RGB. Gains remain
    /// independent of the model and are undone after each ratio replacement.
    pub(crate) fn new(extent: [u32; 2], gains: [f64; 3], model: [f64; 9]) -> Result<Self> {
        if extent.contains(&0)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
            || gains
                .iter()
                .any(|v| !v.is_finite() || *v <= 0. || !(*v as f32).is_finite())
            || model
                .iter()
                .any(|v| !v.is_finite() || !(*v as f32).is_finite())
        {
            return Err("invalid chromatic ratio controls".into());
        }
        let coefficients: Vec<[f32; 2]> = model
            .into_iter()
            .chain(gains)
            .map(|v| {
                let high = v as f32;
                [high, (v - f64::from(high)) as f32]
            })
            .collect();
        let mut intensity: Vec<_> = (0..INTENSITY_ENTRIES)
            .map(|i| {
                let x = (INTENSITY_ENTRIES - i) as f32 / INTENSITY_ENTRIES as f32;
                ((x.powf(4.) + 1.) * 104.) * 0.5
            })
            .collect();
        intensity[INTENSITY_ENTRIES - 1] = INTENSITY_STRENGTH;
        Ok(Self {
            params: Params {
                extent,
                padding: [0; 2],
                gains: [gains[0] as f32, gains[1] as f32, gains[2] as f32, 0.],
            },
            coefficients,
            intensity,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        self.params.extent
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
            .slice(&self.coefficients)
            .slice(&self.intensity)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let extent = prepared.extent();
    let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 12;
    if input.size() != bytes {
        return Err("chromatic ratio input size does not match its extent".into());
    }
    let uniform = gpu.uniform(&prepared.params);
    let coefficients = gpu.upload("Chromatic ratio coefficients", &prepared.coefficients)?;
    let intensity = gpu.upload("Chromatic ratio intensity strengths", &prepared.intensity)?;
    let mut output = gpu.storage("Chromatic ratio noise suppression", bytes)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::ChromaticRatio,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&coefficients)),
            (3, Read(&intensity)),
            (4, Write(output.write())),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "chromatic_ratio_test.rs"]
mod tests;
