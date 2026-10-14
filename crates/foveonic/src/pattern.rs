//! Prepares repeatable pattern-correction geometry without moving photo work to the CPU.

//!
//! Adapted with owner permission from Foveonic noise.rs and filtering/pattern.rs
//! at e47af35. Photographic detection, ordering, and swaps execute on GPU.
use crate::gpu::Binding::{Read, Write};
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
    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::pod(&self.params).slice(&self.offsets)
    }
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
    pub ratio: Option<crate::gpu::ReadGuard>,
    pub mask: Option<crate::gpu::ReadGuard>,
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &crate::gpu::ReadGuard,
    edges: &crate::gpu::ReadGuard,
    p: &Prepared,
    want_ratio: bool,
    want_mask: bool,
    profile: &mut Profile,
) -> Result<Output> {
    let [w, h, tiles, _] = p.params.shape;
    let n = u64::from(w) * u64::from(h) * u64::from(tiles);
    let mut masks = gpu.storage("Native pattern selection", n * 4)?;
    let uniform = gpu.uniform(&p.params);
    let groups = [w.div_ceil(8), (h * tiles).div_ceil(8)];
    let mut trace = gpu.trace(profile.mode.gpu(), 7);
    gpu.dispatch(
        Kernel::PatternMask,
        &mut [
            (0, Read(input)),
            (1, Read(edges)),
            (2, Read(&uniform)),
            (3, Write(masks.write())),
        ],
        groups,
        &mut trace,
    );
    let ratio = if want_ratio {
        let mut counts = gpu.storage(
            "Ordered pattern line prefixes",
            u64::from(w + h) * u64::from(tiles) * 4,
        )?;
        let offsets = gpu.upload("Deterministic pattern offsets", &p.offsets)?;
        let mut permuted = gpu.storage("Owned pattern permutation", n * 4)?;
        gpu.dispatch(
            Kernel::PatternCounts,
            &mut [
                (2, Read(&uniform)),
                (3, Read(&masks)),
                (4, Write(counts.write())),
            ],
            [w.max(h).div_ceil(64), tiles],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::PatternPrefix,
            &mut [(2, Read(&uniform)), (4, Write(counts.write()))],
            [tiles, 1],
            &mut trace,
        );
        gpu.copy(input, permuted.write(), n * 4);
        gpu.dispatch(
            Kernel::PatternHorizontal,
            &mut [
                (2, Read(&uniform)),
                (3, Read(&masks)),
                (4, Read(&counts)),
                (6, Read(&offsets)),
                (7, Write(permuted.write())),
            ],
            [(h * tiles).div_ceil(64), 1],
            &mut trace,
        );
        gpu.dispatch(
            Kernel::PatternVertical,
            &mut [
                (2, Read(&uniform)),
                (3, Read(&masks)),
                (4, Read(&counts)),
                (6, Read(&offsets)),
                (7, Write(permuted.write())),
            ],
            [(w * tiles).div_ceil(64), 1],
            &mut trace,
        );
        let mut output = gpu.storage("Pattern conditioned ratios", n * 4)?;
        gpu.dispatch(
            Kernel::PatternBlend,
            &mut [
                (0, Read(input)),
                (2, Read(&uniform)),
                (7, Read(&permuted)),
                (8, Write(output.write())),
            ],
            groups,
            &mut trace,
        );
        Some(output)
    } else {
        None
    };
    let mask = if want_mask {
        let mut output = gpu.storage("Projected binary pattern mask", n * 4)?;
        gpu.dispatch(
            Kernel::PatternMaskImage,
            &mut [
                (2, Read(&uniform)),
                (3, Read(&masks)),
                (8, Write(output.write())),
            ],
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
