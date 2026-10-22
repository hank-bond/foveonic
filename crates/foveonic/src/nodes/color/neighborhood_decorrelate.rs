//! Decorrelates RGB noise through ordered neighborhood updates with supplied variance and weight tables.
//!
//! The reference implementation updates every neighbor after visiting a center.
//! Each GPU destination replays its contributing centers in that same order.

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
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    controls: [f32; 4],
}

pub(crate) struct Prepared {
    params: Params,
    model: [[f32; 2]; 9],
    intensity: Vec<f32>,
    range: Vec<f32>,
}

impl Prepared {
    pub(crate) fn new(
        extent: [u32; 2],
        model: [f64; 9],
        intensity: Vec<f32>,
        range: Vec<f32>,
        strength: f32,
    ) -> Result<Self> {
        if extent.contains(&0)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 7
            || model
                .iter()
                .any(|v| !v.is_finite() || !(*v as f32).is_finite())
            || intensity.len() != 14385
            || intensity.iter().any(|v| !v.is_finite() || *v <= 0.)
            || range.len() != 2048
            || range
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            || !strength.is_finite()
            || strength <= 0.
        {
            return Err("invalid neighborhood noise parameters".into());
        }
        Ok(Self {
            params: Params {
                shape: [extent[0], extent[1], 0, 0],
                controls: [strength, 0., 0., 0.],
            },
            model: model.map(|v| {
                let high = v as f32;
                [high, (v - f64::from(high)) as f32]
            }),
            intensity,
            range,
        })
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }

    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
            .slice(&self.model)
            .slice(&self.intensity)
            .slice(&self.range)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = prepared.extent();
    let pixels = u64::from(width) * u64::from(height);
    if input.size() != pixels * 12 {
        return Err("neighborhood noise input does not match its extent".into());
    }
    let uniform = gpu.uniform(&prepared.params);
    let model = gpu.upload("Neighborhood variance coefficients", &prepared.model)?;
    let intensity = gpu.upload("Neighborhood intensity weights", &prepared.intensity)?;
    let range = gpu.upload("Neighborhood range weights", &prepared.range)?;
    let mut stats = gpu.storage("Neighborhood noise statistics", pixels * 28)?;
    let mut output = gpu.storage("Decorrelated RGB", pixels * 12)?;
    let groups = [width.div_ceil(8), height.div_ceil(8)];
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    gpu.dispatch(
        Kernel::NeighborhoodNoiseStats,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&model)),
            (3, Read(&intensity)),
            (4, Write(stats.write())),
        ],
        groups,
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NeighborhoodDecorrelate,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&stats)),
            (3, Read(&range)),
            (4, Write(output.write())),
        ],
        groups,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "neighborhood_decorrelate_test.rs"]
mod tests;
