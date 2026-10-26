//! Applies shared contrast and saturation to normalized photographic RGB.

use super::{validate_geometry, Params};
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    source::Result,
};

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
    let mut output = gpu.storage("finish.tone", count * 12)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    let uniform = gpu.uniform(&p);
    gpu.dispatch(
        Kernel::PhotographicTone,
        &mut [
            (0, Read(source)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
        ],
        groups,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
