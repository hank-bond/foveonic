//! Projects original camera clipping evidence and recovers highlights on the working grid.

//!
//! Adapted with owner permission from Foveonic source.rs, gains.rs, highlight.rs,
//! and preprocess.rs at e47af35, including the owner's existing highlight changes.
use crate::gpu::Binding::{Read, Write};
use crate::{
    gpu::{Gpu, Kernel},
    profile::Profile,
    source::{MatrixEncoding, Metadata, Result},
};
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[cfg_attr(test, derive(Default))]
pub(crate) struct Params {
    pub extent: [u32; 2],
    /// Set from the working extent and donor limit when processing.
    pub donor_step: u32,
    pub donor_columns: u32,
    pub area: [u32; 4],
    pub value: [f32; 4],
}
#[derive(Clone)]
#[cfg_attr(test, derive(Default))]
pub(crate) struct Prepared {
    pub params: Params,
    /// Original camera evidence is materialized on this grid only when needed.
    pub flags: Option<crate::recipe::Shared<u32>>,
    pub grid: Option<crate::render::WorkingGrid>,
    /// At most this many donor sites, sampled on a power-of-two grid; None uses every site.
    pub donor_limit: Option<u32>,
}

/// The smallest power-of-two sampling step that keeps the donor sites within `limit`.
fn donor_step([width, height]: [u32; 2], limit: Option<u32>) -> u32 {
    let mut step = 1;
    if let Some(limit) = limit {
        while width.div_ceil(step) * height.div_ceil(step) > limit {
            step *= 2;
        }
    }
    step
}

impl Prepared {
    pub(crate) fn on_grid(mut self, grid: Option<crate::render::WorkingGrid>) -> Self {
        self.grid = grid;
        if grid.is_some() {
            // Materialized flags belong to the previous grid. Rebuild lazily
            // from immutable camera metadata at execution.
            self.flags = None;
        }
        self
    }
    pub(crate) fn with_donor_limit(mut self, limit: Option<u32>) -> Self {
        self.donor_limit = limit;
        self
    }
    /// The limit itself is not keyed: grids small enough to use every donor share a key.
    pub(crate) fn sampling_key(&self) -> crate::recipe::Key {
        let mut key = crate::recipe::Key::pod(&self.params)
            .with(&donor_step(self.extent(), self.donor_limit));
        if let Some(grid) = self.grid {
            key = key
                .with(&grid.scale.divisor())
                .with(&grid.extent)
                .with(&grid.origin);
        }
        key
    }
    fn extent(&self) -> [u32; 2] {
        self.grid.map_or(self.params.extent, |grid| grid.extent)
    }
    fn processing_params(&self) -> Params {
        let mut params = self.params;
        let [width, height] = self.extent();
        params.extent = [width, height];
        if let Some(grid) = self.grid {
            let [l, t, r, b] = self.params.area;
            let bounds = grid.bounds([l, t, r + 1, b + 1]);
            let radius = (2. * grid.scale.factor()).round().max(1.) as u32;
            params.area = [
                bounds[0].max(radius),
                bounds[1].max(radius),
                bounds[2]
                    .saturating_sub(1)
                    .min(width.saturating_sub(radius + 1)),
                bounds[3]
                    .saturating_sub(1)
                    .min(height.saturating_sub(radius + 1)),
            ];
            params.value[1] = 1. / ((2 * radius + 1) * (2 * radius + 1)) as f32;
            params.value[2] = radius as f32;
            params.value[3] = 1.;
        }
        params.donor_step = donor_step([width, height], self.donor_limit);
        params.donor_columns = width.div_ceil(params.donor_step);
        params
    }
    pub(crate) fn key(&self) -> crate::recipe::Key {
        let key = self.sampling_key().with(&u32::from(self.flags.is_some()));
        if let Some(flags) = &self.flags {
            key.join(flags.key())
        } else {
            key
        }
    }
}
pub(crate) struct Output {
    pub pixels: Option<crate::gpu::ReadGuard>,
    pub density: Option<crate::gpu::ReadGuard>,
}

fn unsigned(v: f64) -> Result<u32> {
    if !v.is_finite() || v.fract() != 0. || !(0. ..=u32::MAX as f64).contains(&v) {
        Err("invalid original saturation/geometry value".into())
    } else {
        Ok(v as u32)
    }
}
#[cfg(test)]
fn saturation(metadata: &Metadata) -> Result<Vec<u32>> {
    clipping(metadata, None)
}

/// Clipping is immutable camera metadata. Decode runs directly onto their
/// requested cells, without allocating or uploading a native-size flag image.
pub(crate) fn clipping(
    metadata: &Metadata,
    grid: Option<crate::render::WorkingGrid>,
) -> Result<Vec<u32>> {
    let count = metadata
        .width
        .checked_mul(metadata.height)
        .ok_or("saturation geometry overflow")?;
    let extent = grid.map_or([metadata.width as u32, metadata.height as u32], |g| {
        g.extent
    });
    let mut flags = vec![0_u32; extent[0] as usize * extent[1] as usize];
    let mut mark = |mut begin: usize, end: usize, bit: u32| {
        if let Some(grid) = grid {
            let d = grid.scale.divisor() as usize;
            let width = metadata.width;
            while begin < end {
                let row = begin / width;
                let stop = end.min((row + 1) * width);
                let y = row / d + grid.origin[1] as usize;
                let first = begin % width / d + grid.origin[0] as usize;
                let last = (stop - 1) % width / d + grid.origin[0] as usize + 1;
                let offset = y * extent[0] as usize;
                for flag in &mut flags[offset + first..offset + last] {
                    *flag |= bit;
                }
                begin = stop;
            }
        } else {
            for flag in &mut flags[begin..end] {
                *flag |= bit;
            }
        }
    };
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
                mark(begin, end, bit);
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
                mark(p, p + 1, bit);
            }
        } else {
            return Err(format!("missing original {suffix} saturation map").into());
        }
    }
    if let Some(grid) = grid {
        // Match numerical pooling's edge replication, including its halo.
        for y in 0..extent[1] {
            for x in 0..extent[0] {
                let sx = x.saturating_sub(grid.origin[0]).min(grid.logical[0] - 1) + grid.origin[0];
                let sy = y.saturating_sub(grid.origin[1]).min(grid.logical[1] - 1) + grid.origin[1];
                flags[(y * extent[0] + x) as usize] = flags[(sy * extent[0] + sx) as usize];
            }
        }
    }
    Ok(flags)
}

pub(crate) fn clipping_key(grid: Option<crate::render::WorkingGrid>) -> crate::recipe::Key {
    match grid {
        Some(g) => crate::recipe::Key::pod(&g.native)
            .with(&g.scale.divisor())
            .with(&g.extent)
            .with(&g.origin),
        None => crate::recipe::Key::default(),
    }
}
pub(crate) fn validate_threshold(ev: Option<f32>) -> Result<()> {
    if ev.is_some_and(|v| !(-2. ..=2.).contains(&v)) {
        return Err("highlight threshold must be in -2..2 EV".into());
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn prepare(metadata: &Metadata, threshold_ev: Option<f32>) -> Result<Prepared> {
    prepare_with_flags(metadata, threshold_ev, None)
}

#[cfg(test)]
pub(crate) fn prepare_with_flags(
    metadata: &Metadata,
    threshold_ev: Option<f32>,
    flags: Option<crate::recipe::Shared<u32>>,
) -> Result<Prepared> {
    prepare_description(metadata, threshold_ev, flags, false)
}

/// Authors the exact numerical recipe without expanding native clipping evidence.
pub(crate) fn deferred(metadata: &Metadata, threshold_ev: Option<f32>) -> Result<Prepared> {
    prepare_description(metadata, threshold_ev, None, true)
}

fn prepare_description(
    metadata: &Metadata,
    threshold_ev: Option<f32>,
    flags: Option<crate::recipe::Shared<u32>>,
    deferred: bool,
) -> Result<Prepared> {
    validate_threshold(threshold_ev)?;
    let count = metadata
        .width
        .checked_mul(metadata.height)
        .ok_or("highlight geometry overflow")?;
    if metadata.width < 5 || metadata.height < 5 || count > u32::MAX as usize / 3 {
        return Err("unsupported highlight geometry".into());
    }
    if flags.as_ref().is_some_and(|flags| flags.len() != count) {
        return Err("original saturation map has the wrong sensor extent".into());
    }
    let active = super::calibration::rectangle(metadata, "ActiveImageArea")?;
    let columns = metadata.fixed::<4>("DarkShieldColRange")?;
    let top = active[1];
    let bottom = active[3];
    let left = unsigned(columns[1])?;
    let right = unsigned(columns[2])?;
    let mut area = [
        left.checked_add(6).ok_or("highlight left overflow")?,
        top.saturating_sub(32).max(2),
        right.checked_sub(8).ok_or("highlight right underflow")?,
        bottom.checked_add(32).ok_or("highlight bottom overflow")?,
    ];
    // Reduced captures leave less padding. Keep the work region inside the
    // sensor while reserving the kernel’s two-pixel halo.
    area[3] = area[3].min(metadata.height as u32 - 3);
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
    // Adjust only neutralization onset. Scaling the stage's input would also
    // change donor selection and the transition width.
    let threshold = ramp.mul_add(3000., 4000.) as f32 * threshold_ev.unwrap_or(0.).exp2();
    Ok(Prepared {
        params: Params {
            extent: [metadata.width as u32, metadata.height as u32],
            donor_step: 0,
            donor_columns: 0,
            area,
            value: [threshold, 0., 0., 0.],
        },
        grid: None,
        donor_limit: None,
        flags: if deferred {
            None
        } else {
            Some(match flags {
                Some(flags) => flags,
                None => clipping(metadata, None)?.into(),
            })
        },
    })
}

pub(crate) fn materialize(
    metadata: &Metadata,
    description: &Prepared,
    flags: Option<crate::recipe::Shared<u32>>,
) -> Result<Prepared> {
    let flags = match flags {
        Some(flags) => flags,
        None => clipping(metadata, description.grid)?.into(),
    };
    let [width, height] = description.extent();
    if flags.len() != width as usize * height as usize {
        return Err("original saturation map has the wrong sensor extent".into());
    }
    Ok(Prepared {
        params: description.params,
        flags: Some(flags),
        grid: description.grid,
        donor_limit: description.donor_limit,
    })
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: Option<&crate::gpu::ReadGuard>,
    p: &Prepared,
    want_density: bool,
    profile: &mut Profile,
) -> Result<Output> {
    let params = p.processing_params();
    let [width, height] = params.extent;
    let size = u64::from(width) * u64::from(height) * 4;
    let uniform = gpu.uniform(&params);
    let original = gpu.upload(
        "Original camera clipping flags",
        p.flags
            .as_ref()
            .ok_or("highlight clipping evidence must be materialized")?,
    )?;
    let mut flags = gpu.storage("Connected saturation processing copy", size)?;
    let groups = [width.div_ceil(8), height.div_ceil(8)];
    let mut trace = gpu.trace(profile.mode.gpu(), 12);
    gpu.dispatch(
        Kernel::HighlightFlags,
        &mut [
            (0, Read(&original)),
            (1, Write(flags.write())),
            (2, Read(&uniform)),
        ],
        groups,
        &mut trace,
    );
    let mut map = gpu.storage("Multi-layer clipped neighborhood map", size)?;
    gpu.dispatch(
        Kernel::HighlightMap,
        &mut [
            (1, Read(&flags)),
            (2, Read(&uniform)),
            (3, Write(map.write())),
        ],
        groups,
        &mut trace,
    );
    let density = if want_density {
        let mut output = gpu.storage("Highlight neighborhood density", size)?;
        gpu.dispatch(
            Kernel::HighlightDensity,
            &mut [
                (2, Read(&uniform)),
                (3, Read(&map)),
                (4, Write(output.write())),
            ],
            groups,
            &mut trace,
        );
        Some(output)
    } else {
        None
    };
    let pixels = if let Some(input) = input {
        let candidates = params.donor_columns * height.div_ceil(params.donor_step);
        let blocks = candidates.div_ceil(256);
        let donor_bytes = u64::from(candidates) * 4;
        let mut marks = gpu.storage("Highlight donor ranks", donor_bytes)?;
        let mut counts = gpu.storage("Highlight donor prefix counts", u64::from(blocks + 1) * 4)?;
        let mut donors = gpu.storage("Ordered highlight donor indices", donor_bytes)?;
        let mut sorted = gpu.storage("Stable per-bin highlight donor indices", donor_bytes)?;
        let mut histogram = gpu.storage(
            "Highlight block-bin counts/prefixes",
            u64::from(blocks) * 256 * 4,
        )?;
        let mut starts = gpu.storage("Highlight bin start offsets", 257 * 4)?;
        let mut lookup = gpu.storage("Highlight blue correction table and counts", 512 * 4)?;
        gpu.dispatch(
            Kernel::HighlightDonors,
            &mut [
                (1, Read(&flags)),
                (2, Read(&uniform)),
                (5, Read(input)),
                (6, Write(marks.write())),
                (7, Write(counts.write())),
            ],
            [blocks, 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::HighlightPrefix,
            &mut [(7, Write(counts.write()))],
            [1, 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::HighlightScatter,
            &mut [
                (2, Read(&uniform)),
                (6, Read(&marks)),
                (7, Read(&counts)),
                (8, Write(donors.write())),
            ],
            [blocks, 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::HighlightBinHistogram,
            &mut [
                (5, Read(input)),
                (6, Write(marks.write())),
                (7, Read(&counts)),
                (8, Read(&donors)),
                (11, Write(histogram.write())),
            ],
            [blocks, 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::HighlightBinPrefix,
            &mut [
                (7, Read(&counts)),
                (11, Write(histogram.write())),
                (12, Write(starts.write())),
            ],
            [1, 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::HighlightBinScatter,
            &mut [
                (6, Read(&marks)),
                (7, Read(&counts)),
                (8, Read(&donors)),
                (10, Write(sorted.write())),
                (11, Read(&histogram)),
                (12, Read(&starts)),
            ],
            [blocks, 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::HighlightTable,
            &mut [
                (5, Read(input)),
                (9, Write(lookup.write())),
                (10, Read(&sorted)),
                (12, Read(&starts)),
            ],
            [256, 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::HighlightInterpolation,
            &mut [(9, Write(lookup.write()))],
            [1, 1],
            &mut trace,
        );
        let mut output = gpu.storage("Highlight restored layers", size * 3)?;
        // Keep the donor map unchanged during correction. Reading earlier output
        // writes would make later pixels depend on the order they were processed.
        gpu.dispatch(
            Kernel::HighlightApply,
            &mut [
                (1, Read(&flags)),
                (2, Read(&uniform)),
                (3, Read(&map)),
                (4, Write(output.write())),
                (5, Read(input)),
                (9, Read(&lookup)),
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
