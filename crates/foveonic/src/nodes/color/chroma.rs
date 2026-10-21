//! Applies signed-code chroma smoothing with caller-selected support and tapered correction tables.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    source::Result,
};

/// The numerical support is independent of camera identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Support {
    Binomial3,
    Box5,
    /// Quarter-resolution recursive support with a five-pixel decay in both axes.
    Recursive,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    /// Width, height, support selector, per-channel lookup length.
    pub shape: [u32; 4],
    /// Brightness regularizer, common-channel right shift, then reserved words.
    pub correction: [u32; 4],
    /// Horizontal alpha/beta and vertical alpha/beta in Q8.
    pub decay: [i32; 4],
    /// Coarse vertical alpha/beta in Q8, then reserved words.
    pub coarse_decay: [i32; 4],
}
#[derive(Clone)]
pub struct Prepared {
    pub params: Params,
    pub tables: Vec<i32>,
}
impl Prepared {
    pub(crate) fn new(
        extent: [u32; 2],
        amplitude: [f64; 3],
        slope: [f64; 3],
        regularizer: u32,
        shift: u32,
        support: Support,
    ) -> Result<Self> {
        if extent.contains(&0)
            || amplitude
                .iter()
                .chain(&slope)
                .any(|v| !v.is_finite() || *v <= 0.)
            || shift > 15
            || regularizer > 65535
        {
            return Err("invalid signed chroma parameters".into());
        }
        let frequency = 0.25
            / amplitude
                .iter()
                .zip(slope)
                .map(|(a, b)| a / b)
                .fold(0_f64, f64::max);
        // Recovered full table phase support, distinct from a rounded 2*pi.
        const TABLE_PHASE_SUPPORT: f64 = 6.2853;
        let requested = (TABLE_PHASE_SUPPORT / frequency) as usize;
        let length = [512, 1024, 2048, 4096]
            .into_iter()
            .find(|n| requested <= *n)
            .unwrap_or(8192);
        let mut tables = vec![0; length * 3];
        for c in 0..3 {
            for i in 0..length / 2 {
                let angle = frequency * i as f64;
                // The reference window cutoff is the literal 3.1416, rather than pi.
                let window = if angle < f64::from_bits(0x4009_21ff_2e48_e8a7) {
                    (angle.cos() + 1.) * 0.5
                } else {
                    0.
                };
                tables[c * length + i] =
                    ((slope[c] * i as f64 / amplitude[c]).tanh() * amplitude[c] * window + 0.5)
                        as i32;
            }
            for i in 1..length / 2 {
                tables[c * length + length - i] = -tables[c * length + i];
            }
            tables[c * length + length / 2] = tables[c * length + length / 2 + 1];
        }
        Ok(Self {
            params: Params {
                shape: [
                    extent[0],
                    extent[1],
                    match support {
                        Support::Binomial3 => 0,
                        Support::Box5 => 1,
                        Support::Recursive => 2,
                    },
                    length as u32,
                ],
                correction: [regularizer, shift, 0, 0],
                // Each recurrence coefficient is independently rounded to Q8.
                decay: {
                    const DECAY_LENGTH: f64 = 5.;
                    let beta = (-1. / DECAY_LENGTH).exp();
                    let a = ((1. - beta) * 256. + 0.5) as i32;
                    let b = (beta * 256. + 0.5) as i32;
                    [a, 256 - a, a, b]
                },
                coarse_decay: {
                    const DECAY_LENGTH: f64 = 5.;
                    let beta = (-4. / DECAY_LENGTH).exp();
                    [
                        ((1. - beta) * 256. + 0.5) as i32,
                        (beta * 256. + 0.5) as i32,
                        0,
                        0,
                    ]
                },
            },
            tables,
        })
    }
    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::pod(&self.params).slice(&self.tables)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    p: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h, support, _] = p.params.shape;
    if support == 2 {
        return apply_recursive(gpu, input, p, profile);
    }
    let size = u64::from(w) * u64::from(h) * 12;
    let mut horizontal = gpu.storage("Signed chroma horizontal", size)?;
    let mut output = gpu.storage("Signed chroma output", size)?;
    let uniform = gpu.uniform(&p.params);
    let tables = gpu.upload("Tapered signed chroma correction", &p.tables)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    gpu.dispatch(
        Kernel::ChromaHorizontal,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(horizontal.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ChromaApply,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&horizontal)),
            (3, Read(&tables)),
            (4, Write(output.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

fn apply_recursive(
    gpu: &Gpu,
    input: &ReadGuard,
    p: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height, _, _] = p.params.shape;
    let coarse_width = width.div_ceil(4);
    let coarse_height = height.div_ceil(4);
    let mut coarse = gpu.storage(
        "Recursive chroma coarse",
        u64::from(coarse_width) * u64::from(coarse_height) * 12,
    )?;
    let mut horizontal = gpu.storage(
        "Recursive chroma horizontal",
        u64::from(width) * u64::from(coarse_height) * 12,
    )?;
    let mut output = gpu.storage(
        "Recursive chroma output",
        u64::from(width) * u64::from(height) * 12,
    )?;
    let uniform = gpu.uniform(&p.params);
    let tables = gpu.upload("Recursive chroma lookup", &p.tables)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 3);
    gpu.dispatch(
        Kernel::ChromaRecursiveInit,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(coarse.write())),
        ],
        [coarse_width.div_ceil(64), 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ChromaRecursiveHorizontal,
        &mut [
            (1, Read(&uniform)),
            (2, Read(&coarse)),
            (5, Write(horizontal.write())),
        ],
        [coarse_height.div_ceil(64), 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ChromaRecursiveApply,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (3, Read(&tables)),
            (4, Write(output.write())),
            (5, Read(&horizontal)),
        ],
        [width.div_ceil(64), 1],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "chroma_test.rs"]
mod tests;
