//! Estimates GPU allocations for resolved operations, audited where a contract exists and generic otherwise.

use crate::{
    graph::Operation,
    nodes::{color::reconstruction, color::texture, finish::resize},
};

/// Outputs without an image description, such as masks and evidence tables,
/// are sized at the widest image layout: RGBA32 float.
const UNDESCRIBED_BYTES_PER_PIXEL: u64 = 16;
/// Operations without an audited contract reserve temporaries at three times
/// their output size. Measured Quattro noise, repair and blending operations
/// allocate up to about that much. Overestimating only narrows how many jobs
/// run together.
const GENERIC_TEMPORARY_PER_OUTPUT_BYTE: u64 = 3;

/// Fresh output storage and operation-local buffers that can remain submitted.
/// These bytes exclude existing inputs, CPU preparation, profiling and presentation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuAllocation {
    pub output_bytes: u64,
    pub temporary_bytes: u64,
}

impl GpuAllocation {
    pub(crate) fn new(output_bytes: u64, temporary_bytes: u64) -> Option<Self> {
        output_bytes.checked_add(temporary_bytes)?;
        Some(Self {
            output_bytes,
            temporary_bytes,
        })
    }
}

/// One node's estimated allocations and the input bytes it releases after its last read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NodeCost {
    pub allocation: GpuAllocation,
    pub released_bytes: u64,
}

/// The most new memory the remaining nodes need beyond what is allocated now.
/// Released inputs stay allocated as scratch and later outputs reuse them before
/// allocating more. Each node's temporaries add while it runs.
/// Processing tiles pad outputs with overscan the per-node sizes do not model.
/// Measured native Quattro regions exceed the unpadded peak by about 3%.
const TILE_PADDING_HEADROOM_PERCENT: u64 = 10;

pub(super) fn peak_bytes(costs: &[NodeCost]) -> Option<u64> {
    let mut allocated = 0_u64;
    let mut reusable = 0_u64;
    let mut peak = 0_u64;
    for cost in costs {
        let reused = reusable.min(cost.allocation.output_bytes);
        reusable -= reused;
        allocated = allocated.checked_add(cost.allocation.output_bytes - reused)?;
        peak = peak.max(allocated.checked_add(cost.allocation.temporary_bytes)?);
        reusable = reusable.checked_add(cost.released_bytes)?;
    }
    peak.checked_mul(100 + TILE_PADDING_HEADROOM_PERCENT)
        .map(|bytes| bytes / 100)
}

/// BufferInitDescriptor pads uploaded contents to COPY_BUFFER_ALIGNMENT.
/// Reserving the minimum alignment also safely covers an empty upload.
pub(crate) fn uploaded_bytes<T>(count: usize) -> Option<u64> {
    let bytes = u64::try_from(count)
        .ok()?
        .checked_mul(size_of::<T>() as u64)?;
    let alignment = wgpu::COPY_BUFFER_ALIGNMENT;
    bytes
        .max(alignment)
        .checked_add(alignment - 1)
        .map(|n| n / alignment * alignment)
}

pub(crate) fn image_bytes(extent: [u32; 2], components: u32) -> Option<u64> {
    if extent.contains(&0) || components == 0 {
        return None;
    }
    u64::from(extent[0])
        .checked_mul(u64::from(extent[1]))?
        .checked_mul(u64::from(components))?
        .checked_mul(size_of::<f32>() as u64)
}

pub(super) fn undescribed_bytes(extent: [u32; 2]) -> Option<u64> {
    u64::from(extent[0])
        .checked_mul(u64::from(extent[1]))?
        .checked_mul(UNDESCRIBED_BYTES_PER_PIXEL)
}

/// The fallback for operations without an audited contract.
pub(super) fn generic(output_bytes: u64) -> Option<GpuAllocation> {
    GpuAllocation::new(
        output_bytes,
        output_bytes.checked_mul(GENERIC_TEMPORARY_PER_OUTPUT_BYTE)?,
    )
}

/// Audited contracts for specific operations. `required_outputs` follows this
/// node's output order, after cache cuts and region resolution. The caller
/// supplies the first input's resolved extent. None means no audited contract.
pub(super) fn estimate(
    operation: &Operation,
    required_outputs: &[bool],
    input_extent: Option<[u32; 2]>,
) -> Option<GpuAllocation> {
    match (operation, required_outputs) {
        (
            Operation::ImageAlias { .. }
            | Operation::LinearCodeBase
            | Operation::LinearCodeRgb { .. },
            [true],
        ) => Some(GpuAllocation::default()),
        (Operation::Texture { params, prepared }, [true, false]) => prepared.allocation(
            *params,
            texture::Requested {
                layers: true,
                change: false,
            },
        ),
        (Operation::TiledColor { params, .. }, [true, false]) => {
            reconstruction::color_allocation(*params)
        }
        (Operation::TiledColorDq { params, table, .. }, [true, false]) => {
            reconstruction::color_dq_allocation(*params, table)
        }
        (Operation::ProjectColorTiles { params, .. }, [true]) => {
            reconstruction::project_color_allocation(*params)
        }
        (Operation::ResampleRgb { params, .. }, [true]) => resize::allocation(params),
        (Operation::Tone { curve, .. }, [true]) => {
            super::execute::tone_allocation(input_extent?, curve.len())
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "allocation_test.rs"]
mod tests;
