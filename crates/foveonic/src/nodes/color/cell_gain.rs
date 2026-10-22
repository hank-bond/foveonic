//! Applies a spatial gain to each two-by-two cell with Float32 horizontal recurrence.

use super::spatial_color_gain::Grid;
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};
pub(crate) struct Prepared {
    extent: [u32; 2],
    shape: [u32; 4],
    spacing: f32,
    grid: Vec<f32>,
    rows: Vec<[u32; 4]>,
    segments: Vec<[u32; 4]>,
}
impl Prepared {
    pub(crate) fn new(extent: [u32; 2], grid: Grid) -> Result<Self> {
        if extent.iter().any(|v| *v < 4 || v % 2 != 0)
            || grid.extent.iter().any(|v| *v < 2)
            || grid.values.len() != grid.extent[0] as usize * grid.extent[1] as usize
            || grid.values.iter().any(|v| !v.is_finite())
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX)
        {
            return Err("invalid two-by-two cell gain geometry".into());
        }
        let cell = extent.map(|v| v / 2);
        let spacing = cell[0] as f32 / (grid.extent[0] - 1) as f32;
        let mut segments = vec![[0, 0, 0, 0]; (grid.extent[0] - 1) as usize];
        let mut knot = 0_usize;
        let mut boundary = spacing;
        for x in 0..cell[0] {
            if x as f32 >= boundary {
                knot += 1;
                boundary += spacing;
            }
            let s = segments
                .get_mut(knot)
                .ok_or("cell gain knot outside grid")?;
            if s[1] == 0 {
                s[0] = x;
            }
            s[1] = x + 1;
            s[2] = knot as u32;
        }
        let rows = (0..cell[1])
            .map(|y| {
                let q = (y as f32 / (cell[1] - 1) as f32) * (grid.extent[1] - 1) as f32;
                [
                    q.floor() as u32,
                    q.ceil() as u32,
                    (q - q.floor()).to_bits(),
                    0,
                ]
            })
            .collect();
        Ok(Self {
            extent,
            shape: [cell[0], cell[1], grid.extent[0], grid.extent[1]],
            spacing,
            grid: grid.values,
            rows,
            segments,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        self.extent
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.shape)
            .with(&self.spacing)
            .slice(&self.grid)
            .slice(&self.rows)
            .slice(&self.segments)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    if input.size() != u64::from(prepared.extent[0]) * u64::from(prepared.extent[1]) * 4 {
        return Err("cell gain input extent mismatch".into());
    }
    let uniform = gpu.uniform(&[
        prepared.shape[0],
        prepared.shape[1],
        prepared.shape[2],
        prepared.shape[3],
        prepared.spacing.to_bits(),
        0,
        0,
        0,
    ]);
    let grid = gpu.upload("Cell gain knots", &prepared.grid)?;
    let rows = gpu.upload("Cell gain vertical interpolation", &prepared.rows)?;
    let segments = gpu.upload("Cell gain horizontal segments", &prepared.segments)?;
    let mut output = gpu.storage("Gain-corrected two-by-two cells", input.size())?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::CellGain,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&grid)),
            (3, Read(&rows)),
            (4, Read(&segments)),
            (5, Write(output.write())),
        ],
        [
            (prepared.shape[2] - 1).div_ceil(8),
            prepared.shape[1].div_ceil(8),
        ],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "cell_gain_test.rs"]
mod tests;
