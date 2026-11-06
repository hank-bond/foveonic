//! Pools corrected numerical signals while preserving categorical clipping provenance.

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
pub(crate) enum Signal {
    Rgb,
    Blue,
    Mask,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Prepared {
    shape: [u32; 4],
    controls: [u32; 4],
}
impl Prepared {
    /// Complete nonoverlapping cells retain their physical center. Masks use
    /// maximum coverage, so a clipped source never becomes an unmarked donor.
    pub(crate) fn new(extent: [u32; 2], divisor: u32, signal: Signal) -> Result<Self> {
        if ![2, 4, 8].contains(&divisor)
            || extent.iter().any(|v| *v == 0 || !v.is_multiple_of(divisor))
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
        {
            return Err("invalid corrected-signal pooling geometry".into());
        }
        Ok(Self {
            shape: [
                extent[0],
                extent[1],
                extent[0] / divisor,
                extent[1] / divisor,
            ],
            controls: [
                divisor,
                match signal {
                    Signal::Rgb => 3,
                    _ => 1,
                },
                u32::from(matches!(signal, Signal::Mask)),
                0,
            ],
        })
    }
    pub(crate) fn output_extent(&self) -> [u32; 2] {
        [self.shape[2], self.shape[3]]
    }
    pub(crate) fn signal(&self) -> Signal {
        if self.controls[2] != 0 {
            Signal::Mask
        } else if self.controls[1] == 3 {
            Signal::Rgb
        } else {
            Signal::Blue
        }
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(self)
    }
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let pixel_bytes = u64::from(prepared.controls[1]) * 4;
    if input.size() != u64::from(prepared.shape[0]) * u64::from(prepared.shape[1]) * pixel_bytes {
        return Err("corrected-signal pooling input geometry mismatch".into());
    }
    let [w, h] = prepared.output_extent();
    let mut output = gpu.storage(
        "Pooled corrected signal",
        u64::from(w) * u64::from(h) * pixel_bytes,
    )?;
    let uniform = gpu.uniform(prepared);
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::PoolSignal,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "pool_test.rs"]
mod tests;
