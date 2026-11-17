//! Bounds GPU allocations for audited resolved operations before additional work is admitted.

use crate::{
    graph::Operation,
    nodes::{color::reconstruction, color::texture, finish::resize},
    profile::Mode,
};

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

    pub(crate) fn total_bytes(self) -> Option<u64> {
        self.output_bytes.checked_add(self.temporary_bytes)
    }

    pub(crate) fn checked_add(self, other: Self) -> Option<Self> {
        Self::new(
            self.output_bytes.checked_add(other.output_bytes)?,
            self.temporary_bytes.checked_add(other.temporary_bytes)?,
        )
    }
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

/// `required_outputs` follows this node's output order, after cache cuts and
/// region resolution. The caller supplies the first input's resolved extent.
/// An unknown contract cannot admit additional work. It is not a zero estimate.
pub(super) fn estimate(
    operation: &Operation,
    required_outputs: &[bool],
    input_extent: Option<[u32; 2]>,
    mode: Mode,
    regional: bool,
) -> Option<GpuAllocation> {
    if mode != Mode::Off || regional {
        return None;
    }
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
