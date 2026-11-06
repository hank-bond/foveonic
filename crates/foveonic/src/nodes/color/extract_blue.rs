//! Copies the blue component of immutable RGB codes into a same-size scalar plane.

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
    input: &ReadGuard,
    extent: [u32; 2],
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h] = extent;
    let bytes = u64::from(w) * u64::from(h) * 4;
    if extent.contains(&0) || input.size() != bytes * 3 {
        return Err("blue-channel extraction input geometry mismatch".into());
    }
    let uniform = gpu.uniform(&[w, h, 0, 0]);
    let mut blue = gpu.storage("Extracted blue channel", bytes)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::ExtractBlue,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(blue.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(blue)
}
