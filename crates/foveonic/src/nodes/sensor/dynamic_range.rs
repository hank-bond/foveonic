//! Reconstructs near-saturated channels from GPU cell colors and ordered global color-ratio fits.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    source::Result,
};

pub(super) const CELL_SIDE: u32 = 16;
const GRID_COMPONENTS: u64 = 4;
const COEFFICIENT_WORDS: u64 = 8;

/// Populate the shared directional-restoration work planes without reading image data on the host.
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    gpu: &Gpu,
    input: &ReadGuard,
    params: &ReadGuard,
    matrix: &ReadGuard,
    work: &mut ReadGuard,
    green_mask: &mut ReadGuard,
    extent: [u32; 2],
    extend: bool,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = extent;
    let cells = if extend {
        u64::from(width.div_ceil(CELL_SIDE)) * u64::from(height.div_ceil(CELL_SIDE))
    } else {
        1
    };
    let mut grid = gpu.storage(
        "Highlight color cells",
        cells * GRID_COMPONENTS * size_of::<f32>() as u64,
    )?;
    let mut coefficients = gpu.storage(
        "Highlight color regressions",
        COEFFICIENT_WORDS * size_of::<f32>() as u64,
    )?;
    let mut recovery = gpu.storage(
        "Highlight recovery channel flags",
        u64::from(width) * u64::from(height) * size_of::<u32>() as u64,
    )?;
    let mut trace = gpu.trace(profile.mode.gpu(), 4);
    gpu.dispatch(
        Kernel::DynamicRangePrepare,
        &mut [
            (0, Read(input)),
            (1, Read(params)),
            (3, Write(work.write())),
            (5, Write(green_mask.write())),
        ],
        [width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    if extend {
        gpu.dispatch(
            Kernel::DynamicRangeCells,
            &mut [(1, Read(params)), (3, Read(work)), (4, Write(grid.write()))],
            [
                width.div_ceil(CELL_SIDE).div_ceil(8),
                height.div_ceil(CELL_SIDE).div_ceil(8),
            ],
            &mut trace,
        );
        // These small ordered passes preserve which original cells contribute to the fit.
        gpu.dispatch(
            Kernel::DynamicRangeFit,
            &mut [
                (1, Read(params)),
                (4, Write(grid.write())),
                (7, Write(coefficients.write())),
            ],
            [1, 1],
            &mut trace,
        );
    }
    gpu.dispatch(
        Kernel::DynamicRangeApply,
        &mut [
            (1, Read(params)),
            (2, Read(matrix)),
            (3, Write(work.write())),
            (4, Read(&grid)),
            (6, Write(recovery.write())),
            (7, Read(&coefficients)),
        ],
        [width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(recovery)
}
