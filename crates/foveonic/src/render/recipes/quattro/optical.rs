//! Prepares the DP1 optical color correction independently of graph construction.

use crate::{
    nodes::color::purple_fringe::Prepared,
    source::{Metadata, Result},
};

const DP1_CAMERA_ID: f64 = 80.;
const REFERENCE_ISO: f64 = 100.;

pub(super) fn prepare(metadata: &Metadata) -> Result<Option<Prepared>> {
    if metadata.scalar("CAMERAID")? != DP1_CAMERA_ID {
        return Ok(None);
    }
    let extent = super::calibration::ordinary_dp_extent(metadata)?;
    let area = metadata.fixed::<4>("ActiveImageArea")?;
    if area
        .iter()
        .any(|v| !v.is_finite() || *v < 0. || v.fract() != 0. || *v > u32::MAX as f64)
    {
        return Err("invalid purple-fringe active rectangle".into());
    }
    // This callback uses exclusive right and inclusive bottom coordinates.
    let rectangle = [
        area[0] as u32 / 2,
        area[1] as u32 / 2,
        (area[2] as u32).div_ceil(2),
        (area[3] as u32).div_ceil(2),
    ];
    let capture_iso = metadata.scalar("CaptureISO")?;
    if !capture_iso.is_finite() || capture_iso <= 0. {
        return Err("invalid optical correction capture ISO".into());
    }
    // GatherParams scales the cached Auto matrix before the preprocessing
    // request supplies it to this wrapper. The WB triple is a separate input.
    let iso_gain = (capture_iso / REFERENCE_ISO) as f32;
    let matrix = metadata
        .fixed::<9>("AutoCCMatrix")?
        .map(|v| v * f64::from(iso_gain));
    let gains = super::defects::auto_white_balance(metadata)?.map(|v| v as f32);
    Prepared::new(extent, rectangle, gains, matrix).map(Some)
}
