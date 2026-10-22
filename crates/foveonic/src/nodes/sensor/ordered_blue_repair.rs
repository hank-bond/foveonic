//! Groups ordered mono defect records into independent GPU batches without changing their dependencies.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Stage {
    Directional,
    DarkOffsets,
    General,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    counts: [u32; 4],
    controls: [u32; 4],
    gains: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    jobs: Vec<[u32; 4]>,
    marks: Vec<[u32; 2]>,
    batches: Vec<[u32; 2]>,
}

impl Prepared {
    /// Records contain row, column and original packed flags. Overrides and crop
    /// translation belong to preparation. General repair requires the directional
    /// mask and a nonempty eligible neighborhood for each selected record.
    pub(crate) fn new(
        extent: [u32; 2],
        records: Vec<[u32; 3]>,
        threshold: u32,
        blue_gain: f32,
        stage: Stage,
    ) -> Result<Self> {
        let [width, height] = extent;
        if extent.iter().any(|&v| v < 24)
            || u64::from(width) * u64::from(height) > u64::from(u32::MAX - 63)
            || records.iter().any(|r| r[2] > 65535)
            || !blue_gain.is_finite()
        {
            return Err("invalid ordered blue repair parameters".into());
        }
        let mut marks = BTreeMap::new();
        let mut selected = Vec::new();
        for [y, x, flags] in records {
            let valid = y > 1 && y < height - 2 && x > 1 && x < width - 1;
            match stage {
                Stage::Directional if valid => {
                    marks.insert(y * width + x, ((flags >> 13) ^ 255) & 2);
                }
                Stage::DarkOffsets if valid && flags & 0x4000 == 0 => {
                    marks.insert(y * width + x, 1);
                }
                _ => {}
            }
            let severity = flags & 255;
            let eligible = match stage {
                Stage::Directional => {
                    y > 10
                        && y < height - 10
                        && x > 10
                        && x < width - 10
                        && severity == 255
                        && flags & 0x6000 == 0
                        && threshold <= ((flags >> 8) & 7)
                }
                Stage::DarkOffsets => valid && severity != 0 && flags & 0x6000 == 0x4000,
                Stage::General => {
                    y > 10
                        && y < height - 11
                        && x > 10
                        && x < width - 11
                        && severity != 0
                        && severity != 255
                        && ((flags >> 13) & 3) < 2
                        && threshold <= ((flags >> 8) & 7)
                }
            };
            if eligible {
                selected.push([y, x, flags, 0]);
            }
        }
        // A job writes one center. Its complete search reads at most eleven
        // pixels away. Earlier conflicting jobs therefore receive lower levels.
        let radius = if matches!(stage, Stage::DarkOffsets) {
            1
        } else {
            11
        };
        let cell_size = radius + 1;
        // Stored points are (row, column, dependency level).
        type ScheduledPoints = Vec<(u32, u32, usize)>;
        let mut grid: HashMap<_, ScheduledPoints> = HashMap::new();
        let mut levels: Vec<Vec<[u32; 4]>> = Vec::new();
        for job in selected {
            let [y, x, _, _] = job;
            let cell = (y / cell_size, x / cell_size);
            let mut level = 0;
            for cy in cell.0.saturating_sub(1)..=cell.0 + 1 {
                for cx in cell.1.saturating_sub(1)..=cell.1 + 1 {
                    if let Some(prior) = grid.get(&(cy, cx)) {
                        for &(py, px, pl) in prior {
                            if py.abs_diff(y) <= radius && px.abs_diff(x) <= radius {
                                level = level.max(pl + 1);
                            }
                        }
                    }
                }
            }
            grid.entry(cell).or_default().push((y, x, level));
            if levels.len() <= level {
                levels.resize_with(level + 1, Vec::new);
            }
            levels[level].push(job);
        }
        let mut jobs = Vec::new();
        let mut batches = Vec::new();
        for level in levels {
            batches.push([u32::try_from(jobs.len())?, u32::try_from(level.len())?]);
            jobs.extend(level);
        }
        let count = u32::try_from(jobs.len())?;
        if jobs.is_empty() {
            jobs.push([0; 4]);
        }
        let mut marks: Vec<_> = marks.into_iter().map(|(i, v)| [i, v]).collect();
        let mark_count = u32::try_from(marks.len())?;
        if marks.is_empty() {
            marks.push([0; 2]);
        }
        Ok(Self {
            params: Params {
                shape: [width, height, stage as u32, threshold],
                counts: [count, mark_count, 0, 0],
                controls: [0; 4],
                gains: [blue_gain, 0., 0., 0.],
            },
            jobs,
            marks,
            batches,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn stage(&self) -> Stage {
        match self.params.shape[2] {
            0 => Stage::Directional,
            1 => Stage::DarkOffsets,
            2 => Stage::General,
            _ => unreachable!("validated numerical stage"),
        }
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
            .slice(&self.jobs)
            .slice(&self.marks)
            .slice(&self.batches)
    }
}

pub(crate) struct Output {
    pub(crate) blue: ReadGuard,
    pub(crate) mask: ReadGuard,
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prior_mask: Option<&ReadGuard>,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<Output> {
    let [width, height] = prepared.extent();
    let cells = u64::from(width) * u64::from(height);
    if input.size() != cells * 4 || prior_mask.is_some_and(|m| m.size() != cells * 4) {
        return Err("ordered blue repair inputs do not match their extents".into());
    }
    if prepared.params.shape[2] == Stage::General as u32 && prior_mask.is_none() {
        return Err("general blue repair requires the directional mask".into());
    }
    let empty = gpu.upload("No prior blue mask", &[0u32])?;
    let prior_mask = prior_mask.unwrap_or(&empty);
    let jobs = gpu.upload("Ordered blue repair jobs", &prepared.jobs)?;
    let marks = gpu.upload("Blue defect marks", &prepared.marks)?;
    let mut blue = gpu.storage("Repaired blue codes", cells * 4)?;
    let mut mask = gpu.storage("Blue repair mask", cells * 4)?;
    let mut trace = gpu.trace(profile.mode.gpu(), prepared.batches.len() as u32 + 2);
    for (phase, start, count) in std::iter::once((0, 0, u32::try_from(cells)?))
        .chain(std::iter::once((1, 0, prepared.params.counts[1])))
        .chain(prepared.batches.iter().map(|b| (2, b[0], b[1])))
    {
        if count == 0 {
            continue;
        }
        let mut params = prepared.params;
        params.controls = [
            phase,
            start,
            count,
            u32::from(prepared.params.shape[2] == Stage::General as u32),
        ];
        // Repair candidates cooperate within one workgroup per ordered job.
        // Initialization and mask marking still assign one item to each lane.
        let groups = if phase == 2 {
            count
        } else {
            count.div_ceil(64)
        };
        let groups_x = groups.min(gpu.device.limits().max_compute_workgroups_per_dimension);
        params.counts[2] = groups_x * 64;
        let uniform = gpu.uniform(&params);
        gpu.dispatch(
            Kernel::OrderedBlueRepair,
            &mut [
                (0, Read(input)),
                (1, Read(prior_mask)),
                (2, Read(&uniform)),
                (3, Read(&jobs)),
                (4, Read(&marks)),
                (5, Write(blue.write())),
                (6, Write(mask.write())),
            ],
            [groups_x, groups.div_ceil(groups_x)],
            &mut trace,
        );
    }
    gpu.finish_trace(&trace, profile)?;
    Ok(Output { blue, mask })
}

#[cfg(test)]
#[path = "ordered_blue_repair_test.rs"]
mod tests;
