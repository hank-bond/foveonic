//! Maps declared linear RGB codes into another linear sample basis without tone or clipping.
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
    rows: [[f32; 4]; 3],
    extent: [u32; 2],
    white: u32,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Params {
        shape: [u32; 4],
        rows: [[f32; 4]; 3],
        scale: [f32; 4],
    }
    let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 12;
    if extent.contains(&0)
        || white == 0
        || input.size() != bytes
        || rows.iter().flatten().any(|v| !v.is_finite())
    {
        return Err(
            "linear basis mapping needs finite coefficients and matching RGB geometry".into(),
        );
    }
    let parameters = gpu.uniform(&Params {
        shape: [extent[0], extent[1], 0, 0],
        rows,
        scale: [1. / white as f32, 0., 0., 0.],
    });
    let mut output = gpu.storage("Mapped linear RGB basis", bytes)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::LinearBasis,
        &mut [
            (0, Read(input)),
            (1, Read(&parameters)),
            (2, Write(output.write())),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
