//! Multiplies red and blue by prepared spatial gains while preserving green.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

pub(crate) struct Grid {
    pub extent: [u32; 2],
    pub values: Vec<f32>,
}
pub(crate) struct Prepared {
    extent: [u32; 2],
    shape: [u32; 4],
    tables: Vec<f32>,
    axes: Vec<[u32; 4]>,
    inverse_spacing: [f32; 2],
}
fn pair(v: f64) -> [f32; 2] {
    let h = v as f32;
    [h, (v - f64::from(h)) as f32]
}
impl Prepared {
    pub(crate) fn new(extent: [u32; 2], red: Grid, blue: Grid) -> Result<Self> {
        if extent.iter().any(|v| *v < 2)
            || red.extent != blue.extent
            || red.extent.iter().any(|v| *v < 2)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
            || red.values.len() != red.extent[0] as usize * red.extent[1] as usize
            || blue.values.len() != red.values.len()
            || red
                .values
                .iter()
                .chain(&blue.values)
                .any(|v| !v.is_finite())
        {
            return Err("invalid spatial color-gain grids".into());
        }
        let spacing = f64::from(extent[0]) / f64::from(red.extent[0] - 1);
        let mut boundary = spacing;
        let mut knot = 0_u32;
        let mut start = 0_u32;
        let mut axes = Vec::with_capacity((extent[0] + extent[1]) as usize);
        for x in 0..extent[0] {
            if f64::from(x) >= boundary {
                knot += 1;
                start = x;
                boundary += spacing;
            }
            if knot + 1 >= red.extent[0] {
                return Err("spatial color-gain knot outside grid".into());
            }
            axes.push([knot, x - start, 0, 0]);
        }
        for y in 0..extent[1] {
            let q = (y as f32 / (extent[1] - 1) as f32) * (red.extent[1] - 1) as f32;
            let low = q.floor() as u32;
            let high = q.ceil() as u32;
            axes.push([low, high, (q - low as f32).to_bits(), 0]);
        }
        let mut tables = red.values;
        tables.extend(blue.values);
        Ok(Self {
            extent,
            shape: [extent[0], extent[1], red.extent[0], red.extent[1]],
            tables,
            axes,
            inverse_spacing: pair(1. / spacing),
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        self.extent
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.shape)
            .slice(&self.tables)
            .slice(&self.axes)
            .with(&self.inverse_spacing)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    if input.size() != u64::from(prepared.extent[0]) * u64::from(prepared.extent[1]) * 12 {
        return Err("spatial color-gain input geometry differs".into());
    }
    let uniform = gpu.uniform(&[
        prepared.shape[0],
        prepared.shape[1],
        prepared.shape[2],
        prepared.shape[3],
        prepared.inverse_spacing[0].to_bits(),
        prepared.inverse_spacing[1].to_bits(),
        0,
        0,
    ]);
    let tables = gpu.upload("Spatial color gain knots", &prepared.tables)?;
    let axes = gpu.upload("Spatial color gain axes", &prepared.axes)?;
    let mut output = gpu.storage("Spatially corrected camera RGB", input.size())?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::SpatialColorGain,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&tables)),
            (3, Read(&axes)),
            (4, Write(output.write())),
        ],
        [
            prepared.extent[0].div_ceil(8),
            prepared.extent[1].div_ceil(8),
        ],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "spatial_color_gain_test.rs"]
mod tests;
