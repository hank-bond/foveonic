//! Executes reference-style processing tiles, noise filtering, residual detail, and image projections on GPU.

#[cfg(test)]
#[path = "reconstruction_test.rs"]
mod tests;

use crate::{
    calibration::ColorParams,
    color_dq::Table,
    gpu::{Gpu, Kernel},
    noise::Parameters,
    profile::Profile,
    source::Result,
};

fn tables(params: Parameters) -> Vec<f32> {
    let normalization = 1. / (1. - params.filter[2] as f64);
    (0..4096)
        .map(|i| {
            if i < 2048 {
                (i as f32 * f32::from_bits(0xbb801002)).exp()
            } else {
                let value = (((i - 2048) as f32 * f32::from_bits(0xbb001002)).tanh() + 1.)
                    - params.filter[2];
                params.filter[3] * (normalization * value as f64).max(0.) as f32
            }
        })
        .collect()
}
fn groups(params: Parameters) -> [u32; 2] {
    params.atlas_extent().map(|v| v.div_ceil(8))
}

pub(crate) fn project(
    gpu: &Gpu,
    tiles: &wgpu::Buffer,
    mut params: Parameters,
    components: u32,
    core_only: bool,
    profile: &mut Profile,
) -> Result<wgpu::Buffer> {
    let size =
        u64::from(params.source[0]) * u64::from(params.source[1]) * u64::from(components) * 4;
    let output = gpu.storage("Projected processing tiles", size)?;
    params.weights[3] = components as f32;
    params.diffusion[3] = if core_only { 2. } else { 0. };
    let uniform = gpu.uniform(&params);
    let mut trace = gpu.trace(profile.enabled, 1);
    gpu.dispatch(
        Kernel::NoiseStitch,
        &[(0, tiles), (1, &uniform), (2, &output)],
        [params.source[0].div_ceil(8), params.source[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

pub(crate) struct Base {
    pub tiles: Option<wgpu::Buffer>,
    pub edges: Option<wgpu::Buffer>,
}
pub(crate) fn base(
    gpu: &Gpu,
    input: &wgpu::Buffer,
    params: Parameters,
    keep_edges: bool,
    need_tiles: bool,
    profile: &mut Profile,
) -> Result<Base> {
    let n = params.atlas_pixels();
    let raw = gpu.storage("Noise input tiles", n * 12)?;
    let filtered = gpu.storage("Bilateral base tiles", n * 12)?;
    let edges = gpu.storage("Noise edge weights", n * 4)?;
    let grid = [
        params.tile[0].div_ceil(32),
        params.tile[1].div_ceil(32) * params.source[2] * params.source[3],
    ];
    let block_mean = gpu.storage(
        "Noise block means",
        u64::from(grid[0]) * u64::from(grid[1]) * 4,
    )?;
    let radii = gpu.storage("Adaptive noise radii", block_mean.size())?;
    let tables = gpu.upload("Reference noise lookup tables", &tables(params))?;
    let uniform = gpu.uniform(&params);
    let mut trace = gpu.trace(profile.enabled, 7);
    gpu.dispatch(
        Kernel::NoiseTile,
        &[(0, input), (1, &uniform), (2, &raw)],
        groups(params),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseBlock,
        &[(0, &raw), (1, &uniform), (4, &block_mean)],
        grid.map(|v| v.div_ceil(8)),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseRadius,
        &[(1, &uniform), (4, &block_mean), (7, &radii)],
        grid.map(|v| v.div_ceil(8)),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseBilateral,
        &[
            (0, &raw),
            (1, &uniform),
            (2, &filtered),
            (6, &tables),
            (7, &radii),
        ],
        groups(params),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseEdge,
        &[(0, &filtered), (1, &uniform), (4, &edges), (6, &tables)],
        groups(params),
        &mut trace,
    );
    if !need_tiles {
        gpu.finish_trace(&trace, profile)?;
        return Ok(Base {
            tiles: None,
            edges: keep_edges.then_some(edges),
        });
    }
    let prefix = gpu.storage("Directional prefix sums", n * 12)?;
    let weights = gpu.storage("Directional accumulated weights", n * 4)?;
    gpu.dispatch(
        Kernel::NoiseMean,
        &[
            (0, &filtered),
            (1, &uniform),
            (2, &raw),
            (3, &edges),
            (4, &weights),
            (5, &prefix),
        ],
        [
            (params.tile[1] * params.source[2] * params.source[3]).div_ceil(64),
            1,
        ],
        &mut trace,
    );
    let mut vertical = params;
    vertical.diffusion[3] = 1.;
    let uniform = gpu.uniform(&vertical);
    gpu.dispatch(
        Kernel::NoiseMean,
        &[
            (0, &raw),
            (1, &uniform),
            (2, &filtered),
            (3, &edges),
            (4, &weights),
            (5, &prefix),
        ],
        [
            (params.tile[0] * params.source[2] * params.source[3]).div_ceil(64),
            1,
        ],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(Base {
        tiles: Some(filtered),
        edges: keep_edges.then_some(edges),
    })
}

pub(crate) struct Detail {
    pub gains: Option<wgpu::Buffer>,
    pub ratio: wgpu::Buffer,
}
pub(crate) fn detail(
    gpu: &Gpu,
    raw: &wgpu::Buffer,
    base: &wgpu::Buffer,
    params: Parameters,
    need_gains: bool,
    profile: &mut Profile,
) -> Result<Detail> {
    let n = params.atlas_pixels();
    let a = gpu.storage("Conditioned luminance A", n * 4)?;
    let b = gpu.storage("Conditioned luminance B", n * 4)?;
    let uniform = gpu.uniform(&params);
    let mut trace = gpu.trace(profile.enabled, 6);
    gpu.dispatch(
        Kernel::NoiseLuminance,
        &[(0, raw), (1, &uniform), (2, &a)],
        groups(params),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseDiffuse,
        &[(0, &a), (1, &uniform), (2, &b)],
        groups(params),
        &mut trace,
    );
    let mut fixed = params;
    fixed.diffusion[3] = 1.;
    let fixed_uniform = gpu.uniform(&fixed);
    gpu.dispatch(
        Kernel::NoiseDiffuse,
        &[(0, &b), (1, &fixed_uniform), (2, &a)],
        groups(params),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseOsf,
        &[(0, &a), (1, &uniform), (2, &b)],
        groups(params),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseRatio,
        &[(0, &b), (1, &uniform), (2, &a), (3, base)],
        groups(params),
        &mut trace,
    );
    let gains = if need_gains {
        let gains = gpu.storage("Residual layer gains", n * 12)?;
        gpu.dispatch(
            Kernel::NoiseMix,
            &[(0, raw), (1, &uniform), (2, &gains), (3, base), (4, &a)],
            groups(params),
            &mut trace,
        );
        Some(gains)
    } else {
        None
    };
    gpu.finish_trace(&trace, profile)?;
    Ok(Detail { gains, ratio: a })
}

pub(crate) fn color(
    gpu: &Gpu,
    input: &wgpu::Buffer,
    mut color: ColorParams,
    params: Parameters,
    profile: &mut Profile,
) -> Result<wgpu::Buffer> {
    color.source[..2].copy_from_slice(&params.atlas_extent());
    let uniform = gpu.uniform(&color);
    let output = gpu.storage("Camera-color processing tiles", params.atlas_pixels() * 12)?;
    let mut trace = gpu.trace(profile.enabled, 1);
    gpu.dispatch(
        Kernel::Color,
        &[(0, input), (1, &uniform), (2, &output)],
        groups(params),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
pub(crate) fn color_dq(
    gpu: &Gpu,
    input: &wgpu::Buffer,
    table: &Table,
    params: Parameters,
    profile: &mut Profile,
) -> Result<wgpu::Buffer> {
    let [w, h] = params.atlas_extent();
    let uniform = gpu.uniform(&[w, h, table.period, 0]);
    let lookup = gpu.upload("Processing-tile ColorDQ", &table.values)?;
    let output = gpu.storage("ColorDQ processing tiles", params.atlas_pixels() * 12)?;
    let mut trace = gpu.trace(profile.enabled, 1);
    gpu.dispatch(
        Kernel::ColorDq,
        &[(0, input), (1, &lookup), (2, &uniform), (3, &output)],
        groups(params),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
pub(crate) fn restore(
    gpu: &Gpu,
    color: &wgpu::Buffer,
    gains: &wgpu::Buffer,
    params: Parameters,
    profile: &mut Profile,
) -> Result<wgpu::Buffer> {
    let output = gpu.storage("Reconstructed RGB tiles", params.atlas_pixels() * 12)?;
    let uniform = gpu.uniform(&params);
    let mut trace = gpu.trace(profile.enabled, 1);
    gpu.dispatch(
        Kernel::NoiseRestore,
        &[(0, gains), (1, &uniform), (2, &output), (3, color)],
        groups(params),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    project(gpu, &output, params, 3, false, profile)
}
pub(crate) fn signal(
    gpu: &Gpu,
    color: &wgpu::Buffer,
    ratio: &wgpu::Buffer,
    params: Parameters,
    profile: &mut Profile,
) -> Result<wgpu::Buffer> {
    let output = gpu.storage(
        "Signed reconstruction detail tiles",
        params.atlas_pixels() * 4,
    )?;
    let uniform = gpu.uniform(&params);
    let mut trace = gpu.trace(profile.enabled, 1);
    gpu.dispatch(
        Kernel::NoiseSignal,
        &[(1, &uniform), (2, &output), (3, color), (4, ratio)],
        groups(params),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    project(gpu, &output, params, 1, true, profile)
}
