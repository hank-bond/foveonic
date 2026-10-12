//! Metadata/geometry-only setup for native deterministic luminance-ratio pattern conditioning.
//!
//! Adapted with owner permission from Foveonic noise.rs and filtering/pattern.rs
//! at e47af35. Photographic detection, ordering, and swaps execute on GPU.
#[cfg(test)]
#[path = "pattern_test.rs"]
mod tests;

use crate::{
    gpu::{Gpu, Kernel},
    profile::Profile,
    source::Result,
};
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    pub shape: [u32; 4],
    pub thresholds: [f32; 4],
}
#[derive(Clone)]
pub(crate) struct Prepared {
    pub params: Params,
    pub offsets: Vec<u32>,
}
impl Prepared {
    pub(crate) fn new(shape: [u32; 4], thresholds: [f32; 4]) -> Result<Self> {
        if shape[..3].contains(&0)
            || shape[3] > 4
            || thresholds.iter().any(|v| !v.is_finite())
            || thresholds[0] < 0.
            || thresholds[1] < 0.
            || !(0. ..=1.).contains(&thresholds[2])
        {
            return Err("invalid pattern geometry or thresholds".into());
        }
        let span = shape[3] * 2 + 1;
        let (mut x, mut y, mut z, mut w) =
            (123456789_u32, 362436069_u32, 521288629_u32, 88675123_u32);
        let offsets = (0..span * 512)
            .map(|_| {
                let t = x ^ (x << 11);
                (x, y, z) = (y, z, w);
                w = w ^ (w >> 19) ^ t ^ (t >> 8);
                w % span
            })
            .collect();
        Ok(Self {
            params: Params { shape, thresholds },
            offsets,
        })
    }
}
pub(crate) struct Output {
    pub ratio: Option<wgpu::Buffer>,
    pub mask: Option<wgpu::Buffer>,
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &wgpu::Buffer,
    edges: &wgpu::Buffer,
    p: &Prepared,
    want_ratio: bool,
    want_mask: bool,
    profile: &mut Profile,
) -> Result<Output> {
    let [w, h, tiles, _] = p.params.shape;
    let n = u64::from(w) * u64::from(h) * u64::from(tiles);
    let masks = gpu.storage("Native pattern selection", n * 4)?;
    let uniform = gpu.uniform(&p.params);
    let groups = [w.div_ceil(8), (h * tiles).div_ceil(8)];
    let mut trace = gpu.trace(profile.enabled, 7);
    gpu.dispatch(
        Kernel::PatternMask,
        &[(0, input), (1, edges), (2, &uniform), (3, &masks)],
        groups,
        &mut trace,
    );
    let ratio = if want_ratio {
        let counts = gpu.storage(
            "Ordered pattern line prefixes",
            u64::from(w + h) * u64::from(tiles) * 4,
        )?;
        let offsets = gpu.upload("Deterministic pattern offsets", &p.offsets)?;
        let permuted = gpu.storage("Owned pattern permutation", n * 4)?;
        gpu.dispatch(
            Kernel::PatternCounts,
            &[(2, &uniform), (3, &masks), (4, &counts)],
            [w.max(h).div_ceil(64), tiles],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::PatternPrefix,
            &[(2, &uniform), (4, &counts)],
            [tiles, 1],
            &mut trace,
        );
        gpu.copy(input, &permuted, n * 4);
        gpu.dispatch(
            Kernel::PatternHorizontal,
            &[
                (2, &uniform),
                (3, &masks),
                (4, &counts),
                (6, &offsets),
                (7, &permuted),
            ],
            [(h * tiles).div_ceil(64), 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::PatternVertical,
            &[
                (2, &uniform),
                (3, &masks),
                (4, &counts),
                (6, &offsets),
                (7, &permuted),
            ],
            [(w * tiles).div_ceil(64), 1],
            &mut trace,
        );
        let output = gpu.storage("Pattern conditioned ratios", n * 4)?;
        gpu.dispatch(
            Kernel::PatternBlend,
            &[(0, input), (2, &uniform), (7, &permuted), (8, &output)],
            groups,
            &mut trace,
        );
        Some(output)
    } else {
        None
    };
    let mask = if want_mask {
        let output = gpu.storage("Projected binary pattern mask", n * 4)?;
        gpu.dispatch(
            Kernel::PatternMaskImage,
            &[(2, &uniform), (3, &masks), (8, &output)],
            groups,
            &mut trace,
        );
        Some(output)
    } else {
        None
    };
    gpu.finish_trace(&trace, profile)?;
    Ok(Output { ratio, mask })
}
