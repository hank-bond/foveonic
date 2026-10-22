//! Corrects spatial green fringes through three immutable, brightness-preserving RGB passes.

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
    active: [u32; 4],
    gains: [f32; 4],
}

pub(crate) struct Prepared {
    params: Params,
    tables: Vec<f32>,
    matrix: [[f32; 2]; 9],
}

fn pair(value: f64) -> [f32; 2] {
    let high = value as f32;
    [high, (value - f64::from(high)) as f32]
}

// Spatial correction grows away from the optical center. The hue pass only
// activates near the outside of that weight range.
const SPATIAL_DISTANCE_SQUARED_SCALE: f64 = 1_000_000.;
const MAX_SPATIAL_WEIGHT: f32 = 0.2;
const HUE_WEIGHT_START: f32 = 0.120_000_005;
const HUE_WEIGHT_WIDTH: f32 = 0.08;

// The reference fills two mirrored half-axes. An odd final slot is unwritten.
fn spatial(extent: u32) -> Vec<f32> {
    let half = i64::from(extent / 2);
    (1 - half..=0)
        .chain(0..half)
        .map(|distance| {
            let exponent = (-(distance * distance) as f64 / SPATIAL_DISTANCE_SQUARED_SCALE) as f32;
            (1. - exponent.exp()) * MAX_SPATIAL_WEIGHT
        })
        .collect()
}

impl Prepared {
    /// Bounds use an exclusive right edge and an inclusive bottom edge.
    /// Gains and matrix project camera RGB only when evaluating the correction.
    pub(crate) fn new(
        extent: [u32; 2],
        active: [u32; 4],
        gains: [f32; 3],
        matrix: [f64; 9],
    ) -> Result<Self> {
        let [width, height] = extent;
        let [left, top, right, bottom] = active;
        if width < 2
            || height < 2
            || u64::from(width) * u64::from(height) > u64::from(u32::MAX) / 3
            || left >= right
            || top > bottom
            || right > width / 2 * 2
            || bottom >= height / 2 * 2
            || gains.iter().any(|v| !v.is_finite() || *v <= 0.)
            || matrix
                .iter()
                .any(|v| !v.is_finite() || !(*v as f32).is_finite())
        {
            return Err("invalid spatial fringe geometry, gains or matrix".into());
        }
        // Neighbor selection uses absolute coordinates against the active extent.
        // Verify the selected neighbor is owned, including rectangles at the edges.
        let middle_x = (right - left) / 2;
        let middle_y = (bottom - top) / 2;
        if (right == width && right - 1 <= middle_x) || (bottom == height - 1 && bottom <= middle_y)
        {
            return Err("spatial fringe neighbor outside image".into());
        }
        let mut horizontal = spatial(width);
        horizontal.resize(width as usize, 0.);
        let hue: Vec<f32> = horizontal
            .iter()
            .map(|v| (v - HUE_WEIGHT_START).max(0.) / HUE_WEIGHT_WIDTH)
            .collect();
        let mut vertical = spatial(height);
        vertical.resize(height as usize, 0.);
        let mut tables = horizontal;
        tables.extend(hue);
        tables.extend(vertical);
        Ok(Self {
            params: Params {
                shape: [width, height, 0, 0],
                active,
                gains: [gains[0], gains[1], gains[2], 0.],
            },
            tables,
            matrix: matrix.map(pair),
        })
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
            .slice(&self.tables)
            .with(&self.matrix)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = prepared.extent();
    if input.size() != u64::from(width) * u64::from(height) * 12 {
        return Err("spatial fringe input geometry differs".into());
    }
    let tables = gpu.upload("Spatial fringe axes", &prepared.tables)?;
    let matrix = gpu.upload("Spatial fringe projection", &prepared.matrix)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 3);
    let mut dispatch = |pass: u32, source: &ReadGuard, baseline: &ReadGuard| -> Result<ReadGuard> {
        let mut output = gpu.storage("Spatial fringe pass", input.size())?;
        let mut params = prepared.params;
        params.shape[2] = pass;
        let uniform = gpu.uniform(&params);
        gpu.dispatch(
            Kernel::PurpleFringe,
            &mut [
                (0, Read(source)),
                (1, Read(&uniform)),
                (2, Read(&tables)),
                (3, Read(&matrix)),
                (4, Read(input)),
                (5, Read(baseline)),
                (6, Write(output.write())),
            ],
            [width.div_ceil(8), height.div_ceil(8)],
            &mut trace,
        );
        Ok(output)
    };
    let first = dispatch(0, input, input)?;
    let second = dispatch(1, &first, input)?;
    let output = dispatch(2, &second, &first)?;
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "purple_fringe_test.rs"]
mod tests;
