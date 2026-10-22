//! Repairs ordered RGB defect records with directional and masked-neighborhood passes.

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
pub enum Stage {
    Single,
    General,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    controls: [u32; 4],
}
#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    records: Vec<[u32; 4]>,
    batches: [Vec<[u32; 2]>; 2],
    noise: [f32; 12],
    transforms: Vec<[f32; 2]>,
}
impl Prepared {
    /// Records contain row, column and packed neighbor/channel/severity bits.
    /// Both passes preserve dependencies between earlier writes. Mask samples are
    /// immutable zero/nonzero exclusions, widened from the reference byte mask.
    pub(crate) fn new(
        extent: [u32; 2],
        records: Vec<[u32; 3]>,
        threshold: u32,
        noise: [f32; 9],
        matrix: [f64; 9],
        force_blue_and_joint: bool,
        single_uses_threshold: bool,
    ) -> Result<Self> {
        if extent.iter().any(|&v| v < 4)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
            || noise.iter().any(|v| !v.is_finite())
            || matrix.iter().any(|v| !v.is_finite())
            || records.iter().any(|r| r[2] > 65535)
            || records.len() > u32::MAX as usize
        {
            return Err("invalid ordered chroma repair parameters".into());
        }
        let transforms = projection(matrix)?;
        let mut coefficients = [0.; 12];
        coefficients[..9].copy_from_slice(&noise);
        let mut jobs = Vec::new();
        let mut batches = [Vec::new(), Vec::new()];
        for (stage, stage_batches) in batches.iter_mut().enumerate() {
            let selected = records.iter().filter(|&&[y, x, flags]| {
                if y <= 1 || y >= extent[1] - 1 || x <= 1 || x >= extent[0] - 1 {
                    return false;
                }
                let bits = flags & 255;
                let severity = (flags >> 8) & 15;
                if stage == 0 {
                    bits == 255 && (!single_uses_threshold || threshold <= severity)
                } else {
                    bits != 0 && bits != 255 && threshold <= severity
                }
            });
            for level in schedule(extent, selected.map(|r| [r[0], r[1], r[2], 0])) {
                stage_batches.push([u32::try_from(jobs.len())?, u32::try_from(level.len())?]);
                jobs.extend(level);
            }
        }
        if jobs.is_empty() {
            jobs.push([0; 4]);
        }
        Ok(Self {
            params: Params {
                shape: [extent[0], extent[1], 0, threshold],
                controls: [
                    u32::from(force_blue_and_joint),
                    u32::from(single_uses_threshold),
                    0,
                    0,
                ],
            },
            records: jobs,
            batches,
            noise: coefficients,
            transforms,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
            .slice(&self.records)
            .slice(&self.batches[0])
            .slice(&self.batches[1])
            .slice(&self.noise)
            .slice(&self.transforms)
    }
}
// Bound temporary dense scheduling storage to 64 MiB. Larger sparse inputs
// retain coordinate hashing instead of allocating a full image-sized table.
const MAX_DENSE_DEPENDENCY_CELLS: u64 = 16 * 1024 * 1024;
enum Dependencies {
    Dense { width: u32, levels: Vec<u32> },
    Sparse(std::collections::HashMap<(u32, u32), u32>),
}
impl Dependencies {
    fn new([width, height]: [u32; 2]) -> Self {
        let cells = u64::from(width) * u64::from(height);
        if cells <= MAX_DENSE_DEPENDENCY_CELLS {
            Self::Dense {
                width,
                levels: vec![0; cells as usize],
            }
        } else {
            Self::Sparse(std::collections::HashMap::new())
        }
    }
    fn get(&self, y: u32, x: u32) -> u32 {
        match self {
            Self::Dense { width, levels } => levels[(y * width + x) as usize],
            Self::Sparse(levels) => levels.get(&(y, x)).copied().unwrap_or(0),
        }
    }
    fn set(&mut self, y: u32, x: u32, level: u32) {
        match self {
            Self::Dense { width, levels } => levels[(y * *width + x) as usize] = level,
            Self::Sparse(levels) => {
                levels.insert((y, x), level);
            }
        }
    }
}
/// Independent records can run together. A record reads its eight neighbors
/// and writes its center, so every earlier center within one pixel must finish.
fn schedule(extent: [u32; 2], records: impl IntoIterator<Item = [u32; 4]>) -> Vec<Vec<[u32; 4]>> {
    let mut previous = Dependencies::new(extent);
    let mut levels: Vec<Vec<[u32; 4]>> = Vec::new();
    for record in records {
        let [y, x, _, _] = record;
        let mut level = 0;
        for py in y - 1..=y + 1 {
            for px in x - 1..=x + 1 {
                level = level.max(previous.get(py, px));
            }
        }
        previous.set(y, x, level + 1);
        let level = level as usize;
        if levels.len() <= level {
            levels.resize_with(level + 1, Vec::new);
        }
        levels[level].push(record);
    }
    levels
}

/// Double camera-to-luma/chroma projection shared by ordinary defect filters.
pub(crate) fn projection(matrix: [f64; 9]) -> Result<Vec<[f32; 2]>> {
    let inverse = inverse(matrix)?;
    let forward = matmul(
        [
            0.299, 0.587, 0.114, -0.14713, -0.28886, 0.436, 0.615, -0.51499, -0.10001,
        ],
        matrix,
    );
    let backward = matmul(
        inverse,
        [1., 0., 1.13983, 1., -0.39465, -0.5806, 1., 2.03211, 0.],
    );
    let transforms = forward
        .into_iter()
        .chain(backward)
        .map(|v| {
            let high = v as f32;
            [high, (v - f64::from(high)) as f32]
        })
        .collect();
    Ok(transforms)
}
fn matmul(a: [f64; 9], b: [f64; 9]) -> [f64; 9] {
    std::array::from_fn(|i| {
        let r = i / 3;
        let c = i % 3;
        a[r * 3 + 2].mul_add(b[6 + c], a[r * 3 + 1].mul_add(b[3 + c], a[r * 3] * b[c]))
    })
}
fn inverse(a: [f64; 9]) -> Result<[f64; 9]> {
    let [a, b, c, d, e, f, g, h, i] = a;
    let co = [
        e * i - f * h,
        c * h - b * i,
        b * f - c * e,
        f * g - d * i,
        a * i - c * g,
        c * d - a * f,
        d * h - e * g,
        b * g - a * h,
        a * e - b * d,
    ];
    let determinant = a * co[0] + b * co[3] + c * co[6];
    if !determinant.is_finite() || determinant == 0. {
        return Err("singular chroma repair color projection".into());
    }
    let inverse = co.map(|v| v / determinant);
    if inverse.iter().any(|v| !v.is_finite()) {
        return Err("invalid chroma repair color projection".into());
    }
    Ok(inverse)
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    mask: &ReadGuard,
    prepared: &Prepared,
    stage: Stage,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h] = prepared.extent();
    let cells = u64::from(w) * u64::from(h);
    if input.size() != cells * 12 || mask.size() != cells * 4 {
        return Err("chroma repair input geometry mismatch".into());
    }
    let mut output = gpu.storage("Ordered chroma repair", input.size())?;
    gpu.copy(input, output.write(), input.size());
    let stage_batches = &prepared.batches[match stage {
        Stage::Single => 0,
        Stage::General => 1,
    }];
    if stage_batches.is_empty() {
        return Ok(output);
    }
    let records = gpu.upload("Chroma defect records", &prepared.records)?;
    let noise = gpu.upload("Chroma noise model", &prepared.noise)?;
    let matrix = gpu.upload("Chroma color projection", &prepared.transforms)?;
    let mut trace = gpu.trace(profile.mode.gpu(), stage_batches.len() as u32);
    for &[start, count] in stage_batches {
        let groups = count.div_ceil(64);
        let groups_x = groups.min(gpu.device.limits().max_compute_workgroups_per_dimension);
        let mut params = prepared.params;
        params.shape[2] = groups_x * 64;
        params.controls[2] = start;
        params.controls[3] = count;
        let params = gpu.uniform(&params);
        let mut bindings = vec![
            (0, Write(output.write())),
            (1, Read(mask)),
            (2, Read(&params)),
            (3, Read(&records)),
            (4, Read(&noise)),
        ];
        if matches!(stage, Stage::Single) {
            bindings.push((5, Read(&matrix)));
        }
        gpu.dispatch(
            match stage {
                Stage::Single => Kernel::ChromaSingle,
                Stage::General => Kernel::ChromaGeneral,
            },
            &mut bindings,
            [groups_x, groups.div_ceil(groups_x)],
            &mut trace,
        );
    }
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
#[cfg(test)]
#[path = "chroma_repair_test.rs"]
mod tests;
