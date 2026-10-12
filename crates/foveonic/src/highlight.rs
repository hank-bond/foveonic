//! Prepares immutable camera saturation provenance and performs reference highlight restoration and neighborhood conditioning on GPU.
//!
//! Adapted with owner permission from Foveonic source.rs, gains.rs, highlight.rs,
//! and preprocess.rs at e47af35, including the owner's existing highlight changes.
use crate::{
    gpu::{Gpu, Kernel},
    profile::Profile,
    source::{MatrixEncoding, Metadata, Result},
};
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[cfg_attr(test, derive(Default))]
pub(crate) struct Params {
    pub source: [u32; 4],
    pub area: [u32; 4],
    pub value: [f32; 4],
}
#[cfg_attr(test, derive(Default))]
pub(crate) struct Prepared {
    pub params: Params,
    pub flags: Vec<u32>,
}
pub(crate) struct Output {
    pub pixels: Option<wgpu::Buffer>,
    pub density: Option<wgpu::Buffer>,
}

fn unsigned(v: f64) -> Result<u32> {
    if !v.is_finite() || v.fract() != 0. || !(0. ..=u32::MAX as f64).contains(&v) {
        Err("invalid original saturation/geometry value".into())
    } else {
        Ok(v as u32)
    }
}
fn saturation(metadata: &Metadata) -> Result<Vec<u32>> {
    let count = metadata
        .width
        .checked_mul(metadata.height)
        .ok_or("saturation geometry overflow")?;
    let mut flags = vec![0_u32; count];
    for (c, suffix) in ["R", "G", "B"].into_iter().enumerate() {
        let runs = format!("SatMap{suffix}");
        let full = format!("SatMapFull{suffix}");
        let bit = 4 >> c;
        if let Some(m) = metadata.matrix(&runs) {
            let values = m
                .values
                .iter()
                .copied()
                .map(unsigned)
                .collect::<Result<Vec<_>>>()?;
            if values.len() % 2 != 1 {
                return Err(format!("invalid {runs} run count").into());
            }
            let mut position = 0_usize;
            for pair in values[..values.len() - 1].as_chunks::<2>().0 {
                let begin = position
                    .checked_add(pair[0] as usize)
                    .ok_or("saturation gap overflow")?;
                let end = begin
                    .checked_add(pair[1] as usize)
                    .ok_or("saturation run overflow")?;
                if end > count {
                    return Err(format!("{runs} exceeds sensor").into());
                }
                for flag in &mut flags[begin..end] {
                    *flag |= bit;
                }
                position = end;
            }
            if position.checked_add(*values.last().unwrap() as usize) != Some(count) {
                return Err(format!("{runs} does not describe full sensor").into());
            }
        } else if let Some(m) = metadata.matrix(&full) {
            for &v in &m.values {
                let p = unsigned(v)? as usize;
                if p >= count {
                    return Err(format!("{full} exceeds sensor").into());
                }
                flags[p] |= bit;
            }
        } else {
            return Err(format!("missing original {suffix} saturation map").into());
        }
    }
    Ok(flags)
}
pub(crate) fn prepare(metadata: &Metadata, gain: f32) -> Result<Prepared> {
    let count = metadata
        .width
        .checked_mul(metadata.height)
        .ok_or("highlight geometry overflow")?;
    if metadata.width < 5
        || metadata.height < 5
        || count > u32::MAX as usize / 3
        || !gain.is_finite()
        || gain < 0.
    {
        return Err("unsupported highlight geometry or Sensor gain".into());
    }
    let active = metadata.fixed::<4>("ActiveImageArea")?;
    let columns = metadata.fixed::<4>("DarkShieldColRange")?;
    let top = unsigned(active[1])?;
    let bottom = unsigned(active[3])?;
    let left = unsigned(columns[1])?;
    let right = unsigned(columns[2])?;
    let area = [
        left.checked_add(6).ok_or("highlight left overflow")?,
        top.checked_sub(32).ok_or("highlight top underflow")?,
        right.checked_sub(8).ok_or("highlight right underflow")?,
        bottom.checked_add(33).ok_or("highlight bottom overflow")?,
    ];
    if area[0] < 2
        || area[1] < 2
        || area[0] > area[2]
        || area[1] > area[3]
        || area[2] as usize + 2 >= metadata.width
        || area[3] as usize + 2 >= metadata.height
    {
        return Err("invalid highlight work rectangle".into());
    }
    let camera = |name: &str| -> Result<f64> {
        match metadata
            .matrix(name)
            .filter(|m| m.encoding == MatrixEncoding::Float32 && m.dimensions == [1])
        {
            None => Ok(0.),
            Some(m) if m.values.len() == 1 && (m.values[0] as f32).is_finite() => {
                Ok(m.values[0] as f32 as f64)
            }
            Some(_) => Err("invalid highlight exposure key".into()),
        }
    };
    let ramp = (1. - (camera("ExpEstimate")? + camera("ExpNet")? - 1.5).max(0.)).max(0.);
    Ok(Prepared {
        params: Params {
            source: [
                metadata.width as u32,
                metadata.height as u32,
                count as u32,
                (count as u32).div_ceil(256),
            ],
            area,
            value: [ramp.mul_add(3000., 4000.) as f32, gain, 0., 0.],
        },
        flags: saturation(metadata)?,
    })
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: Option<&wgpu::Buffer>,
    p: &Prepared,
    want_density: bool,
    profile: &mut Profile,
) -> Result<Output> {
    let size = u64::from(p.params.source[2]) * 4;
    let uniform = gpu.uniform(&p.params);
    let original = gpu.upload("Original camera saturation flags", &p.flags)?;
    let flags = gpu.storage("Connected saturation processing copy", size)?;
    let map = gpu.storage("Multi-layer clipped neighborhood map", size)?;
    let groups = [
        p.params.source[0].div_ceil(8),
        p.params.source[1].div_ceil(8),
    ];
    let mut trace = gpu.trace(profile.enabled, 12);
    gpu.dispatch(
        Kernel::HighlightFlags,
        &[(0, &original), (1, &flags), (2, &uniform)],
        groups,
        &mut trace,
    );
    gpu.dispatch(
        Kernel::HighlightMap,
        &[(1, &flags), (2, &uniform), (3, &map)],
        groups,
        &mut trace,
    );
    let density = if want_density {
        let output = gpu.storage("Highlight neighborhood density", size)?;
        gpu.dispatch(
            Kernel::HighlightDensity,
            &[(2, &uniform), (3, &map), (4, &output)],
            groups,
            &mut trace,
        );
        Some(output)
    } else {
        None
    };
    let pixels = if let Some(input) = input {
        let marks = gpu.storage("Highlight donor ranks", size)?;
        let counts = gpu.storage(
            "Highlight donor prefix counts",
            u64::from(p.params.source[3] + 1) * 4,
        )?;
        let donors = gpu.storage("Ordered highlight donor indices", size)?;
        let sorted = gpu.storage("Stable per-bin highlight donor indices", size)?;
        let histogram = gpu.storage(
            "Highlight block-bin counts/prefixes",
            u64::from(p.params.source[3]) * 256 * 4,
        )?;
        let starts = gpu.storage("Highlight bin start offsets", 257 * 4)?;
        let lookup = gpu.storage("Highlight blue correction table and counts", 512 * 4)?;
        gpu.dispatch(
            Kernel::HighlightDonors,
            &[
                (1, &flags),
                (2, &uniform),
                (5, input),
                (6, &marks),
                (7, &counts),
            ],
            [p.params.source[3], 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::HighlightPrefix,
            &[(2, &uniform), (7, &counts)],
            [1, 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::HighlightScatter,
            &[(2, &uniform), (6, &marks), (7, &counts), (8, &donors)],
            [p.params.source[3], 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::HighlightBinHistogram,
            &[
                (2, &uniform),
                (5, input),
                (6, &marks),
                (7, &counts),
                (8, &donors),
                (11, &histogram),
            ],
            [p.params.source[3], 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::HighlightBinPrefix,
            &[(2, &uniform), (7, &counts), (11, &histogram), (12, &starts)],
            [1, 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::HighlightBinScatter,
            &[
                (2, &uniform),
                (6, &marks),
                (7, &counts),
                (8, &donors),
                (10, &sorted),
                (11, &histogram),
                (12, &starts),
            ],
            [p.params.source[3], 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::HighlightTable,
            &[
                (2, &uniform),
                (5, input),
                (9, &lookup),
                (10, &sorted),
                (12, &starts),
            ],
            [8, 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::HighlightInterpolation,
            &[(9, &lookup)],
            [1, 1],
            &mut trace,
        );
        let output = gpu.storage("Highlight restored layers", size * 3)?;
        // No density readback or photographic host arithmetic; apply uses the
        // immutable neighborhood map and table, not preceding output writes.
        gpu.dispatch(
            Kernel::HighlightApply,
            &[
                (1, &flags),
                (2, &uniform),
                (3, &map),
                (4, &output),
                (5, input),
                (9, &lookup),
            ],
            groups,
            &mut trace,
        );
        Some(output)
    } else {
        None
    };
    gpu.finish_trace(&trace, profile)?;
    Ok(Output { pixels, density })
}
#[cfg(test)]
#[path = "highlight_test.rs"]
mod tests;
