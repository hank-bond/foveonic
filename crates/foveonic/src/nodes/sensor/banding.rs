//! Prepares coordinate-only row-correction weights while keeping photo measurements on the GPU.

//!
//! Adapted with owner permission from Foveonic banding/mod.rs and patches.rs at
//! e47af35. Host fitting matrices depend only on fixed geometric polynomial bases.
use crate::gpu::Binding::{Read, Write};
use crate::recipe::Shared;
use crate::source::{MatrixEncoding, Metadata, Result};

// Largest selected polynomial basis. Match the coefficient slots in banding.wgsl.
const MAX_FIT_COEFFICIENTS: u32 = 6;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    pub source: [u32; 4],
    pub area: [u32; 4],
    pub values: [f32; 4],
    pub gains: [f32; 4],
    pub output: [u32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Patch {
    pub geometry: [u32; 4],
    pub storage: [u32; 4],
}
pub(crate) struct Prepared {
    pub params: Params,
    pub patches: Shared<Patch>,
    pub tables: Shared<f32>,
    pub links: Shared<u32>,
    pub rows: Shared<[u32; 2]>,
    pub contributions: u32,
    pub batches: Shared<[u32; 4]>,
}

/// Cheap graph identity. Fitting tables are materialized only for executed work.
#[derive(Clone)]
pub(crate) struct Description {
    params: Params,
    native_height: u32,
    scale: crate::render::Scale,
    origin: [u32; 2],
}
impl Description {
    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::pod(&self.params).join(crate::recipe::Key::pod(&[
            self.native_height,
            self.scale.divisor(),
            self.origin[0],
            self.origin[1],
        ]))
    }
    /// Preserve physical patch-bank phase while restricting corrected output storage.
    pub(crate) fn for_region(mut self, bounds: [u32; 4]) -> Result<Self> {
        if (0..2).any(|a| bounds[a] >= bounds[a + 2] || bounds[a + 2] > self.params.source[a]) {
            return Err("invalid row-correction output bounds".into());
        }
        self.params.output = bounds;
        Ok(self)
    }
    /// Patch evidence spans the active width. Keep a conservative vertical halo
    /// for the largest complete patch contributing to any requested row.
    pub(crate) fn required_input_bounds(&self, bounds: [u32; 4]) -> Result<[u32; 4]> {
        self.clone().for_region(bounds)?;
        let side = (151_u32.div_ceil(self.scale.divisor()) | 1).max(3);
        Ok([
            0,
            bounds[1].saturating_sub(side),
            self.params.source[0],
            bounds[3].saturating_add(side).min(self.params.source[1]),
        ])
    }
    pub(crate) fn compact_bounds(&self) -> Option<[u32; 4]> {
        (self.params.output != [0, 0, self.params.source[0], self.params.source[1]])
            .then_some(self.params.output)
    }
    pub(crate) fn output_bounds(&self) -> [u32; 4] {
        self.params.output
    }
    pub(crate) fn on_grid(mut self, grid: Option<crate::render::WorkingGrid>) -> Self {
        if let Some(grid) = grid {
            self.params.source[..2].copy_from_slice(&grid.extent);
            self.params.area = grid.bounds(self.params.area);
            self.params.output = [0, 0, grid.extent[0], grid.extent[1]];
            self.scale = grid.scale;
            self.origin = grid.origin;
        }
        self
    }
    pub(crate) fn materialize(&self) -> Result<Prepared> {
        materialize(self)
    }
}

fn matrices(side: u32, coefficients: u32) -> Result<(u32, Vec<f32>, Vec<f32>)> {
    if !(3..=151).contains(&side) || side.is_multiple_of(2) || ![3, 6].contains(&coefficients) {
        return Err("unsupported row-banding patch side".into());
    }
    let n = coefficients as usize;
    let count = (side * side) as usize;
    let center = (side as f32 + 1.) * 0.5;
    let mut basis = Vec::with_capacity(count * n);
    for y in 0..side {
        let y = (y + 1) as f32 - center;
        for x in 0..side {
            let x = (x + 1) as f32 - center;
            if n == 3 {
                basis.extend([y, x, 1.]);
            } else {
                basis.extend([y * y, x * x, y * x, y, x, 1.]);
            }
        }
    }
    // These three fixed geometric fits have no singular value near the native
    // 1e-4 cutoff, so a small double-precision solve is sufficient here.
    // The weights depend only on coordinates, not on photo samples.
    let mut gram = [[0_f64; 12]; 6];
    for row in basis.chunks(n) {
        for i in 0..n {
            for j in 0..n {
                gram[i][j] = f64::from(row[i]).mul_add(f64::from(row[j]), gram[i][j]);
            }
        }
    }
    for (i, row) in gram.iter_mut().enumerate().take(n) {
        row[n + i] = 1.;
    }
    for k in 0..n {
        let pivot = (k..n)
            .max_by(|&i, &j| gram[i][k].abs().total_cmp(&gram[j][k].abs()))
            .unwrap();
        gram.swap(k, pivot);
        let divisor = gram[k][k];
        if !divisor.is_finite() || divisor.abs() < 1e-12 {
            return Err("singular geometric banding basis".into());
        }
        for value in &mut gram[k][..2 * n] {
            *value /= divisor;
        }
        let pivot = gram[k];
        for (i, row) in gram.iter_mut().enumerate().take(n) {
            if i == k {
                continue;
            }
            let factor = row[k];
            for (value, pivot) in row[..2 * n].iter_mut().zip(&pivot[..2 * n]) {
                *value = (-factor).mul_add(*pivot, *value);
            }
        }
    }
    let mut inverse = vec![0_f32; count * n];
    for k in 0..n {
        for sample in 0..count {
            let mut value = 0_f64;
            for j in 0..n {
                value = gram[k][n + j].mul_add(f64::from(basis[sample * n + j]), value);
            }
            inverse[k * count + sample] = value as f32;
        }
    }
    Ok((n as u32, basis, inverse))
}

pub(crate) fn describe(metadata: &Metadata, calibration_gains: [f32; 3]) -> Result<Description> {
    let area = super::calibration::rectangle(metadata, "ActiveImageArea")?;
    if super::calibration::rectangle(metadata, "KeepImageArea")?
        != [0, 0, metadata.width as u32, metadata.height as u32]
    {
        return Err("row correction requires full-sensor keep geometry".into());
    }
    let iso = match metadata
        .matrix("CaptureISO")
        .filter(|m| m.encoding == MatrixEncoding::Float32 && m.dimensions == [1])
    {
        None => 0.,
        Some(m) if m.values.len() == 1 && (m.values[0] as f32).is_finite() => m.values[0] as f32,
        Some(_) => return Err("invalid row-correction capture ISO".into()),
    };
    if calibration_gains.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err("invalid row-correction gains".into());
    }
    let params = Params {
        source: [metadata.width as u32, metadata.height as u32, 0, 0],
        area,
        values: [
            0.,
            if iso < 800. {
                0.75
            } else {
                f32::from_bits(0x3f99999a)
            },
            0.,
            0.,
        ],
        output: [0, 0, metadata.width as u32, metadata.height as u32],
        gains: [
            calibration_gains[0],
            calibration_gains[1],
            calibration_gains[2],
            0.,
        ],
    };
    Ok(Description {
        params,
        native_height: metadata.height as u32,
        scale: crate::render::Scale::NATIVE,
        origin: [0; 2],
    })
}

#[cfg(test)]
fn prepare(metadata: &Metadata, calibration_gains: [f32; 3]) -> Result<Prepared> {
    describe(metadata, calibration_gains)?.materialize()
}

fn materialize(description: &Description) -> Result<Prepared> {
    let mut params = description.params;
    let area = params.area;
    let width = params.source[0];
    let height = params.source[1];
    let mut patches = Vec::new();
    let mut tables = Vec::new();
    let mut rows = Vec::new();
    let mut samples = 0_u32;
    let mut contributions = 0_u32;
    let mut batches = Vec::new();
    let mut links = vec![Vec::new(); height as usize];
    for (native_side, coefficients) in [(31_u32, 3), (73, 6), (151, 6)] {
        let side = (native_side.div_ceil(description.scale.divisor()) | 1).max(3);
        let first = patches.len() as u32;
        let (n, basis, inverse) = matrices(side, coefficients)?;
        let table = tables.len() as u32;
        tables.extend(basis);
        tables.extend(inverse);
        let columns = ((area[2] - area[0]) as f32 / side as f32).floor() as u32;
        let eighth = (area[3] - area[1]) / 8;
        for bank in 0..8_u32 {
            let span = if bank == 7 {
                eighth as f32
            } else {
                (eighth as f32 / side as f32).ceil() * side as f32
            };
            let base = if bank == 0 {
                area[1]
            } else {
                // Bank phase belongs to the physical sensor, independently of
                // working-grid padding and filter support.
                bank * (description.native_height / 8) / description.scale.divisor()
                    + description.origin[1]
            };
            let rows =
                ((span / side as f32).floor() as u32).min(height.saturating_sub(base + 1) / side);
            if columns == 0 || rows == 0 {
                continue;
            }
            if area[0] + columns * side > width || base + rows * side >= height {
                return Err("row patch bank exceeds full sensor".into());
            }
            for row in 0..rows {
                for column in 0..columns {
                    let origin_y = base + row * side;
                    let origin_x = area[0] + column * side;
                    // Output y consumes row y+1. Each patch contributes rows
                    // origin_y+1 through origin_y+side, across the full active width.
                    // Keep complete patches so fitting, rejection and ordered sums stay exact.
                    if origin_y >= params.output[3] || origin_y + side <= params.output[1] {
                        continue;
                    }
                    patches.push(Patch {
                        geometry: [origin_x, origin_y, side, n],
                        storage: [table, samples, contributions, 0],
                    });
                    for y in 0..side {
                        links[(origin_y + y + 1) as usize].push(contributions + y);
                    }
                    samples = samples
                        .checked_add(side * side)
                        .ok_or("row sample workspace overflow")?;
                    contributions = contributions
                        .checked_add(side)
                        .ok_or("row contribution workspace overflow")?;
                }
            }
        }
        let count = patches.len() as u32 - first;
        if count > 0 {
            batches.push([first, count, side, n]);
        }
    }
    let mut flattened_links = Vec::new();
    for row in links {
        rows.push([flattened_links.len() as u32, row.len() as u32]);
        flattened_links.extend(row);
    }
    params.source[2] = u32::try_from(patches.len()).map_err(|_| "too many row patches")?;
    params.source[3] = contributions;
    Ok(Prepared {
        params,
        patches: patches.into(),
        tables: tables.into(),
        links: flattened_links.into(),
        rows: rows.into(),
        contributions,
        batches: batches.into(),
    })
}
pub(crate) struct Output {
    pub image: Option<crate::gpu::ReadGuard>,
    pub rows: crate::gpu::ReadGuard,
}
pub(crate) fn apply(
    gpu: &crate::gpu::Gpu,
    input: &crate::gpu::ReadGuard,
    p: &Prepared,
    want_image: bool,
    profile: &mut crate::profile::Profile,
) -> Result<Output> {
    use crate::gpu::Kernel;
    let mut offsets = gpu.storage(
        "Applied row biases and sample counts",
        u64::from(p.params.source[1]) * 16,
    )?;
    let mut trace = gpu.trace(profile.mode.gpu(), 3 + 3 * p.batches.len() as u32);
    if p.patches.is_empty() {
        let image = if !want_image {
            None
        } else if p.params.output == [0, 0, p.params.source[0], p.params.source[1]] {
            let mut output = gpu.storage("Unchanged row-correction layers", input.size())?;
            gpu.copy(input, output.write(), input.size());
            Some(output)
        } else {
            Some(apply_rows(gpu, input, &offsets, &p.params, &mut trace)?)
        };
        gpu.finish_trace(&trace, profile)?;
        return Ok(Output {
            image,
            rows: offsets,
        });
    }
    let uniform = gpu.uniform(&p.params);
    let patches = gpu.upload("Row patch traversal", &p.patches)?;
    let tables = gpu.upload("Geometric fitting basis and inverse", &p.tables)?;
    let mut coefficients = gpu.storage(
        "Row patch fitted coefficients",
        p.patches.len() as u64 * u64::from(MAX_FIT_COEFFICIENTS) * 3 * 4,
    )?;
    let samples = p
        .batches
        .iter()
        .map(|b| u64::from(b[1]) * u64::from(b[2]) * u64::from(b[2]))
        .max()
        .unwrap();
    let mut residuals = gpu.storage("Reused row patch residual workspace", samples * 12)?;
    let mut accepted = gpu.storage(
        "Row patch correlation acceptance",
        p.patches.len() as u64 * 4,
    )?;
    let mut means = gpu.storage(
        "Ordered row patch contributions",
        u64::from(p.contributions) * 16,
    )?;
    let ranges = gpu.upload("Row contribution ranges", &p.rows)?;
    let links = gpu.upload("Stable row contribution links", &p.links)?;
    gpu.dispatch(
        Kernel::BandingFit,
        &mut [
            (0, Read(input)),
            (1, Read(&patches)),
            (2, Read(&uniform)),
            (3, Read(&tables)),
            (4, Write(coefficients.write())),
        ],
        [(p.params.source[2] * MAX_FIT_COEFFICIENTS).div_ceil(32), 1],
        &mut trace,
    );
    for batch in p.batches.iter() {
        let batch_uniform = gpu.uniform(batch);
        gpu.dispatch(
            Kernel::BandingResidual,
            &mut [
                (0, Read(input)),
                (1, Read(&patches)),
                (2, Read(&uniform)),
                (3, Read(&tables)),
                (4, Read(&coefficients)),
                (5, Write(residuals.write())),
                (8, Read(&batch_uniform)),
            ],
            [(batch[2] * batch[2]).div_ceil(64), batch[1]],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::BandingQuality,
            &mut [
                (0, Read(input)),
                (1, Read(&patches)),
                (2, Read(&uniform)),
                (5, Read(&residuals)),
                (6, Write(accepted.write())),
                (8, Read(&batch_uniform)),
            ],
            [batch[1], 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::BandingMeans,
            &mut [
                (1, Read(&patches)),
                (5, Read(&residuals)),
                (6, Read(&accepted)),
                (7, Write(means.write())),
                (8, Read(&batch_uniform)),
            ],
            [batch[2].div_ceil(32), batch[1]],
            &mut trace,
        );
    }
    gpu.dispatch(
        Kernel::BandingReduce,
        &mut [
            (2, Read(&uniform)),
            (7, Read(&means)),
            (9, Read(&ranges)),
            (10, Read(&links)),
            (11, Write(offsets.write())),
        ],
        [p.params.source[1].div_ceil(64), 1],
        &mut trace,
    );
    let image = if want_image {
        Some(apply_rows(gpu, input, &offsets, &p.params, &mut trace)?)
    } else {
        None
    };
    gpu.finish_trace(&trace, profile)?;
    Ok(Output {
        image,
        rows: offsets,
    })
}

fn apply_rows(
    gpu: &crate::gpu::Gpu,
    input: &crate::gpu::ReadGuard,
    offsets: &crate::gpu::ReadGuard,
    params: &Params,
    trace: &mut crate::gpu::GpuTrace,
) -> Result<crate::gpu::ReadGuard> {
    use crate::gpu::Kernel;
    let extent = [
        params.output[2] - params.output[0],
        params.output[3] - params.output[1],
    ];
    let mut output = gpu.storage(
        "Row conditioned layers",
        u64::from(extent[0]) * u64::from(extent[1]) * 12,
    )?;
    let uniform = gpu.uniform(params);
    gpu.dispatch(
        Kernel::BandingApply,
        &mut [
            (0, Read(input)),
            (2, Read(&uniform)),
            (11, Read(offsets)),
            (12, Write(output.write())),
        ],
        extent.map(|v| v.div_ceil(8)),
        trace,
    );
    Ok(output)
}
#[cfg(test)]
#[path = "banding_test.rs"]
mod tests;
