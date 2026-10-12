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
fn bilateral_groups(params: Parameters) -> [u32; 2] {
    let [width, height] = params.atlas_extent();
    [width.div_ceil(32), height.div_ceil(8)]
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
    pub seed: Option<wgpu::Buffer>,
}
struct Filtered {
    tiles: Option<wgpu::Buffer>,
    edges: wgpu::Buffer,
}
fn raw_tiles(
    gpu: &Gpu,
    input: &wgpu::Buffer,
    p: Parameters,
    profile: &mut Profile,
) -> Result<wgpu::Buffer> {
    let raw = gpu.storage("Noise input tiles", p.atlas_pixels() * 12)?;
    let uniform = gpu.uniform(&p);
    let mut trace = gpu.trace(profile.enabled, 1);
    gpu.dispatch(
        Kernel::NoiseTile,
        &[(0, input), (1, &uniform), (2, &raw)],
        groups(p),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(raw)
}
fn global_tiles(
    gpu: &Gpu,
    input: &wgpu::Buffer,
    mut p: Parameters,
    components: u32,
    profile: &mut Profile,
) -> Result<wgpu::Buffer> {
    p.weights[3] = components as f32;
    let out = gpu.storage(
        "Global reduced context tiles",
        p.atlas_pixels() * u64::from(components) * 4,
    )?;
    let uniform = gpu.uniform(&p);
    let mut trace = gpu.trace(profile.enabled, 1);
    let bindings = if components == 1 {
        vec![(0, input), (1, &uniform), (4, &out)]
    } else {
        vec![(0, input), (1, &uniform), (2, &out)]
    };
    gpu.dispatch(
        if components == 1 {
            Kernel::NoiseGlobalScalarTile
        } else {
            Kernel::NoiseGlobalTile
        },
        &bindings,
        groups(p),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(out)
}
pub(crate) fn pattern_seed(
    gpu: &Gpu,
    edges: &wgpu::Buffer,
    p: Parameters,
    profile: &mut Profile,
) -> Result<wgpu::Buffer> {
    let out = gpu.storage("Native transposed pattern seed", p.atlas_pixels() * 4)?;
    let uniform = gpu.uniform(&p);
    let mut trace = gpu.trace(profile.enabled, 1);
    gpu.dispatch(
        Kernel::NoisePatternSeed,
        &[(0, edges), (1, &uniform), (4, &out)],
        groups(p),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(out)
}
pub(crate) fn base(
    gpu: &Gpu,
    input: &wgpu::Buffer,
    p: Parameters,
    keep_edges: bool,
    need_tiles: bool,
    keep_seed: bool,
    profile: &mut Profile,
) -> Result<Base> {
    let scale = p.reduction[0];
    if !(0.375..=1.).contains(&scale) {
        return Err("unsupported reconstruction scale".into());
    }
    let tiles = p.source[2] * p.source[3];
    let reduced_tile = [
        (p.tile[0] as f32 * scale) as u32,
        (p.tile[1] as f32 * scale) as u32,
    ];
    if p.tile[..2]
        .iter()
        .zip(reduced_tile)
        .any(|(&a, b)| a as f32 * scale != b as f32)
    {
        return Err("nonintegral reduced tile geometry".into());
    }
    let global = scale <= 0.5;
    let mut reduced = p;
    reduced.tile[..2].copy_from_slice(&reduced_tile);
    reduced.tile[2] = (p.tile[2] as f32 * scale) as u32;
    let (raw, filter_params) = if global {
        let mut whole = p;
        whole.source[2..].copy_from_slice(&[1, 1]);
        whole.tile = [p.source[0], p.source[1], 0, p.geometry[2]];
        whole.geometry = [4, 0, p.geometry[2], 1];
        let raw = raw_tiles(gpu, input, whole, profile)?;
        let size = [
            (p.source[0] as f32 * scale) as u32,
            (p.source[1] as f32 * scale) as u32,
        ];
        let sampling = crate::resample::Prepared::new([p.source[0], p.source[1]], size, 1, scale)?;
        let raw = crate::resample::apply(gpu, &raw, &sampling, profile)?;
        whole.source[..2].copy_from_slice(&size);
        whole.tile[..2].copy_from_slice(&size);
        reduced.source[..2].copy_from_slice(&size);
        reduced.geometry[3] = 1;
        (raw, whole)
    } else {
        if scale < 1. {
            reduced.filter[3] = reduced.filter[3].min(0.92);
        }
        let raw = raw_tiles(gpu, input, p, profile)?;
        let raw = if scale < 1. {
            crate::resample::apply(
                gpu,
                &raw,
                &crate::resample::Prepared::new(
                    [p.tile[0], p.tile[1]],
                    reduced_tile,
                    tiles,
                    scale,
                )?,
                profile,
            )?
        } else {
            raw
        };
        (raw, reduced)
    };
    let filtered = filter(gpu, &raw, filter_params, need_tiles, profile)?;
    let seed = if keep_seed {
        let mut seed_p = p;
        seed_p.geometry[3] = u32::from(global);
        Some(pattern_seed(gpu, &filtered.edges, seed_p, profile)?)
    } else {
        None
    };
    let base = if global {
        filtered
            .tiles
            .as_ref()
            .map(|v| global_tiles(gpu, v, reduced, 3, profile))
            .transpose()?
    } else {
        filtered.tiles
    };
    let edges = if keep_edges {
        Some(if global {
            global_tiles(gpu, &filtered.edges, reduced, 1, profile)?
        } else {
            filtered.edges
        })
    } else {
        None
    };
    if scale < 1. {
        let up = crate::resample::Prepared::new(
            reduced_tile,
            [p.tile[0], p.tile[1]],
            tiles,
            1. / scale,
        )?;
        Ok(Base {
            tiles: base
                .as_ref()
                .map(|v| crate::resample::apply(gpu, v, &up, profile))
                .transpose()?,
            edges: edges
                .as_ref()
                .map(|v| crate::resample::apply_scalar(gpu, v, &up, profile))
                .transpose()?,
            seed,
        })
    } else {
        Ok(Base {
            tiles: base,
            edges,
            seed,
        })
    }
}
fn filter(
    gpu: &Gpu,
    raw: &wgpu::Buffer,
    params: Parameters,
    need_tiles: bool,
    profile: &mut Profile,
) -> Result<Filtered> {
    let n = params.atlas_pixels();
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
    if params.geometry[3] == 0 {
        gpu.dispatch(
            Kernel::NoiseBlock,
            &[(0, raw), (1, &uniform), (4, &block_mean)],
            grid.map(|v| v.div_ceil(8)),
            &mut trace,
        );
        gpu.dispatch(
            Kernel::NoiseRadius,
            &[(1, &uniform), (4, &block_mean), (7, &radii)],
            grid.map(|v| v.div_ceil(8)),
            &mut trace,
        );
    }
    gpu.dispatch(
        Kernel::NoiseBilateral,
        &[
            (0, raw),
            (1, &uniform),
            (2, &filtered),
            (6, &tables),
            (7, &radii),
        ],
        bilateral_groups(params),
        &mut trace,
    );
    // Median values are reused across neighboring gradients, then this buffer
    // is fully overwritten by the horizontal directional-mean pass.
    let horizontal = gpu.storage("Cross-median / directional-mean workspace", n * 12)?;
    gpu.dispatch(
        Kernel::NoiseMedian,
        &[(0, &filtered), (1, &uniform), (2, &horizontal)],
        groups(params),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseEdge,
        &[(0, &horizontal), (1, &uniform), (4, &edges), (6, &tables)],
        groups(params),
        &mut trace,
    );
    if !need_tiles {
        gpu.finish_trace(&trace, profile)?;
        return Ok(Filtered { tiles: None, edges });
    }
    let prefix = gpu.storage("Directional prefix sums", n * 12)?;
    let weights = gpu.storage("Directional accumulated weights", n * 4)?;
    gpu.dispatch(
        Kernel::NoiseMean,
        &[
            (0, &filtered),
            (1, &uniform),
            (2, &horizontal),
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
            (0, &horizontal),
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
    Ok(Filtered {
        tiles: Some(filtered),
        edges,
    })
}

pub(crate) fn column(
    gpu: &Gpu,
    input: &wgpu::Buffer,
    params: Parameters,
    profile: &mut Profile,
) -> Result<wgpu::Buffer> {
    let n = params.atlas_pixels();
    let raw = gpu.storage("Immutable column input tiles", n * 12)?;
    let smoothed = gpu.storage("Column directional workspace", n * 12)?;
    let blocks = params.tile[1] / 40;
    if blocks == 0 {
        return Err("unsupported column tile height".into());
    }
    let deltas = gpu.storage(
        "Column block deltas",
        u64::from(params.tile[0])
            * u64::from(blocks)
            * u64::from(params.source[2])
            * u64::from(params.source[3])
            * 12,
    )?;
    let output = gpu.storage("Column conditioned tiles", n * 12)?;
    let uniform = gpu.uniform(&params);
    let table = gpu.upload("Column bilateral lookup", &tables(params)[..2048])?;
    let mut trace = gpu.trace(profile.enabled, 4);
    gpu.dispatch(
        Kernel::NoiseTile,
        &[(0, input), (1, &uniform), (2, &raw)],
        groups(params),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ColumnHorizontal,
        &[(0, &raw), (1, &uniform), (2, &smoothed), (5, &table)],
        groups(params),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ColumnDeltas,
        &[(0, &raw), (1, &uniform), (2, &smoothed), (3, &deltas)],
        [
            params.tile[0].div_ceil(64),
            blocks * params.source[2] * params.source[3],
        ],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ColumnApply,
        &[(0, &raw), (1, &uniform), (3, &deltas), (4, &output)],
        groups(params),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

pub(crate) fn ratio(
    gpu: &Gpu,
    raw: &wgpu::Buffer,
    base: &wgpu::Buffer,
    params: Parameters,
    profile: &mut Profile,
) -> Result<wgpu::Buffer> {
    let n = params.atlas_pixels();
    let a = gpu.storage("Conditioned luminance A", n * 4)?;
    let b = gpu.storage("Conditioned luminance B", n * 4)?;
    let uniform = gpu.uniform(&params);
    let mut trace = gpu.trace(profile.enabled, 5);
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
    gpu.finish_trace(&trace, profile)?;
    Ok(a)
}

pub(crate) fn mix(
    gpu: &Gpu,
    raw: &wgpu::Buffer,
    base: &wgpu::Buffer,
    ratio: &wgpu::Buffer,
    params: Parameters,
    profile: &mut Profile,
) -> Result<wgpu::Buffer> {
    let gains = gpu.storage("Residual layer gains", params.atlas_pixels() * 12)?;
    let uniform = gpu.uniform(&params);
    let mut trace = gpu.trace(profile.enabled, 1);
    gpu.dispatch(
        Kernel::NoiseMix,
        &[(0, raw), (1, &uniform), (2, &gains), (3, base), (4, ratio)],
        groups(params),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(gains)
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
