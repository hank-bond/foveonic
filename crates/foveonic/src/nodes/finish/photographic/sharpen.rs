//! Sharpens square-root brightness with independent threshold and halo restraint.

use super::{validate_geometry, Params, GAUSSIAN_SUPPORT_SIGMAS};
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
    let mut output = gpu.storage("Universal sharpening", count * 12)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 4);

    let mut signal = gpu.storage("Sharpen brightness", count * 4)?;
    let uniform = gpu.uniform(&p);
    gpu.dispatch(
        Kernel::UniversalSharpExtract,
        &mut [
            (0, Read(source)),
            (1, Read(&uniform)),
            (5, Write(signal.write())),
        ],
        groups,
        &mut trace,
    );
    let mut horizontal = gpu.storage("Sharpen horizontal base", count * 4)?;
    let mut base = gpu.storage("Sharpen base", count * 4)?;
    let mut blur = p;
    blur.shape[2] = (GAUSSIAN_SUPPORT_SIGMAS * p.values[3]).ceil() as u32;
    blur.values[0] = p.values[3];
    let uniform = gpu.uniform(&blur);
    gpu.dispatch(
        Kernel::UniversalSharpBlur,
        &mut [
            (1, Read(&uniform)),
            (4, Read(&signal)),
            (5, Write(horizontal.write())),
        ],
        groups,
        &mut trace,
    );
    blur.shape[3] = 1;
    let uniform = gpu.uniform(&blur);
    gpu.dispatch(
        Kernel::UniversalSharpBlur,
        &mut [
            (1, Read(&uniform)),
            (4, Read(&horizontal)),
            (5, Write(base.write())),
        ],
        groups,
        &mut trace,
    );
    let uniform = gpu.uniform(&p);
    gpu.dispatch(
        Kernel::UniversalSharpen,
        &mut [
            (0, Read(source)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
            (4, Read(&base)),
        ],
        groups,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
