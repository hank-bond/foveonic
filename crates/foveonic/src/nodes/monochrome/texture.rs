//! Restores Fine and Coarse monochrome texture independently of noise cleanup.
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    nodes::color::texture::{FINE_BOUNDARY_STEP, NATIVE_STEPS},
    noise::TextureControls,
    profile::Profile,
    source::Result,
};
/// Scalar texture uniforms. Size is expressed in native sensor pixels.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    /// Width, height, reserved and blur axis.
    shape: [u32; 4],
    /// Fine amount, coarse amount, binomial step and reserved.
    values: [f32; 4],
}
fn pass(
    gpu: &Gpu,
    kernel: Kernel,
    inputs: &[&ReadGuard],
    p: Params,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let bytes = u64::from(p.shape[0]) * u64::from(p.shape[1]) * 4;
    if inputs.iter().any(|b| b.size() != bytes) {
        return Err("mono texture geometry mismatch".into());
    }
    let mut output = gpu.storage(kernel.label(), bytes)?;
    let uniform = gpu.uniform(&p);
    let mut bindings = vec![
        (0, Read(inputs[0])),
        (1, Read(&uniform)),
        (2, Write(output.write())),
    ];
    for (i, v) in inputs.iter().enumerate().skip(1) {
        bindings.push((i as u32 + 2, Read(v)));
    }
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        kernel,
        &mut bindings,
        [p.shape[0].div_ceil(8), p.shape[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    controls: TextureControls,
    extent: [u32; 2],
    profile: &mut Profile,
) -> Result<ReadGuard> {
    if controls.is_neutral() {
        return Ok(input.clone());
    }
    crate::noise::Controls {
        texture: controls,
        ..Default::default()
    }
    .validate()?;
    let mut p = Params {
        shape: [extent[0], extent[1], 0, 0],
        values: [controls.fine, controls.coarse, 0., 0.],
    };
    let log = pass(gpu, Kernel::MonoTextureExtract, &[input], p, profile)?;
    let mut current = log.clone();
    let mut fine = log.clone();
    for (index, step) in NATIVE_STEPS.into_iter().enumerate() {
        p.values[2] = step as f32
            * if index <= FINE_BOUNDARY_STEP {
                controls.fine_size
            } else {
                controls.coarse_size
            };
        for axis in 0..2 {
            p.shape[3] = axis;
            current = pass(gpu, Kernel::MonoTextureBlur, &[&current], p, profile)?;
        }
        if index == FINE_BOUNDARY_STEP {
            fine = current.clone();
        }
    }
    pass(
        gpu,
        Kernel::MonoTextureApply,
        &[input, &log, &fine, &current],
        p,
        profile,
    )
}
