//! Adjusts six log-brightness texture bands without coupling noise or sharpening.

use super::{validate_geometry, Params};
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    source::Result,
};

pub(crate) const LEVELS: u32 = 6;
pub(crate) const SUPPORT: u32 = 2 * ((1 << LEVELS) - 1);
// One base and one accumulated exposure correction per pixel.
const STATE_BYTES: u64 = size_of::<[f32; 2]>() as u64;

pub(crate) fn apply(
    gpu: &Gpu,
    source: &ReadGuard,
    mut p: Params,
    extent: [u32; 2],
    profile: &mut Profile,
) -> Result<ReadGuard> {
    validate_geometry(source, extent)?;
    p.shape[..2].copy_from_slice(&extent);
    let count = u64::from(extent[0]) * u64::from(extent[1]);
    let groups = [extent[0].div_ceil(8), extent[1].div_ceil(8)];
    let mut output = gpu.storage("finish.local-contrast", count * 12)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 2 * LEVELS + 2);

    let mut state = gpu.storage("Local contrast brightness", count * STATE_BYTES)?;
    let uniform = gpu.uniform(&p);
    gpu.dispatch(
        Kernel::LocalContrastExtract,
        &mut [
            (0, Read(source)),
            (1, Read(&uniform)),
            (7, Write(state.write())),
        ],
        groups,
        &mut trace,
    );
    let amount = p.values[0];
    let gains = super::super::wavelet_contrast::photographic_gains();
    // The native table runs from coarsest to finest. This undecimated
    // decomposition builds outward from the finest band instead.
    for (level, gain) in gains.into_iter().rev().enumerate() {
        let mut horizontal = gpu.storage("Local contrast horizontal", count * STATE_BYTES)?;
        let mut next = gpu.storage("Local contrast base", count * STATE_BYTES)?;
        p.shape[2] = ((1u32 << level) as f32 * p.values[1]).round().max(1.) as u32;
        p.shape[3] = 0;
        p.values[0] = (gain - 1.) * amount;
        let uniform = gpu.uniform(&p);
        gpu.dispatch(
            Kernel::LocalContrastBlur,
            &mut [
                (1, Read(&uniform)),
                (8, Read(&state)),
                (6, Read(&state)),
                (7, Write(horizontal.write())),
            ],
            groups,
            &mut trace,
        );
        p.shape[3] = 1;
        let uniform = gpu.uniform(&p);
        gpu.dispatch(
            Kernel::LocalContrastBlur,
            &mut [
                (1, Read(&uniform)),
                (8, Read(&state)),
                (6, Read(&horizontal)),
                (7, Write(next.write())),
            ],
            groups,
            &mut trace,
        );
        state = next;
    }
    let uniform = gpu.uniform(&p);
    gpu.dispatch(
        Kernel::LocalContrastApply,
        &mut [
            (0, Read(source)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
            (6, Read(&state)),
        ],
        groups,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
