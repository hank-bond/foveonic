//! Applies prepared chroma-sector matrices without selecting a camera or color mode.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Sector {
    pub matrix: [f32; 9],
    pub quadrant: u32,
    pub bin: u32,
}

pub(crate) struct Prepared {
    extent: [u32; 2],
    tables: Vec<f32>,
}

impl Prepared {
    pub(crate) fn new(
        extent: [u32; 2],
        chroma: [f32; 9],
        boundaries: [f32; 6],
        sectors: [Sector; 20],
    ) -> Result<Self> {
        if extent.contains(&0)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
            || chroma.iter().chain(&boundaries).any(|v| !v.is_finite())
            || boundaries.windows(2).any(|v| v[0] < v[1])
            || sectors.iter().any(|s| {
                !(1..=4).contains(&s.quadrant)
                    || !(1..=5).contains(&s.bin)
                    || s.matrix.iter().any(|v| !v.is_finite())
            })
        {
            return Err("invalid chroma-sector geometry or matrices".into());
        }
        let mut tables = chroma.to_vec();
        tables.extend(boundaries);
        for sector in sectors {
            tables.extend(sector.matrix);
            tables.push(f32::from_bits(sector.quadrant));
            tables.push(f32::from_bits(sector.bin));
        }
        Ok(Self { extent, tables })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        self.extent
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.extent).slice(&self.tables)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    source: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let extent = prepared.extent;
    if source.size() != u64::from(extent[0]) * u64::from(extent[1]) * 12 {
        return Err("chroma-sector input geometry differs".into());
    }
    let params = gpu.uniform(&[extent[0], extent[1], 0_u32, 0]);
    let tables = gpu.upload("Prepared chroma-sector matrices", &prepared.tables)?;
    let mut output = gpu.storage("Chroma-sector corrected RGB", source.size())?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::SectorColor,
        &mut [
            (0, Read(source)),
            (1, Read(&params)),
            (2, Read(&tables)),
            (3, Write(output.write())),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "sector_color_test.rs"]
mod tests;
