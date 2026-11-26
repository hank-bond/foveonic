//! Repairs ordered mono clusters with parallel candidate scoring and stable source-order ties.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct Cluster {
    /// Absolute row and column in the physical mono plane.
    pub origin: [u32; 2],
    /// Height and width of the packed defect footprint.
    pub extent: [u32; 2],
    pub margin: u32,
    /// Relative row and column of each defective point.
    pub points: Vec<[u32; 2]>,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Job {
    first_point: u32,
    point_count: u32,
    first_candidate: u32,
    candidate_count: u32,
}

/// One batch dispatch: the image width and the batch's first job.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Batch {
    width: u32,
    first_job: u32,
    _padding: [u32; 2],
}

// Inclusive bounds in row and column order.
fn overlaps(a: [u32; 4], b: [u32; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

#[derive(Clone, Debug)]
pub struct Prepared {
    extent: [u32; 2],
    jobs: Vec<Job>,
    batches: Vec<[u32; 2]>,
    points: Vec<[u32; 4]>,
    candidates: Vec<u32>,
}
impl Prepared {
    pub(crate) fn new(extent: [u32; 2], clusters: Vec<Cluster>, margin: u32) -> Result<Self> {
        let [width, height] = extent;
        if extent.iter().any(|&v| v < 16)
            || u64::from(width) * u64::from(height) > u64::from(u32::MAX)
        {
            return Err("invalid luma cluster image geometry".into());
        }
        let mut levels: Vec<Vec<Job>> = Vec::new();
        let mut prior = Vec::new();
        let mut points = Vec::new();
        let mut candidates = Vec::new();
        for cluster in clusters {
            let [row, col] = cluster.origin;
            let [ch, cw] = cluster.extent;
            if row <= 5
                || row >= height - 5
                || col <= 5
                || col >= width - 5
                || ch == 0
                || cw == 0
                || ch.max(cw) > 25
                || cluster.points.len() > 255
            {
                return Err("unsupported luma cluster header".into());
            }
            let radius = margin.min(cluster.margin).max(5);
            let top = row.saturating_sub(radius);
            let left = col.saturating_sub(radius);
            let bottom = row
                .saturating_add(ch)
                .saturating_add(radius)
                .min(height - 1);
            let right = col.saturating_add(cw).saturating_add(radius).min(width - 1);
            let rw = right - left + 1;
            let rh = bottom - top + 1;
            if u64::from(rw) * u64::from(rh) > 10000 || left == 0 || top == 0 {
                return Err("unsupported clipped luma cluster search region".into());
            }
            if cluster.points.is_empty() {
                continue;
            }
            let mut bad = BTreeSet::new();
            for &[dy, dx] in &cluster.points {
                let y = row + dy - top;
                let x = col + dx - left;
                if dy >= ch
                    || dx >= cw
                    || y < 2
                    || y + 2 >= rh
                    || x < 2
                    || x + 2 >= rw
                    || !bad.insert((y, x))
                {
                    return Err("unsupported luma cluster point geometry".into());
                }
            }
            let initial = bad.clone();
            let mut priority: Vec<_> = bad
                .iter()
                .map(|&(y, x)| {
                    let support = 25
                        - initial
                            .iter()
                            .filter(|&&(by, bx)| by.abs_diff(y) <= 2 && bx.abs_diff(x) <= 2)
                            .count();
                    (support, y, x)
                })
                .collect();
            priority.sort_by_key(|&(support, _, _)| std::cmp::Reverse(support));
            let start = u32::try_from(points.len())?;
            for (_, y, x) in priority {
                let mut mask = 0u32;
                for dy in -2..=2 {
                    for dx in -2..=2 {
                        if !bad.contains(&((y as i32 + dy) as u32, (x as i32 + dx) as u32)) {
                            mask |= 1 << ((dy + 2) * 5 + dx + 2);
                        }
                    }
                }
                points.push([(top + y) * width + left + x, mask, 0, 0]);
                bad.remove(&(y, x));
            }
            let candidate_start = u32::try_from(candidates.len())?;
            for y in 1..rh - 2 {
                for x in 1..rw - 2 {
                    if initial
                        .iter()
                        .all(|&(by, bx)| by.abs_diff(y) > 1 || bx.abs_diff(x) > 1)
                    {
                        candidates.push((top + y) * width + left + x);
                    }
                }
            }
            let candidate_count = u32::try_from(candidates.len())? - candidate_start;
            if candidate_count == 0 {
                return Err("luma cluster has no eligible candidate patches".into());
            }
            // The first donor center starts one pixel in and reads two pixels out.
            // Both read bounds include the cluster's own writes.
            let reads = [top - 1, left - 1, bottom, right];
            let writes = [row, col, row + ch - 1, col + cw - 1];
            let mut level = 0;
            for &(earlier_reads, earlier_writes, earlier_level) in &prior {
                if overlaps(reads, earlier_writes) || overlaps(writes, earlier_reads) {
                    level = level.max(earlier_level + 1);
                }
            }
            prior.push((reads, writes, level));
            if level == levels.len() {
                levels.push(Vec::new());
            }
            levels[level].push(Job {
                first_point: start,
                point_count: u32::try_from(cluster.points.len())?,
                first_candidate: candidate_start,
                candidate_count,
            });
        }
        let mut jobs = Vec::new();
        let mut batches = Vec::new();
        for level in levels {
            batches.push([u32::try_from(jobs.len())?, u32::try_from(level.len())?]);
            jobs.extend(level);
        }
        Ok(Self {
            extent,
            jobs,
            batches,
            points,
            candidates,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        self.extent
    }
    pub(crate) fn key(&self) -> Key {
        Key::default()
            .slice(&self.extent)
            .slice(&self.jobs)
            .slice(&self.batches)
            .slice(&self.points)
            .slice(&self.candidates)
    }
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = prepared.extent();
    if input.size() != u64::from(width) * u64::from(height) * 4 {
        return Err("luma cluster input does not match its extent".into());
    }
    if prepared.jobs.is_empty() {
        return Ok(input.clone());
    }
    let mut output = gpu.storage("Cluster-repaired blue", input.size())?;
    gpu.copy(input, output.write(), input.size());
    let points = gpu.upload("Luma cluster points", &prepared.points)?;
    let candidates = gpu.upload("Luma cluster candidates", &prepared.candidates)?;
    let jobs = gpu.upload("Luma cluster jobs", &prepared.jobs)?;
    let maximum_groups = gpu.device.limits().max_compute_workgroups_per_dimension;
    let dispatch_count = prepared
        .batches
        .iter()
        .map(|b| b[1].div_ceil(maximum_groups))
        .sum();
    let mut trace = gpu.trace(profile.mode.gpu(), dispatch_count);
    for &[start, count] in &prepared.batches {
        for offset in (0..count).step_by(maximum_groups as usize) {
            let uniform = gpu.uniform(&Batch {
                width,
                first_job: start + offset,
                _padding: [0; 2],
            });
            gpu.dispatch(
                Kernel::LumaCluster,
                &mut [
                    (0, Read(&uniform)),
                    (1, Read(&points)),
                    (2, Read(&candidates)),
                    (3, Write(output.write())),
                    (4, Read(&jobs)),
                ],
                [(count - offset).min(maximum_groups), 1],
                &mut trace,
            );
        }
    }
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
#[cfg(test)]
#[path = "luma_cluster_test.rs"]
mod tests;
