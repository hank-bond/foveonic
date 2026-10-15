//! Prepares coordinate-only row-correction weights while keeping photo measurements on the GPU.

//!
//! Adapted with owner permission from Foveonic banding/mod.rs and patches.rs at
//! e47af35. Host fitting matrices depend only on fixed geometric polynomial bases.
use crate::gpu::Binding::{Read, Write};
use crate::source::{MatrixEncoding, Metadata, Result};
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    pub source: [u32; 4],
    pub area: [u32; 4],
    pub values: [f32; 4],
    pub gains: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Patch {
    pub geometry: [u32; 4],
    pub storage: [u32; 4],
}
pub(crate) struct Prepared {
    pub params: Params,
    pub patches: Vec<Patch>,
    pub tables: Vec<f32>,
    pub links: Vec<u32>,
    pub rows: Vec<[u32; 2]>,
    pub samples: u32,
    pub contributions: u32,
    pub batches: Vec<[u32; 4]>,
}

impl Prepared {
    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::pod(&self.params)
            .slice(&self.patches)
            .slice(&self.tables)
            .slice(&self.links)
            .slice(&self.rows)
            .with(&[self.samples, self.contributions])
            .slice(&self.batches)
    }
}

fn matrices(side: u32) -> Result<(u32, Vec<f32>, Vec<f32>)> {
    if ![31, 73, 151].contains(&side) {
        return Err("unsupported row-banding patch side".into());
    }
    let n = if side == 31 { 3 } else { 6 };
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

pub(crate) fn prepare(metadata: &Metadata, calibration_gains: [f32; 3]) -> Result<Prepared> {
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
    let mut p = Prepared {
        params: Params {
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
            gains: [
                calibration_gains[0],
                calibration_gains[1],
                calibration_gains[2],
                0.,
            ],
        },
        patches: Vec::new(),
        tables: Vec::new(),
        links: Vec::new(),
        rows: Vec::new(),
        samples: 0,
        contributions: 0,
        batches: Vec::new(),
    };
    let mut links = vec![Vec::new(); metadata.height];
    for side in [31, 73, 151] {
        let first = p.patches.len() as u32;
        let (n, basis, inverse) = matrices(side)?;
        let table = p.tables.len() as u32;
        p.tables.extend(basis);
        p.tables.extend(inverse);
        let columns = ((area[2] - area[0]) as f32 / side as f32).floor() as u32;
        let eighth = (area[3] - area[1]) / 8;
        for bank in 0..8_u32 {
            let span = if bank == 7 {
                eighth as f32
            } else {
                (eighth as f32 / side as f32).ceil() * side as f32
            };
            let rows = (span / side as f32).floor() as u32;
            let base = if bank == 0 {
                area[1]
            } else {
                bank * (metadata.height as u32 / 8)
            };
            if columns == 0 || rows == 0 {
                continue;
            }
            if area[0] + columns * side > metadata.width as u32
                || base + rows * side >= metadata.height as u32
            {
                return Err("row patch bank exceeds full sensor".into());
            }
            for row in 0..rows {
                for column in 0..columns {
                    let origin_y = base + row * side;
                    let origin_x = area[0] + column * side;
                    p.patches.push(Patch {
                        geometry: [origin_x, origin_y, side, n],
                        storage: [table, p.samples, p.contributions, 0],
                    });
                    for y in 0..side {
                        links[(origin_y + y + 1) as usize].push(p.contributions + y);
                    }
                    p.samples = p
                        .samples
                        .checked_add(side * side)
                        .ok_or("row sample workspace overflow")?;
                    p.contributions = p
                        .contributions
                        .checked_add(side)
                        .ok_or("row contribution workspace overflow")?;
                }
            }
        }
        let count = p.patches.len() as u32 - first;
        if count > 0 {
            p.batches.push([first, count, side, n]);
        }
    }
    for row in links {
        p.rows.push([p.links.len() as u32, row.len() as u32]);
        p.links.extend(row);
    }
    p.params.source[2] = u32::try_from(p.patches.len()).map_err(|_| "too many row patches")?;
    p.params.source[3] = p.contributions;
    Ok(p)
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
        "Applied row biases and native counts",
        u64::from(p.params.source[1]) * 16,
    )?;
    if p.patches.is_empty() {
        let image = if want_image {
            let mut output = gpu.storage("Unchanged row-correction layers", input.size())?;
            gpu.copy(input, output.write(), input.size());
            Some(output)
        } else {
            None
        };
        return Ok(Output {
            image,
            rows: offsets,
        });
    }
    let uniform = gpu.uniform(&p.params);
    let patches = gpu.upload("Native row patch traversal", &p.patches)?;
    let tables = gpu.upload("Geometric fitting basis and inverse", &p.tables)?;
    let mut coefficients = gpu.storage(
        "Row patch fitted coefficients",
        p.patches.len() as u64 * 6 * 3 * 4,
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
    let mut trace = gpu.trace(profile.mode.gpu(), 3 + 3 * p.batches.len() as u32);
    gpu.dispatch(
        Kernel::BandingFit,
        &mut [
            (0, Read(input)),
            (1, Read(&patches)),
            (2, Read(&uniform)),
            (3, Read(&tables)),
            (4, Write(coefficients.write())),
        ],
        [p.params.source[2].div_ceil(32), 1],
        &mut trace,
    );
    for batch in &p.batches {
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
            [batch[1].div_ceil(32), 1],
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
        let mut output = gpu.storage("Row conditioned layers", input.size())?;
        gpu.dispatch(
            Kernel::BandingApply,
            &mut [
                (0, Read(input)),
                (2, Read(&uniform)),
                (11, Read(&offsets)),
                (12, Write(output.write())),
            ],
            [
                p.params.source[0].div_ceil(8),
                p.params.source[1].div_ceil(8),
            ],
            &mut trace,
        );
        Some(output)
    } else {
        None
    };
    gpu.finish_trace(&trace, profile)?;
    Ok(Output {
        image,
        rows: offsets,
    })
}
#[cfg(test)]
#[path = "banding_test.rs"]
mod tests;
