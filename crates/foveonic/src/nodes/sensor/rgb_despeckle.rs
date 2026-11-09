//! Removes isolated RGB excursions using immutable masked five-by-five neighborhoods.

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
    extent: [u32; 2],
    strength: f32,
    radius: u32,
    bright_only: u32,
    _padding: [u32; 3],
}
#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    noise: [f32; 9],
    projection: Vec<[f32; 2]>,
}
impl Prepared {
    /// This contract uses ordinary first extrema. The separate long-exposure
    /// second-extrema branch requires a different masked-neighbor contract.
    pub(crate) fn new(
        extent: [u32; 2],
        noise: [f32; 9],
        strength: f32,
        matrix: [f64; 9],
    ) -> Result<Self> {
        if extent.iter().any(|&v| v < 5)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
            || noise.iter().any(|v| !v.is_finite())
            || !strength.is_finite()
            || strength < 0.
            || matrix.iter().any(|v| !v.is_finite())
        {
            return Err("invalid RGB despeckle controls".into());
        }
        Ok(Self {
            params: Params {
                extent,
                strength,
                radius: 2,
                bright_only: 0,
                _padding: [0; 3],
            },
            noise,
            projection: super::chroma_repair::projection(matrix)?,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        self.params.extent
    }
    /// Pooling changes independent noise and the physical neighborhood pitch.
    /// Reduced detection repairs isolated bright excursions without treating
    /// naturally darker pooled details as sensor defects.
    pub(crate) fn on_grid(mut self, grid: crate::render::WorkingGrid, energy: f64) -> Result<Self> {
        if grid.native != self.extent() || !energy.is_finite() || energy <= 0. || energy > 1. {
            return Err("invalid RGB despeckle working grid".into());
        }
        self.params.extent = grid.extent;
        self.params.radius = 2_u32.div_ceil(grid.scale.divisor()).max(1);
        self.params.bright_only = u32::from(grid.scale.is_reduced());
        for coefficient in &mut self.noise {
            *coefficient *= energy as f32;
        }
        Ok(self)
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
            .slice(&self.noise)
            .slice(&self.projection)
    }
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    mask: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h] = prepared.extent();
    let cells = u64::from(w) * u64::from(h);
    if input.size() != cells * 12 || mask.size() != cells * 4 {
        return Err("RGB despeckle input geometry mismatch".into());
    }
    let mut output = gpu.storage("RGB despeckle", input.size())?;
    let params = gpu.uniform(&prepared.params);
    let noise = gpu.upload("RGB despeckle noise", &prepared.noise)?;
    let matrix = gpu.upload("RGB despeckle projection", &prepared.projection)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::RgbDespeckle,
        &mut [
            (0, Read(input)),
            (1, Read(mask)),
            (2, Read(&params)),
            (3, Read(&noise)),
            (4, Read(&matrix)),
            (5, Write(output.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
#[cfg(test)]
#[path = "rgb_despeckle_test.rs"]
mod tests;
