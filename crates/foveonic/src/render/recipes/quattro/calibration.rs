//! Resolves Quattro camera calibration into numerical extraction and phase-gain parameters.

use super::resources::ResourceDirectory;
use crate::{
    nodes::sensor::{phase_extraction, phase_gain},
    source::{Metadata, Result},
};

const DEFAULT_RAW_OFFSET: f64 = 2048.;
const REFERENCE_SENSOR_ISO: f64 = 100.;
// Offset-subtracted camera codes before sensor-ISO scaling. This is a recovered
// processing ceiling, not a measured physical sensor saturation level.
const BLUE_CLIP_AT_UNIT_SCALE: f32 = 14300.;
// The blue-map operation uses this fixed cutoff on its already-scaled input.
const PHASE_GAIN_CUTOFF: f32 = 14300.;

/// Older versions need column correction. Tiger needs a separate gain-bearing route.
pub(super) fn supports_extraction(metadata: &Metadata) -> bool {
    metadata
        .processing_version
        .is_some_and(|v| v[0] == 3 && !matches!(v, [3, 0, 0, 0] | [3, 0, 0, 1]))
}

pub(super) fn extraction(metadata: &Metadata) -> Result<phase_extraction::Params> {
    if !supports_extraction(metadata) {
        return Err("this sensor extraction variant is not implemented".into());
    }
    let offset = scalar(metadata, "RawOffset", DEFAULT_RAW_OFFSET)?;
    if !offset.is_finite() || offset < 0. || offset > f64::from(u16::MAX) || offset.fract() != 0. {
        return Err("invalid sensor raw offset".into());
    }
    let sensor_iso = scalar(metadata, "SensorISO", REFERENCE_SENSOR_ISO)?;
    let normalization = if sensor_iso == 0. {
        1.
    } else {
        (sensor_iso / REFERENCE_SENSOR_ISO) as f32
    };
    let scale = 1. / normalization;
    phase_extraction::Params::calibrated(
        (metadata.width / 2) as u32,
        (metadata.height / 2) as u32,
        offset as u32,
        scale,
        BLUE_CLIP_AT_UNIT_SCALE * scale,
    )
}

pub(super) fn phase_gain(
    metadata: &Metadata,
    resources: Option<&ResourceDirectory>,
) -> Result<phase_gain::Prepared> {
    let resources = resources.ok_or(
        "camera calibration maps are required for phase-gain.blue. Supply a calibration resource directory",
    )?;
    let map = resources
        .prepare(metadata)?
        .ok_or("this camera has no phase gain map")?;
    // This correction tests normalized input against the native fixed threshold.
    // Extraction's upper bound instead scales with sensor ISO.
    phase_gain::Prepared::new(
        [metadata.width as u32, metadata.height as u32],
        map.extent,
        map.values,
        PHASE_GAIN_CUTOFF,
    )
}

fn scalar(metadata: &Metadata, name: &str, fallback: f64) -> Result<f64> {
    if metadata.matrix(name).is_some() {
        metadata.scalar(name)
    } else {
        Ok(fallback)
    }
}
