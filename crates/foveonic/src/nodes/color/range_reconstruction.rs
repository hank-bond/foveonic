//! Reconstructs linear RGB with range filtering, guided means, and deterministic residual diffusion.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    source::Result,
};

/// Functional controls supplied by the recipe, independent of camera identity.
#[derive(Clone, Debug)]
pub(crate) struct Controls {
    pub enabled: bool,
    pub downsample: u32,
    pub window: u32,
    pub subsample: bool,
    pub noise_scaling: f32,
    pub edge_strength: f32,
    pub local_decay: f32,
    pub local_iterations: u32,
    pub dark_low: f32,
    pub dark_high: f32,
    /// Signal-dependent diffusion, descriptor offset 0x64.
    pub diffusion_iterations: u32,
    /// Optional constant-threshold diffusion, descriptor offset 0x60.
    pub constant_iterations: u32,
    pub diffusion_k: f32,
    pub diffusion_step: f32,
    pub residual_strength: f32,
    pub residual_epsilon: f32,
    /// Constant and linear descriptor rows. The reference stores a third row,
    /// but its range and edge kernels read only these six coefficients.
    pub color_noise: [[f32; 3]; 2],
    /// The reference diffusion kernel reads only the first two descriptor
    /// coefficients, even when metadata supplies a nonzero third value.
    pub intensity_noise: [f32; 2],
    pub luma_weights: [f32; 3],
    /// White-normalized camera-to-working matrix, including output gain.
    pub matrix: [f64; 9],
    pub channel_noise: [f64; 3],
    pub digital_gain: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    shape: [u32; 4],
    filter: [u32; 4],
    noise_a: [f32; 4],
    noise_b: [f32; 4],
    guide: [f32; 4],
    residual: [f32; 4],
    weights: [f32; 4],
    matrix: [[f32; 4]; 3],
    channel_noise: [f32; 4],
}

#[derive(Clone)]
pub struct Prepared {
    params: Params,
    tables: Vec<i32>,
    iterations: [u32; 3],
}
impl Prepared {
    pub(crate) fn new(extent: [u32; 2], c: Controls) -> Result<Self> {
        if extent.iter().any(|n| *n < 8)
            || ![1, 2, 4].contains(&c.downsample)
            || c.window < 3
            || c.window > 31
            || c.window.is_multiple_of(2)
            || c.local_iterations > 16
            || c.diffusion_iterations > 32
            || c.constant_iterations > 32
            || !(0.0..1.0).contains(&c.local_decay)
            || c.dark_high <= c.dark_low
            || c.diffusion_k <= 0.0
            || c.residual_epsilon <= 0.0
            || c.channel_noise.iter().any(|x| !x.is_finite() || *x <= 0.0)
            || c.matrix.iter().any(|x| !x.is_finite())
        {
            return Err("invalid range reconstruction controls".into());
        }
        let amplitude = c
            .channel_noise
            .map(|x| x * f64::from(c.digital_gain.min(8.0)));
        let lookup = super::chroma::Prepared::new(
            extent,
            amplitude,
            [0.8; 3],
            0,
            0,
            super::chroma::Support::Binomial3,
        )?;
        let length = lookup.tables.len() / 3;
        let params = Params {
            shape: [
                extent[0],
                extent[1],
                extent[0] / c.downsample,
                extent[1] / c.downsample,
            ],
            filter: [
                c.downsample,
                c.window,
                u32::from(c.subsample),
                length as u32,
            ],
            noise_a: [
                c.color_noise[0][0],
                c.color_noise[0][1],
                c.color_noise[0][2],
                c.noise_scaling,
            ],
            noise_b: [
                c.color_noise[1][0],
                c.color_noise[1][1],
                c.color_noise[1][2],
                c.edge_strength,
            ],
            guide: [c.local_decay, c.dark_low, c.dark_high, c.diffusion_step],
            residual: [
                c.intensity_noise[0],
                c.intensity_noise[1],
                c.diffusion_k,
                c.residual_strength,
            ],
            weights: [
                c.luma_weights[0],
                c.luma_weights[1],
                c.luma_weights[2],
                c.residual_epsilon,
            ],
            matrix: std::array::from_fn(|r| {
                [
                    c.matrix[r * 3] as f32,
                    c.matrix[r * 3 + 1] as f32,
                    c.matrix[r * 3 + 2] as f32,
                    0.0,
                ]
            }),
            channel_noise: [
                c.channel_noise[0] as f32,
                c.channel_noise[1] as f32,
                c.channel_noise[2] as f32,
                f32::from(u8::from(c.enabled)),
            ],
        };
        let values: &[f32] = bytemuck::cast_slice(&bytemuck::bytes_of(&params)[32..]);
        if values.iter().any(|x| !x.is_finite())
            || !c.digital_gain.is_finite()
            || c.digital_gain <= 0.0
            || c.diffusion_step < 0.0
            || c.diffusion_step > 0.25
        {
            return Err("nonfinite or unstable range reconstruction controls".into());
        }
        Ok(Self {
            params,
            tables: lookup.tables,
            iterations: [
                c.local_iterations,
                c.diffusion_iterations,
                c.constant_iterations,
            ],
        })
    }
    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::pod(&self.params)
            .slice(&self.tables)
            .slice(&self.iterations)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    p: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h, rw, rh] = p.params.shape;
    let full = u64::from(w) * u64::from(h) * 16;

    if p.params.channel_noise[3] == 0.0 {
        let uniform = gpu.uniform(&p.params);
        let tables = gpu.upload("Floating color lookup", &p.tables)?;
        let black = gpu.upload("Floating color zero offset", &[0_f32; 4])?;
        let mut packed = gpu.storage("Floating color input", full)?;
        let mut output = gpu.storage("Floating color output", u64::from(w) * u64::from(h) * 12)?;
        let mut trace = gpu.trace(profile.mode.gpu(), 2);
        let grid = [w.div_ceil(8), h.div_ceil(8)];
        gpu.dispatch(
            Kernel::RangePack,
            &mut [
                (0, Read(input)),
                (1, Read(&uniform)),
                (3, Write(packed.write())),
            ],
            grid,
            &mut trace,
        );
        gpu.dispatch(
            Kernel::RangeFinish,
            &mut [
                (0, Read(&packed)),
                (1, Read(&uniform)),
                (2, Read(&packed)),
                (3, Write(output.write())),
                (4, Read(&tables)),
                (5, Read(&black)),
            ],
            grid,
            &mut trace,
        );
        gpu.finish_trace(&trace, profile)?;
        return Ok(output);
    }
    let reduced = u64::from(rw) * u64::from(rh) * 16;
    let mut a = gpu.storage("Range reduced RGB", reduced)?;
    let mut b = gpu.storage("Range filtered RGB", reduced)?;
    let mut guide = gpu.storage("Range fixed edge guide", reduced)?;
    let mut scratch = gpu.storage("Range recursive scratch", reduced)?;
    let mut expanded = gpu.storage("Range expanded RGB", full)?;
    let mut lum_a = gpu.storage("Range original luminance", full)?;
    let mut lum_b = gpu.storage("Range diffused luminance", full)?;
    let mut output = gpu.storage("Range linear RGB", u64::from(w) * u64::from(h) * 12)?;
    let mut black = gpu.storage("Range damped black estimate", 16)?;
    let uniform = gpu.uniform(&p.params);
    let tables = gpu.upload("Range tapered color lookup", &p.tables)?;
    let count = 9 + p.iterations[0] * 2 + p.iterations[1] + p.iterations[2];
    let mut trace = gpu.trace(profile.mode.gpu(), count);
    macro_rules! dispatch {
        ($kernel:ident,$src:expr,$other:expr,$dest:expr,$grid:expr) => {
            let mut bindings = vec![
                (0, Read($src)),
                (1, Read(&uniform)),
                (3, Write($dest.write())),
            ];
            if matches!(
                Kernel::$kernel,
                Kernel::RangeMeanHorizontal | Kernel::RangeMeanVertical
            ) {
                bindings.push((2, Read($other)));
            }
            gpu.dispatch(Kernel::$kernel, &mut bindings, $grid, &mut trace);
        };
    }
    let small = [rw.div_ceil(8), rh.div_ceil(8)];
    let large = [w.div_ceil(8), h.div_ceil(8)];
    dispatch!(RangeReduce, input, input, a, small);
    dispatch!(RangeBlack, &a, &a, black, [1, 1]);
    dispatch!(RangeBilateral, &a, &a, b, small);
    dispatch!(RangeGuide, &b, &b, guide, small);
    dispatch!(RangeNeutral, &b, &b, a, small);
    for _ in 0..p.iterations[0] {
        dispatch!(
            RangeMeanHorizontal,
            &a,
            &guide,
            scratch,
            [rh.div_ceil(64), 1]
        );
        dispatch!(RangeMeanVertical, &scratch, &guide, b, [rw.div_ceil(64), 1]);
        std::mem::swap(&mut a, &mut b);
    }
    dispatch!(RangeExpand, &a, &a, expanded, large);
    dispatch!(RangeLuminance, input, input, lum_a, large);
    // Every iteration owns a complete immutable input. The reference implementation's
    // overlapping worker strips race across iterations, so it has no unique target.
    for _ in 0..p.iterations[1] {
        dispatch!(RangeDiffuseNoise, &lum_a, &lum_a, lum_b, large);
        std::mem::swap(&mut lum_a, &mut lum_b);
    }
    for _ in 0..p.iterations[2] {
        dispatch!(RangeDiffuseConstant, &lum_a, &lum_a, lum_b, large);
        std::mem::swap(&mut lum_a, &mut lum_b);
    }
    dispatch!(RangeExtrema, &lum_a, &lum_a, lum_b, large);
    gpu.dispatch(
        Kernel::RangeFinish,
        &mut [
            (0, Read(&expanded)),
            (1, Read(&uniform)),
            (2, Read(&lum_b)),
            (3, Write(output.write())),
            (4, Read(&tables)),
            (5, Read(&black)),
        ],
        large,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "range_reconstruction_test.rs"]
mod tests;
