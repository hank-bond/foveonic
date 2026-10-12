//! Prepares complete native camera repair metadata and executes raw-signal bad-red selection, ordered lists, and masked cluster matching on GPU.
//!
//! Adapted with owner permission from Foveonic repair_setup.rs, bad_reds.rs,
//! and clusters.rs at e47af35. Original sensor data and metadata remain immutable.
use crate::{
    gpu::{Gpu, Kernel},
    profile::Profile,
    repair::Threshold,
    source::{MatrixEncoding, Metadata, Result},
};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[cfg_attr(test, derive(Default))]
struct Params {
    source: [u32; 4],    // width, height, main count, JPEG count
    selection: [u32; 4], // cutoff, detected capacity, dynamic base, reserved
    model: [[f32; 4]; 3],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Cluster {
    rect: [u32; 4],
    indices: [u32; 4],
}

#[cfg_attr(test, derive(Default))]
pub(crate) struct Prepared {
    params: Params,
    records: Vec<[u32; 2]>,
    clusters: Vec<Cluster>,
    members: Vec<[u32; 4]>,
    masks: Vec<u32>,
}

pub(crate) fn prepare(metadata: &Metadata, threshold: Threshold) -> Result<Prepared> {
    let cutoff = threshold.resolve(metadata)?;
    let main = crate::repair::prepare(metadata, 0)?;
    let jpeg = if metadata.matrix("Jpeg_BadClusters").is_some() {
        crate::repair::prepare_list(metadata, "Jpeg_BadClusters", 0)?.records
    } else {
        Vec::new()
    };
    let model = metadata
        .matrix("ColorNoiseModel")
        .ok_or("missing bad-red noise model")?;
    if model.encoding != MatrixEncoding::Float32
        || model.dimensions != [3, 3]
        || model.values.len() != 9
        || model.values.iter().any(|v| !v.is_finite())
    {
        return Err("invalid bad-red noise model".into());
    }
    if metadata.width > u16::MAX as usize
        || metadata.height > u16::MAX as usize
        || model.values.iter().any(|v| !(*v as f32).is_finite())
    {
        return Err("unsupported bad-red geometry or noise model".into());
    }
    let dynamic_base = u32::try_from(main.records.len() + jpeg.len())
        .map_err(|_| "too many camera repair records")?;
    let mut prepared = Prepared {
        params: Params {
            source: [
                main.params[0],
                main.params[1],
                main.params[2],
                jpeg.len() as u32,
            ],
            selection: [cutoff, 999, dynamic_base, 0],
            model: std::array::from_fn(|r| {
                std::array::from_fn(|c| {
                    if c < 3 {
                        model.values[r * 3 + c] as f32
                    } else {
                        0.
                    }
                })
            }),
        },
        records: main.records.into_iter().chain(jpeg).collect(),
        clusters: Vec::new(),
        members: Vec::new(),
        masks: Vec::new(),
    };
    prepared
        .records
        .resize(dynamic_base as usize + 1000, [0; 2]);
    let cluster_class = match threshold {
        Threshold::Manual(value) => value,
        Threshold::Camera => {
            let class = crate::repair::camera_class(metadata)?;
            if class == 0 {
                0
            } else {
                class
                    .checked_sub(crate::repair::camera_integer(
                        metadata,
                        "BCBadnessOffset",
                        1,
                    )?)
                    .ok_or("invalid cluster repair cutoff")?
            }
        }
    };
    let margin = crate::repair::camera_integer(metadata, "BadClusterMargin", 25)?;
    if let Some(matrix) = metadata.matrix("BadClustersF20") {
        if matrix
            .values
            .iter()
            .any(|v| !v.is_finite() || v.fract() != 0. || !(0. ..=65535.).contains(v))
        {
            return Err("invalid packed camera clusters".into());
        }
        let words: Vec<_> = matrix.values.iter().map(|v| *v as u16).collect();
        prepare_clusters(&mut prepared, &words, cluster_class, margin)?;
    }
    Ok(prepared)
}

fn prepare_clusters(
    p: &mut Prepared,
    words: &[u16],
    badness: u32,
    maximum_margin: u32,
) -> Result<()> {
    let w = p.params.source[0];
    let h = p.params.source[1];
    let mut offset = 0;
    while offset < words.len() {
        let header = words
            .get(offset..offset + 4)
            .ok_or("truncated camera cluster header")?;
        let count = u32::from(header[3] & 255);
        let members = words
            .get(offset + 4..offset + 4 + count as usize)
            .ok_or("truncated camera cluster members")?;
        offset += 4 + count as usize;
        if u32::from(header[0] >> 14) != badness {
            continue;
        }
        let columns = u32::from(header[0] & 127);
        let rows = u32::from((header[0] >> 7) & 127);
        let y = u32::from(header[1]);
        let x = u32::from(header[2]);
        if columns > 25 || rows > 25 {
            return Err("unsupported camera cluster extent".into());
        }
        if y >= h || x >= w {
            break;
        }
        if count == 0 {
            continue;
        }
        let margin = u32::from(header[3] >> 8).min(maximum_margin).max(5);
        let left = x.saturating_sub(margin);
        let top = y.saturating_sub(margin);
        let pw = (x + columns + margin).min(w - 1) - left + 1;
        let ph = (y + rows + margin).min(h - 1) - top + 1;
        if pw * ph > 10000 {
            return Err("camera cluster scratch exceeds native limit".into());
        }
        let mut mask = vec![7_u32; (pw * ph) as usize];
        let mut occupied = std::collections::BTreeSet::new();
        for &member in members {
            let dx = u32::from((member >> 4) & 63);
            let dy = u32::from(member >> 10);
            let layers = u32::from(member & 15);
            if dx > columns
                || dy > rows
                || layers == 0
                || layers > 7
                || x + dx < 2
                || y + dy < 2
                || x + dx >= w - 2
                || y + dy >= h - 2
                || !occupied.insert((dx, dy))
            {
                return Err("invalid camera cluster member or context".into());
            }
            mask[((y + dy - top) * pw + x + dx - left) as usize] &= !layers;
        }
        let mut ordered = Vec::new();
        for yy in 0..ph {
            for xx in 0..pw {
                let q = yy * pw + xx;
                if mask[q as usize] == 7 {
                    continue;
                }
                let mut quality = 0;
                if xx >= 2 && yy >= 2 && xx + 2 < pw && yy + 2 < ph {
                    for cy in yy - 2..=yy + 2 {
                        for cx in xx - 2..=xx + 2 {
                            quality += mask[(cy * pw + cx) as usize].count_ones();
                        }
                    }
                }
                ordered.push([(top + yy) * w + left + xx, q, quality, 0]);
            }
        }
        ordered.sort_by_key(|m| std::cmp::Reverse(m[2]));
        p.clusters.push(Cluster {
            rect: [left, top, pw, ph],
            indices: [
                p.members.len() as u32,
                ordered.len() as u32,
                p.masks.len() as u32,
                0,
            ],
        });
        p.members.extend(ordered);
        p.masks
            .extend(mask.into_iter().map(|m| m | if m == 7 { 256 } else { 0 }));
    }
    Ok(())
}

pub(crate) fn apply(
    gpu: &Gpu,
    original: &wgpu::Buffer,
    input: &wgpu::Buffer,
    p: &Prepared,
    profile: &mut Profile,
) -> Result<wgpu::Buffer> {
    let output = gpu.storage("Complete camera repaired layers", input.size())?;
    gpu.copy(input, &output, input.size());
    let records = gpu.upload("Main JPEG and detected repair records", &p.records)?;
    let uniform = gpu.uniform(&p.params);
    let mut trace = gpu.trace(profile.enabled, 2);
    gpu.dispatch(
        Kernel::CameraRepair,
        &[(0, original), (1, &records), (2, &uniform), (3, &output)],
        [1, 1],
        &mut trace,
    );
    if !p.clusters.is_empty() {
        let clusters = gpu.upload("Camera cluster descriptors", &p.clusters)?;
        let members = gpu.upload("Stable quality ordered cluster members", &p.members)?;
        let masks = gpu.upload("Cluster known and channel masks", &p.masks)?;
        let uniform = gpu.uniform(&[
            p.params.source[0],
            p.params.source[1],
            p.clusters.len() as u32,
            0,
        ]);
        gpu.dispatch(
            Kernel::ClusterRepair,
            &[
                (0, &output),
                (1, &clusters),
                (2, &members),
                (3, &masks),
                (4, &uniform),
            ],
            [1, 1],
            &mut trace,
        );
    }
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "camera_repair_test.rs"]
mod tests;
