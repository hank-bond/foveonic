//! Prepares camera resampling weights and leaves image calculations on the GPU.

use crate::gpu::Binding::{Read, Write};
// Adapted with owner permission from Foveonic filtering/resample{,-params}.rs at e47af35.
use crate::{
    gpu::{Gpu, Kernel},
    profile::Profile,
    source::Result,
};
use bytemuck::{Pod, Zeroable};
#[cfg(test)]
#[path = "resample_test.rs"]
mod tests;
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    source: [u32; 4],
    output: [u32; 4],
    advance: [i32; 4],
}
pub(crate) struct Prepared {
    params: Params,
    weights: Vec<f32>,
    starts: Vec<i32>,
}
impl Prepared {
    pub(crate) fn new(source: [u32; 2], target: [u32; 2], tiles: u32, scale: f32) -> Result<Self> {
        if !(0.375..=3.).contains(&scale)
            || source.contains(&0)
            || target[0] == 0
            || target[1] < 4
            || tiles == 0
            || source
                .iter()
                .chain(&target)
                .any(|&v| v > i32::MAX as u32 / 8)
        {
            return Err("invalid polyphase resampling geometry or scale".into());
        }
        let span = if scale < 1. { 2_f32 / scale } else { 2. };
        let tolerance = f32::from_bits(0x3a83126f);
        let taps = (span.ceil() + 2.) as usize;
        let offset = 1_f64 - 1. / f64::from(scale);
        let mut starts = [0_i32; 64];
        let mut rows = vec![vec![0_f32; taps]; 64];
        let mut columns = vec![0_f32; taps];
        for i in 0..64 {
            let center = 0.5_f64.mul_add(offset, f64::from((i + 1) as f32 / scale)) as f32;
            starts[i] = (center - span * 0.5).floor() as i32;
            let mut sum = 0_f32;
            for (k, value) in rows[i].iter_mut().enumerate() {
                let distance = (center - (starts[i] + k as i32) as f32).abs();
                *value = if scale < 1. {
                    ((1. - f64::from((distance * scale).clamp(0., 1.))) * f64::from(scale)) as f32
                } else {
                    (1. - distance).max(0.)
                };
                sum += *value;
            }
            for (value, column) in rows[i].iter_mut().zip(&mut columns) {
                *value /= sum;
                *column += *value;
            }
        }
        let lo = columns
            .iter()
            .position(|&v| v != 0.)
            .ok_or("empty resampling coefficients")?;
        let hi = columns.iter().rposition(|&v| v != 0.).unwrap();
        let first = rows[0][lo];
        let phases = (1..64)
            .find(|&i| {
                let v = rows[i][lo];
                (first == 0. && v == 0.) || (v - first).abs() / v.abs().max(first.abs()) < tolerance
            })
            .unwrap_or(64);
        if phases >= 64 || !target[0].is_multiple_of(phases as u32) {
            return Err("unsupported native resampling phase geometry".into());
        }
        let advance = starts[phases] - starts[0];
        let position = |n: u32| {
            i64::from(starts[n as usize % phases])
                + i64::from(n / phases as u32) * i64::from(advance)
        };
        // Native addresses complete rows linearly: right taps may spill into the following row.
        if target[1] > 4 {
            let x = (position(target[0] - 1) + (hi - lo) as i64).max(0);
            let y = (position(target[1] - 5) + (hi - lo) as i64).max(0);
            if y * i64::from(source[0]) + x >= i64::from(source[0]) * i64::from(source[1]) {
                return Err("native resampling taps outside source storage".into());
            }
        }
        let weights = rows[..phases]
            .iter()
            .flat_map(|r| r[lo..=hi].iter().copied())
            .collect();
        Ok(Self {
            params: Params {
                source: [source[0], source[1], tiles, 0],
                output: [target[0], target[1], (hi - lo + 1) as u32, phases as u32],
                advance: [advance, 0, 0, 0],
            },
            weights,
            starts: starts[..phases].to_vec(),
        })
    }
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &crate::gpu::ReadGuard,
    p: &Prepared,
    profile: &mut Profile,
) -> Result<crate::gpu::ReadGuard> {
    apply_components(gpu, input, p, 3, profile)
}
pub(crate) fn apply_scalar(
    gpu: &Gpu,
    input: &crate::gpu::ReadGuard,
    p: &Prepared,
    profile: &mut Profile,
) -> Result<crate::gpu::ReadGuard> {
    apply_components(gpu, input, p, 1, profile)
}
fn apply_components(
    gpu: &Gpu,
    input: &crate::gpu::ReadGuard,
    p: &Prepared,
    components: u32,
    profile: &mut Profile,
) -> Result<crate::gpu::ReadGuard> {
    let [w, h, tiles, _] = p.params.source;
    let [ow, oh, _, _] = p.params.output;
    if input.size() != u64::from(w) * u64::from(h) * u64::from(tiles) * u64::from(components) * 4 {
        return Err("polyphase input extent mismatch".into());
    }
    let mut output = gpu.storage(
        "Native polyphase image",
        u64::from(ow) * u64::from(oh) * u64::from(tiles) * u64::from(components) * 4,
    )?;
    let mut geometry = p.params;
    geometry.source[3] = components;
    let params = gpu.uniform(&geometry);
    let weights = gpu.upload("Native polyphase coefficients", &p.weights)?;
    let starts = gpu.upload("Native polyphase source starts", &p.starts)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::Resample,
        &mut [
            (0, Read(input)),
            (1, Read(&weights)),
            (2, Read(&params)),
            (3, Write(output.write())),
            (4, Read(&starts)),
        ],
        [ow.div_ceil(8), (oh * tiles).div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
