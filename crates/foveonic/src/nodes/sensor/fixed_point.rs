//! Executes signed fixed-point layer correction and nonlinear matrix transforms with explicit numerical contracts.

use crate::source::Result;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CorrectionParams {
    /// Width, height, gain-knot count, Q14 input multiplier (zero means unchanged).
    pub source: [u32; 4],
    /// Per-channel fractions of the sparse image mean to subtract, before gain.
    pub flare: [f32; 4],
    /// Per-channel multipliers combined with interpolated gain knots.
    pub final_gain: [f32; 4],
    /// Q14 coefficients multiplying the difference from the previous column.
    pub linear: [i32; 4],
    /// Q14 coefficients multiplying the squared column difference shifted down by 14.
    pub quadratic: [i32; 4],
    /// Q14 coefficients multiplying the previous sample at each segment boundary.
    pub segment_scale: [i32; 4],
    /// Additive segment-boundary offsets in sample-code units multiplied by 2^14.
    pub segment_bias: [i32; 4],
    /// Segment period in columns (zero or one disables correction), then reserved.
    pub controls: [u32; 4],
}

pub struct Correction {
    pub params: CorrectionParams,
    /// Row-major Float32 gain knots, vertically interpolated on the host.
    pub knots: Vec<f32>,
}
impl Correction {
    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::pod(&self.params).slice(&self.knots)
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TransformParams {
    /// Width, height, table extent, integer matrix shift.
    pub source: [u32; 4],
    /// Three RGB rows scaled by 2^source[3]. The fourth column is unused.
    pub matrix: [[i32; 4]; 3],
    /// Signed output lower and upper bounds.
    pub bounds: [i32; 4],
}
pub struct Transform {
    pub params: TransformParams,
    /// Three channel-major signed-code correction tables, each source[2] entries.
    pub tables: Vec<i32>,
}
impl Transform {
    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::pod(&self.params).slice(&self.tables)
    }
}

pub(crate) fn apply_correction(
    gpu: &crate::gpu::Gpu,
    input: &crate::gpu::ReadGuard,
    prepared: &Correction,
    profile: &mut crate::profile::Profile,
) -> Result<crate::gpu::ReadGuard> {
    use crate::gpu::{
        Binding::{Read, Write},
        Kernel,
    };
    let [width, height, _, _] = prepared.params.source;
    let bytes = u64::from(width) * u64::from(height) * 12;
    let columns = prepared.params.source[2];
    if width < 3
        || height < 3
        || columns < 2
        || prepared.knots.len() as u64 != u64::from(height) * u64::from(columns) * 3
        || input.size() < bytes
    {
        return Err(
            "fixed-point correction input or gain-knot geometry does not match its contract".into(),
        );
    }
    let uniform = gpu.uniform(&prepared.params);
    let knots = gpu.upload("Correction gain knots", &prepared.knots)?;
    let scaling = prepared.params.source[3] != 0;
    let mut offsets = gpu.storage("Sparse sampled flare", 12)?;
    let mut output = gpu.storage("Fixed-point corrected layers", bytes)?;
    let groups = [width.div_ceil(8), height.div_ceil(8)];
    let mut trace = gpu.trace(profile.mode.gpu(), if scaling { 3 } else { 2 });
    // Original sensor samples are integral codes. A disabled input multiplier
    // leaves them unchanged, so preserve the immutable input rather than copy it.
    let scaled = if scaling {
        let mut scaled = gpu.storage("Integer sample scaling", bytes)?;
        gpu.dispatch(
            Kernel::FixedPointScale,
            &mut [
                (0, Read(input)),
                (3, Read(&uniform)),
                (4, Write(scaled.write())),
            ],
            groups,
            &mut trace,
        );
        scaled
    } else {
        input.clone()
    };
    gpu.dispatch(
        Kernel::FixedPointStatistics,
        &mut [
            (0, Read(&scaled)),
            (3, Read(&uniform)),
            (5, Write(offsets.write())),
        ],
        [3, 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::FixedPointCorrection,
        &mut [
            (0, Read(&scaled)),
            (1, Read(&offsets)),
            (2, Read(&knots)),
            (3, Read(&uniform)),
            (4, Write(output.write())),
        ],
        [height.div_ceil(64), 1],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

pub(crate) fn apply_transform(
    gpu: &crate::gpu::Gpu,
    input: &crate::gpu::ReadGuard,
    prepared: &Transform,
    profile: &mut crate::profile::Profile,
) -> Result<(crate::gpu::ReadGuard, crate::gpu::ReadGuard)> {
    use crate::gpu::{
        Binding::{Read, Write},
        Kernel,
    };
    let [width, height, extent, shift] = prepared.params.source;
    if width == 0
        || height == 0
        || extent < 2
        || !extent.is_power_of_two()
        || !(1..=31).contains(&shift)
        || prepared.tables.len() as u64 != u64::from(extent) * 3
        || input.size() < u64::from(width) * u64::from(height) * 12
        || prepared.params.bounds[0] > prepared.params.bounds[1]
    {
        return Err(
            "fixed-point color input or lookup geometry does not match its contract".into(),
        );
    }
    let uniform = gpu.uniform(&prepared.params);
    let tables = gpu.upload("Signed nonlinear channel tables", &prepared.tables)?;
    let mut output = gpu.storage(
        "Fixed-point working color",
        u64::from(width) * u64::from(height) * 12,
    )?;
    let mut layers = gpu.storage(
        "Nonlinear corrected camera layers",
        u64::from(width) * u64::from(height) * 12,
    )?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::FixedPointColor,
        &mut [
            (0, Read(input)),
            (1, Read(&tables)),
            (2, Read(&uniform)),
            (3, Write(output.write())),
            (4, Write(layers.write())),
        ],
        [width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok((output, layers))
}

#[cfg(test)]
#[path = "fixed_point_test.rs"]
mod tests;
