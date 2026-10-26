//! Filters camera noise, conditions brightness, and projects processing tiles on GPU.

use crate::gpu::Binding::{Read, Write};
#[cfg(test)]
#[path = "reconstruction_test.rs"]
mod tests;

use crate::{
    gpu::{Gpu, Kernel},
    nodes::{color::color_dq::Table, finish::resample, sensor::calibration::ColorParams},
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
    tiles: &crate::gpu::ReadGuard,
    mut params: Parameters,
    components: u32,
    profile: &mut Profile,
) -> Result<crate::gpu::ReadGuard> {
    let size = u64::from(params.output_extent()[0])
        * u64::from(params.output_extent()[1])
        * u64::from(components)
        * 4;
    let mut output = gpu.storage("Projected processing tiles", size)?;
    params.weights[3] = components as f32;
    let uniform = gpu.uniform(&params);
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::NoiseStitch,
        &mut [
            (0, Read(tiles)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
        ],
        params.output_extent().map(|v| v.div_ceil(8)),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

pub(crate) struct Base {
    pub tiles: Option<crate::gpu::ReadGuard>,
    pub edges: Option<crate::gpu::ReadGuard>,
    pub seed: Option<crate::gpu::ReadGuard>,
}
struct Filtered {
    tiles: Option<crate::gpu::ReadGuard>,
    edges: crate::gpu::ReadGuard,
}
pub(crate) struct GlobalContext {
    pub layers: crate::gpu::ReadGuard,
    pub edges: crate::gpu::ReadGuard,
}
pub(crate) fn raw_tiles(
    gpu: &Gpu,
    input: &crate::gpu::ReadGuard,
    p: Parameters,
    profile: &mut Profile,
) -> Result<crate::gpu::ReadGuard> {
    let mut raw = gpu.storage("Noise input tiles", p.atlas_pixels() * 12)?;
    let uniform = gpu.uniform(&p);
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::NoiseTile,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(raw.write())),
        ],
        groups(p),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(raw)
}
fn global_tiles(
    gpu: &Gpu,
    input: &crate::gpu::ReadGuard,
    mut p: Parameters,
    components: u32,
    profile: &mut Profile,
) -> Result<crate::gpu::ReadGuard> {
    p.weights[3] = components as f32;
    let mut out = gpu.storage(
        "Global reduced context tiles",
        p.atlas_pixels() * u64::from(components) * 4,
    )?;
    let uniform = gpu.uniform(&p);
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    let mut bindings = if components == 1 {
        vec![
            (0, Read(input)),
            (1, Read(&uniform)),
            (4, Write(out.write())),
        ]
    } else {
        vec![
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(out.write())),
        ]
    };
    gpu.dispatch(
        if components == 1 {
            Kernel::NoiseGlobalScalarTile
        } else {
            Kernel::NoiseGlobalTile
        },
        &mut bindings,
        groups(p),
        &mut trace,
    );
    drop(bindings);
    gpu.finish_trace(&trace, profile)?;
    Ok(out)
}
pub(crate) fn pattern_seed(
    gpu: &Gpu,
    edges: &crate::gpu::ReadGuard,
    p: Parameters,
    profile: &mut Profile,
) -> Result<crate::gpu::ReadGuard> {
    let mut out = gpu.storage("Native transposed pattern seed", p.atlas_pixels() * 4)?;
    let uniform = gpu.uniform(&p);
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::NoisePatternSeed,
        &mut [
            (0, Read(edges)),
            (1, Read(&uniform)),
            (4, Write(out.write())),
        ],
        groups(p),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(out)
}
pub(crate) fn base(
    gpu: &Gpu,
    input: &crate::gpu::ReadGuard,
    p: Parameters,
    keep_edges: bool,
    need_tiles: bool,
    keep_seed: bool,
    profile: &mut Profile,
) -> Result<Base> {
    let scale = p.reduction[0];
    let tiles = p.tile_count();
    let reduced_tile = p.reduced_tile_extent()?;
    if p.uses_global_context() {
        let context = global_context(gpu, input, p, profile)?;
        return base_from_context(
            gpu,
            &context.layers,
            &context.edges,
            p,
            keep_edges,
            need_tiles,
            keep_seed,
            profile,
        );
    }
    let mut reduced = p;
    reduced.tile[..2].copy_from_slice(&reduced_tile);
    reduced.tile[2] = (p.tile[2] as f32 * scale) as u32;
    let raw = raw_tiles(gpu, input, p, profile)?;
    let raw = if scale < 1. {
        resample::apply(
            gpu,
            &raw,
            &resample::Prepared::new([p.tile[0], p.tile[1]], reduced_tile, tiles, scale)?,
            profile,
        )?
    } else {
        raw
    };
    let filtered = filter(gpu, &raw, reduced, need_tiles, profile)?;
    let seed = if keep_seed {
        Some(pattern_seed(gpu, &filtered.edges, p, profile)?)
    } else {
        None
    };
    let base = filtered.tiles;
    let edges = keep_edges.then_some(filtered.edges);
    if scale < 1. {
        let up = resample::Prepared::new(reduced_tile, [p.tile[0], p.tile[1]], tiles, 1. / scale)?;
        Ok(Base {
            tiles: base
                .as_ref()
                .map(|v| resample::apply(gpu, v, &up, profile))
                .transpose()?,
            edges: edges
                .as_ref()
                .map(|v| resample::apply_scalar(gpu, v, &up, profile))
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
/// The low-scale branch filters the reduced whole sensor before tile extraction.
/// Keep this immutable context independent of the requested tile window.
pub(crate) fn global_context(
    gpu: &Gpu,
    input: &crate::gpu::ReadGuard,
    p: Parameters,
    profile: &mut Profile,
) -> Result<GlobalContext> {
    p.reduced_tile_extent()?;
    let mut whole = p.full_frame();
    whole.source[2..].copy_from_slice(&[1, 1]);
    whole.window = [0, 0, 1, 1];
    whole.tile = [p.source[0], p.source[1], 0, p.geometry[2]];
    whole.geometry = [4, 0, p.geometry[2], 1];
    let raw = raw_tiles(gpu, input, whole, profile)?;
    let size = context_extent(p);
    let raw = resample::apply(
        gpu,
        &raw,
        &resample::Prepared::new([p.source[0], p.source[1]], size, 1, p.reduction[0])?,
        profile,
    )?;
    whole.source[..2].copy_from_slice(&size);
    whole.tile[..2].copy_from_slice(&size);
    let filtered = filter(gpu, &raw, whole, true, profile)?;
    Ok(GlobalContext {
        layers: filtered.tiles.ok_or("missing whole-image noise base")?,
        edges: filtered.edges,
    })
}

pub(crate) fn context_extent(p: Parameters) -> [u32; 2] {
    [p.source[0], p.source[1]].map(|v| (v as f32 * p.reduction[0]) as u32)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn base_from_context(
    gpu: &Gpu,
    context: &crate::gpu::ReadGuard,
    context_edges: &crate::gpu::ReadGuard,
    p: Parameters,
    keep_edges: bool,
    need_tiles: bool,
    keep_seed: bool,
    profile: &mut Profile,
) -> Result<Base> {
    let scale = p.reduction[0];
    let reduced_tile = p.reduced_tile_extent()?;
    let mut reduced = p;
    reduced.source[..2].copy_from_slice(&context_extent(p));
    reduced.tile[..2].copy_from_slice(&reduced_tile);
    reduced.tile[2] = (p.tile[2] as f32 * scale) as u32;
    reduced.geometry[3] = 1;
    let up = resample::Prepared::new(
        [reduced.tile[0], reduced.tile[1]],
        [p.tile[0], p.tile[1]],
        p.tile_count(),
        1. / scale,
    )?;
    let tiles = if need_tiles {
        let tiles = global_tiles(gpu, context, reduced, 3, profile)?;
        Some(resample::apply(gpu, &tiles, &up, profile)?)
    } else {
        None
    };
    let edges = if keep_edges {
        let tiles = global_tiles(gpu, context_edges, reduced, 1, profile)?;
        Some(resample::apply_scalar(gpu, &tiles, &up, profile)?)
    } else {
        None
    };
    let seed = if keep_seed {
        let mut seed_p = p;
        seed_p.geometry[3] = 1;
        Some(pattern_seed(gpu, context_edges, seed_p, profile)?)
    } else {
        None
    };
    Ok(Base { tiles, edges, seed })
}

fn filter(
    gpu: &Gpu,
    raw: &crate::gpu::ReadGuard,
    params: Parameters,
    need_tiles: bool,
    profile: &mut Profile,
) -> Result<Filtered> {
    let n = params.atlas_pixels();
    let mut filtered = gpu.storage("Bilateral base tiles", n * 12)?;
    let mut edges = gpu.storage("Noise edge weights", n * 4)?;
    let grid = [
        params.tile[0].div_ceil(32),
        params.tile[1].div_ceil(32) * params.tile_count(),
    ];
    let mut block_mean = gpu.storage(
        "Noise block means",
        u64::from(grid[0]) * u64::from(grid[1]) * 4,
    )?;
    let mut radii = gpu.storage("Adaptive noise radii", block_mean.size())?;
    let tables = gpu.upload("Reference noise lookup tables", &tables(params))?;
    let uniform = gpu.uniform(&params);
    let mut trace = gpu.trace(profile.mode.gpu(), 9);
    if params.geometry[3] == 0 {
        gpu.dispatch(
            Kernel::NoiseBlock,
            &mut [
                (0, Read(raw)),
                (1, Read(&uniform)),
                (4, Write(block_mean.write())),
            ],
            grid.map(|v| v.div_ceil(8)),
            &mut trace,
        );
        gpu.dispatch(
            Kernel::NoiseRadius,
            &mut [
                (1, Read(&uniform)),
                (4, Read(&block_mean)),
                (7, Write(radii.write())),
            ],
            grid.map(|v| v.div_ceil(8)),
            &mut trace,
        );
    }
    gpu.dispatch(
        Kernel::NoiseBilateral,
        &mut [
            (0, Read(raw)),
            (1, Read(&uniform)),
            (2, Write(filtered.write())),
            (6, Read(&tables)),
            (7, Read(&radii)),
        ],
        bilateral_groups(params),
        &mut trace,
    );
    // Median values are reused across neighboring gradients, then this buffer
    // is fully overwritten by the horizontal directional-mean pass.
    let mut horizontal = gpu.storage("Cross-median / directional-mean workspace", n * 12)?;
    gpu.dispatch(
        Kernel::NoiseMedian,
        &mut [
            (0, Read(&filtered)),
            (1, Read(&uniform)),
            (2, Write(horizontal.write())),
        ],
        groups(params),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseEdge,
        &mut [
            (0, Read(&horizontal)),
            (1, Read(&uniform)),
            (4, Write(edges.write())),
            (6, Read(&tables)),
        ],
        groups(params),
        &mut trace,
    );
    if !need_tiles {
        gpu.finish_trace(&trace, profile)?;
        return Ok(Filtered { tiles: None, edges });
    }
    let mut prefix = gpu.storage("Directional prefix sums", n * 12)?;
    let mut weights = gpu.storage("Directional accumulated weights", n * 4)?;
    // Transpose into existing workspaces, rather than changing the recurrence
    // or keeping another RGB allocation. Adjacent lanes then access adjacent
    // samples for both scan directions.
    let mut transposed_edges = gpu.storage("Transposed recursive weights", n * 4)?;
    gpu.dispatch(
        Kernel::NoiseMeanTranspose,
        &mut [
            (0, Read(&filtered)),
            (1, Read(&uniform)),
            (2, Write(horizontal.write())),
            (3, Read(&edges)),
            (4, Write(transposed_edges.write())),
        ],
        groups(params),
        &mut trace,
    );
    let mut transposed = params;
    transposed.tile.swap(0, 1);
    transposed.diffusion[3] = 1.;
    let transposed_uniform = gpu.uniform(&transposed);
    gpu.dispatch(
        Kernel::NoiseMean,
        &mut [
            (0, Read(&horizontal)),
            (1, Read(&transposed_uniform)),
            (2, Write(filtered.write())),
            (3, Read(&transposed_edges)),
            (4, Write(weights.write())),
            (5, Write(prefix.write())),
        ],
        [(params.tile[1] * params.tile_count()).div_ceil(64), 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseMeanRestore,
        &mut [
            (0, Read(&filtered)),
            (1, Read(&transposed_uniform)),
            (2, Write(horizontal.write())),
        ],
        groups(transposed),
        &mut trace,
    );
    let mut vertical = params;
    vertical.diffusion[3] = 1.;
    let uniform = gpu.uniform(&vertical);
    gpu.dispatch(
        Kernel::NoiseMean,
        &mut [
            (0, Read(&horizontal)),
            (1, Read(&uniform)),
            (2, Write(filtered.write())),
            (3, Read(&edges)),
            (4, Write(weights.write())),
            (5, Write(prefix.write())),
        ],
        [(params.tile[0] * params.tile_count()).div_ceil(64), 1],
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
    input: &crate::gpu::ReadGuard,
    params: Parameters,
    profile: &mut Profile,
) -> Result<crate::gpu::ReadGuard> {
    let n = params.atlas_pixels();
    let mut raw = gpu.storage("Immutable column input tiles", n * 12)?;
    let mut smoothed = gpu.storage("Column directional workspace", n * 12)?;
    let blocks = params.tile[1] / 40;
    if blocks == 0 {
        return Err("unsupported column tile height".into());
    }
    let mut deltas = gpu.storage(
        "Column block deltas",
        u64::from(params.tile[0]) * u64::from(blocks) * u64::from(params.tile_count()) * 12,
    )?;
    let mut output = gpu.storage("Column conditioned tiles", n * 12)?;
    let uniform = gpu.uniform(&params);
    let table = gpu.upload("Column bilateral lookup", &tables(params)[..2048])?;
    let mut trace = gpu.trace(profile.mode.gpu(), 4);
    gpu.dispatch(
        Kernel::NoiseTile,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(raw.write())),
        ],
        groups(params),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ColumnHorizontal,
        &mut [
            (0, Read(&raw)),
            (1, Read(&uniform)),
            (2, Write(smoothed.write())),
            (5, Read(&table)),
        ],
        groups(params),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ColumnDeltas,
        &mut [
            (0, Read(&raw)),
            (1, Read(&uniform)),
            (2, Read(&smoothed)),
            (3, Write(deltas.write())),
        ],
        [params.tile[0].div_ceil(64), blocks * params.tile_count()],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::ColumnApply,
        &mut [
            (0, Read(&raw)),
            (1, Read(&uniform)),
            (3, Read(&deltas)),
            (4, Write(output.write())),
        ],
        groups(params),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

pub(crate) fn ratio(
    gpu: &Gpu,
    raw: &crate::gpu::ReadGuard,
    base: &crate::gpu::ReadGuard,
    params: Parameters,
    profile: &mut Profile,
) -> Result<crate::gpu::ReadGuard> {
    let n = params.atlas_pixels();
    let mut a = gpu.storage("Conditioned luminance A", n * 4)?;
    let mut b = gpu.storage("Conditioned luminance B", n * 4)?;
    let uniform = gpu.uniform(&params);
    let mut trace = gpu.trace(profile.mode.gpu(), 5);
    gpu.dispatch(
        Kernel::NoiseLuminance,
        &mut [(0, Read(raw)), (1, Read(&uniform)), (2, Write(a.write()))],
        groups(params),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseDiffuse,
        &mut [(0, Read(&a)), (1, Read(&uniform)), (2, Write(b.write()))],
        groups(params),
        &mut trace,
    );
    let mut fixed = params;
    fixed.diffusion[3] = 1.;
    let fixed_uniform = gpu.uniform(&fixed);
    gpu.dispatch(
        Kernel::NoiseDiffuse,
        &mut [
            (0, Read(&b)),
            (1, Read(&fixed_uniform)),
            (2, Write(a.write())),
        ],
        groups(params),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseOsf,
        &mut [(0, Read(&a)), (1, Read(&uniform)), (2, Write(b.write()))],
        groups(params),
        &mut trace,
    );
    gpu.dispatch(
        Kernel::NoiseRatio,
        &mut [
            (0, Read(&b)),
            (1, Read(&uniform)),
            (2, Write(a.write())),
            (3, Read(base)),
        ],
        groups(params),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(a)
}

pub(crate) fn color(
    gpu: &Gpu,
    input: &crate::gpu::ReadGuard,
    mut color: ColorParams,
    params: Parameters,
    profile: &mut Profile,
) -> Result<crate::gpu::ReadGuard> {
    color.source[..2].copy_from_slice(&params.atlas_extent());
    let uniform = gpu.uniform(&color);
    let mut output = gpu.storage("Camera-color processing tiles", params.atlas_pixels() * 12)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::Color,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
        ],
        groups(params),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
pub(crate) fn color_dq(
    gpu: &Gpu,
    input: &crate::gpu::ReadGuard,
    table: &Table,
    params: Parameters,
    profile: &mut Profile,
) -> Result<crate::gpu::ReadGuard> {
    let [w, h] = params.atlas_extent();
    let uniform = gpu.uniform(&[w, h, table.period, 0]);
    let lookup = gpu.upload("Processing-tile ColorDQ", &table.values)?;
    let mut output = gpu.storage("ColorDQ processing tiles", params.atlas_pixels() * 12)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::ColorDq,
        &mut [
            (0, Read(input)),
            (1, Read(&lookup)),
            (2, Read(&uniform)),
            (3, Write(output.write())),
        ],
        groups(params),
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
