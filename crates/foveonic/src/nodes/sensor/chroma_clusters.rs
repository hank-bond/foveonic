//! Repairs ordered RGB clusters by matching nearby valid three-by-three templates.
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};
// Bounds and minimum search margin used by the reference template matcher.
const MAX_CLUSTER_DIMENSION: u32 = 25;
const MAX_SEARCH_PIXELS: u32 = 10000;
const MIN_SEARCH_MARGIN: u32 = 5;
const ALL_CHANNELS: u32 = 7;
/// Packed catalogue fields decoded by the recipe without reading image pixels.
#[derive(Clone, Debug)]
pub(crate) struct Cluster {
    pub severity: u32,
    pub extent: [u32; 2],
    pub origin: [u32; 2],
    pub margin: u32,
    pub points: Vec<[u32; 3]>,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Region {
    rect: [u32; 4],
    ranges: [u32; 4],
    weights: [u32; 4],
}
#[derive(Clone, Debug)]
pub struct Prepared {
    extent: [u32; 2],
    regions: Vec<Region>,
    batches: Vec<[u32; 2]>,
    points: Vec<[u32; 2]>,
    candidates: Vec<[u32; 2]>,
    weights: Vec<u32>,
}
impl Prepared {
    pub(crate) fn new(
        extent: [u32; 2],
        clusters: Vec<Cluster>,
        severity: u32,
        margin: u32,
    ) -> Result<Self> {
        let [width, height] = extent;
        if width < 5 || height < 5 || u64::from(width) * u64::from(height) > u64::from(u32::MAX) / 3
        {
            return Err("invalid chroma cluster geometry".into());
        }
        let mut result = Self {
            extent,
            regions: vec![],
            batches: vec![],
            points: vec![],
            candidates: vec![],
            weights: vec![],
        };
        for c in clusters.into_iter().filter(|c| c.severity == severity) {
            let [cw, ch] = c.extent;
            let [x, y] = c.origin;
            if x < 2
                || y < 2
                || x > width
                || y > height
                || cw > MAX_CLUSTER_DIMENSION
                || ch > MAX_CLUSTER_DIMENSION
                || cw == 0
                || ch == 0
            {
                return Err("unsupported chroma cluster header".into());
            }
            // Both catalogue origin and sorted repair targets have a native minus-one
            // coordinate adjustment. They are distinct and intentionally retained.
            let ax = x - 1;
            let ay = y - 1;
            let radius = margin.min(c.margin).max(MIN_SEARCH_MARGIN);
            let left = ax.saturating_sub(radius);
            let top = ay.saturating_sub(radius);
            let right = (ax + cw + radius).min(width - 1);
            let bottom = (ay + ch + radius).min(height - 1);
            let w = right - left + 1;
            let h = bottom - top + 1;
            if w * h > MAX_SEARCH_PIXELS {
                return Err("unsupported chroma cluster search region".into());
            }
            let mut weights = vec![ALL_CHANNELS; (w * h) as usize];
            for [dy, dx, bits] in &c.points {
                let px = ax - left + dx;
                let py = ay - top + dy;
                if px < 2 || py < 2 || px >= w - 2 || py >= h - 2 {
                    return Err("clipped chroma cluster support".into());
                }
                let channels = match bits {
                    1..=6 => *bits,
                    _ => ALL_CHANNELS,
                };
                weights[(py * w + px) as usize] &= !channels;
            }
            let mut targets = vec![];
            for py in (ay - top).saturating_sub(1)..(ay - top + ch + 1).min(h) {
                for px in (ax - left).saturating_sub(1)..(ax - left + cw + 1).min(w) {
                    if weights[(py * w + px) as usize] != ALL_CHANNELS {
                        if px < 2 || py < 2 || px >= w - 2 || py >= h - 2 {
                            return Err("invalid chroma cluster target support".into());
                        }
                        let mut support = 0;
                        for yy in py - 2..=py + 2 {
                            for xx in px - 2..=px + 2 {
                                support += weights[(yy * w + xx) as usize].count_ones();
                            }
                        }
                        targets.push((support, [px - 1, py - 1]));
                    }
                }
            }
            if targets.len() != c.points.len() {
                return Err("chroma cluster point-count mismatch".into());
            }
            targets.sort_by_key(|(support, _)| std::cmp::Reverse(*support));
            let point_start = result.points.len() as u32;
            result.points.extend(targets.into_iter().map(|(_, p)| p));
            let candidate_start = result.candidates.len() as u32;
            for py in 1..h - 2 {
                for px in 1..w - 2 {
                    let mut good = true;
                    for yy in py - 1..=py + 1 {
                        for xx in px - 1..=px + 1 {
                            good &= weights[(yy * w + xx) as usize] == ALL_CHANNELS;
                        }
                    }
                    if good {
                        result.candidates.push([px, py]);
                    }
                }
            }
            result.regions.push(Region {
                rect: [left, top, w, h],
                ranges: [
                    point_start,
                    result.points.len() as u32 - point_start,
                    candidate_start,
                    result.candidates.len() as u32 - candidate_start,
                ],
                weights: [result.weights.len() as u32, 0, 0, 0],
            });
            result.weights.extend(weights);
        }
        // Search regions read only their own rectangles. Disjoint rectangles
        // can run together, while intersections retain catalogue order.
        let mut levels: Vec<Vec<Region>> = Vec::new();
        let mut prior = Vec::new();
        for region in result.regions.drain(..) {
            let mut level = 0;
            for &(rect, previous_level) in &prior {
                if intersects(region.rect, rect) {
                    level = level.max(previous_level + 1);
                }
            }
            prior.push((region.rect, level));
            if levels.len() <= level {
                levels.resize_with(level + 1, Vec::new);
            }
            levels[level].push(region);
        }
        for level in levels {
            result.batches.push([
                u32::try_from(result.regions.len())?,
                u32::try_from(level.len())?,
            ]);
            result.regions.extend(level);
        }
        Ok(result)
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        self.extent
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.extent)
            .slice(&self.regions)
            .slice(&self.batches)
            .slice(&self.points)
            .slice(&self.candidates)
            .slice(&self.weights)
    }
}
fn intersects(a: [u32; 4], b: [u32; 4]) -> bool {
    a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    mask: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h] = prepared.extent();
    let cells = u64::from(w) * u64::from(h);
    if input.size() != cells * 12 || mask.size() != cells * 4 {
        return Err("chroma cluster input geometry mismatch".into());
    }
    let mut output = gpu.storage("Ordered chroma clusters", input.size())?;
    gpu.copy(input, output.write(), input.size());
    if prepared.regions.is_empty() {
        return Ok(output);
    }
    let regions = gpu.upload("Cluster regions", &prepared.regions)?;
    let points = gpu.upload("Cluster target order", &prepared.points)?;
    let candidates = gpu.upload("Cluster candidates", &prepared.candidates)?;
    let weights = gpu.upload("Cluster initial weights", &prepared.weights)?;
    let mut scratch = gpu.storage("Cluster mutable weights", weights.size())?;
    gpu.copy(&weights, scratch.write(), weights.size());
    let maximum_groups = gpu.device.limits().max_compute_workgroups_per_dimension;
    let dispatch_count: u32 = prepared
        .batches
        .iter()
        .map(|b| b[1].div_ceil(maximum_groups))
        .sum();
    let mut trace = gpu.trace(profile.mode.gpu(), dispatch_count);
    for &[start, count] in &prepared.batches {
        for offset in (0..count).step_by(maximum_groups as usize) {
            let count = (count - offset).min(maximum_groups);
            let p = gpu.uniform(&[w, h, count, start + offset]);
            gpu.dispatch(
                Kernel::ChromaClusters,
                &mut [
                    (0, Write(output.write())),
                    (1, Read(mask)),
                    (2, Read(&p)),
                    (3, Read(&regions)),
                    (4, Read(&points)),
                    (5, Read(&candidates)),
                    (6, Write(scratch.write())),
                ],
                [count, 1],
                &mut trace,
            );
        }
    }
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
#[cfg(test)]
#[path = "chroma_clusters_test.rs"]
mod tests;
