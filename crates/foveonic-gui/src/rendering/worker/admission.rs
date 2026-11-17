//! Estimates coarse workspace headroom for admitting additional warm Merrill photographs.

use foveonic::{
    render::{native_extent, ProcessingQuality, Settings},
    source::{Metadata, ProcessingFamily},
};

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
const NATIVE_SIDE: u64 = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WorkspaceHint {
    pub bytes: u64,
    pub native: bool,
}

/// Coarse experimental headroom, not an allocation bound. Retained inputs are
/// already included in the worker's actual memory inventory.
pub(super) fn workspace(settings: &Settings, metadata: &Metadata) -> Option<WorkspaceHint> {
    if metadata.processing_family().ok()? != ProcessingFamily::Merrill {
        return None;
    }
    let sensor = [
        u32::try_from(metadata.width).ok()?,
        u32::try_from(metadata.height).ok()?,
    ];
    let extent = native_extent(metadata).ok()?;
    if sensor.contains(&0) || extent.contains(&0) {
        return None;
    }
    if settings.working_scale.is_native() {
        let [left, top, right, bottom] = settings.preview_region?;
        let width = right.checked_sub(left)?;
        let height = bottom.checked_sub(top)?;
        hints_for_grid(
            [u64::from(width), u64::from(height)],
            true,
            settings.processing_quality,
        )
    } else {
        if settings.preview_region.is_some() {
            return None;
        }
        let extent = settings.working_scale.extent(extent);
        hints_for_grid(extent.map(u64::from), false, settings.processing_quality)
    }
}

fn hints_for_grid(
    [width, height]: [u64; 2],
    native: bool,
    quality: ProcessingQuality,
) -> Option<WorkspaceHint> {
    if width == 0 || height == 0 {
        return None;
    }
    let area = width.checked_mul(height)?;
    let bytes = if native {
        let base = match quality {
            ProcessingQuality::Speed => 256 * MIB,
            ProcessingQuality::Quality => 384 * MIB,
        };
        // The requested processing rectangle includes overscan. The minimum
        // covers measured broader kernel support. Axis terms keep long, narrow
        // regions from receiving only the small-area allowance.
        let units = area
            .max(width.checked_mul(NATIVE_SIDE)?)
            .max(height.checked_mul(NATIVE_SIDE)?)
            .max(NATIVE_SIDE * NATIVE_SIDE);
        ceil_scaled_bytes(base, units, NATIVE_SIDE * NATIVE_SIDE)?
    } else {
        // Eight RGBA32 images plus a small fixed workspace allowance.
        area.checked_mul(16 * 8)?.checked_add(16 * MIB)?
    };
    Some(WorkspaceHint { bytes, native })
}

fn ceil_scaled_bytes(base: u64, units: u64, denominator: u64) -> Option<u64> {
    let whole = base.checked_mul(units / denominator)?;
    let remainder = base.checked_mul(units % denominator)?;
    whole.checked_add(remainder.div_ceil(denominator))
}

/// Admit only additional work. The first mandatory photograph may exceed this
/// target. The native capacity floor is a total, not extra retained bytes.
pub(super) fn fits(target: u64, actual: u64, headroom: u64, native_count: usize) -> bool {
    let Some(workspace_total) = actual.checked_add(headroom) else {
        return false;
    };
    let Some(native_floor) = u64::try_from(native_count)
        .ok()
        .and_then(|count| count.checked_mul(GIB))
    else {
        return false;
    };
    workspace_total.max(native_floor) <= target
}

#[cfg(test)]
#[path = "admission_test.rs"]
mod tests;
