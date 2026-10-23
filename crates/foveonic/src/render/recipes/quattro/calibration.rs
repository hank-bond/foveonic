//! Resolves Quattro extraction, phase gain and selected rendering color calibration.

use super::resources::ResourceDirectory;
use crate::{
    nodes::sensor::{column_gain, phase_extraction, phase_gain},
    source::{Metadata, Result},
};

const DEFAULT_RAW_OFFSET: f64 = 2048.;
const REFERENCE_SENSOR_ISO: f64 = 100.;
// Offset-subtracted camera codes before sensor-ISO scaling. This is a recovered
// processing ceiling, not a measured physical sensor saturation level.
const BLUE_CLIP_AT_UNIT_SCALE: f32 = 14300.;
// The blue-map operation uses this fixed cutoff on its already-scaled input.
const PHASE_GAIN_CUTOFF: f32 = 14300.;

/// Ordinary full-size DP cameras share plane geometry. Calibration tables still
/// come from the individual model and file.
pub(super) fn is_ordinary_dp(metadata: &Metadata) -> bool {
    supports_extraction(metadata)
        && metadata
            .scalar("CAMERAID")
            .is_ok_and(|id| matches!(id, 80. | 81. | 82. | 83.))
        && [metadata.width, metadata.height] == [5888, 3672]
}

pub(super) fn ordinary_dp_extent(metadata: &Metadata) -> Result<[u32; 2]> {
    if !is_ordinary_dp(metadata) {
        return Err("this operation requires ordinary full-size DP Quattro input".into());
    }
    Ok([(metadata.width / 2) as u32, (metadata.height / 2) as u32])
}

/// The interchangeable-lens model has additional masked rows and its own
/// calibration stages, followed by the ordinary reconstruction geometry.
pub(super) fn is_ordinary_sd(metadata: &Metadata) -> bool {
    supports_extraction(metadata)
        && metadata.scalar("CAMERAID").is_ok_and(|id| id == 40.)
        && [metadata.width, metadata.height] == [5888, 3776]
}

pub(super) fn is_interchangeable(metadata: &Metadata) -> bool {
    is_ordinary_sd(metadata) || super::tiger::supported(metadata)
}

pub(super) fn full_extent(metadata: &Metadata) -> Result<[u32; 2]> {
    if !is_ordinary_dp(metadata) && !is_ordinary_sd(metadata) && !super::tiger::supported(metadata)
    {
        return Err("this operation requires supported full-size Quattro input".into());
    }
    Ok([(metadata.width / 2) as u32, (metadata.height / 2) as u32])
}

/// Version-three extraction and the calibrated Tiger variant have explicit contracts.
pub(super) fn supports_extraction(metadata: &Metadata) -> bool {
    metadata.processing_version.is_some_and(|v| v[0] == 3) || super::tiger::supported(metadata)
}

pub(super) fn extraction(metadata: &Metadata) -> Result<phase_extraction::Params> {
    if !supports_extraction(metadata) {
        return Err("this sensor extraction variant is not implemented".into());
    }
    let offset = raw_offset(metadata)?;
    if !offset.is_finite() || offset < 0. || offset > f64::from(u16::MAX) || offset.fract() != 0. {
        return Err("invalid sensor raw offset".into());
    }
    let normalization = normalization(metadata)?;
    let scale = 1. / normalization;
    if super::tiger::supported(metadata) {
        return phase_extraction::Params::channel_calibrated(
            (metadata.width / 2) as u32,
            (metadata.height / 2) as u32,
            offset as u32,
            scale,
            super::tiger::digital_gains(metadata)?,
        );
    }
    phase_extraction::Params::calibrated(
        (metadata.width / 2) as u32,
        (metadata.height / 2) as u32,
        offset as u32,
        scale,
        BLUE_CLIP_AT_UNIT_SCALE * scale,
    )
}

pub(super) fn normalization(metadata: &Metadata) -> Result<f32> {
    let iso = scalar(metadata, "SensorISO", REFERENCE_SENSOR_ISO)?;
    let value = if iso == 0. {
        1.
    } else {
        (iso / REFERENCE_SENSOR_ISO) as f32
    };
    if !value.is_finite() || value <= 0. {
        return Err("invalid sensor-ISO normalization".into());
    }
    Ok(value)
}
fn raw_offset(metadata: &Metadata) -> Result<f64> {
    if metadata.processing_version == Some([3, 0, 0, 0]) {
        Ok(0.)
    } else {
        scalar(metadata, "RawOffset", DEFAULT_RAW_OFFSET)
    }
}

/// The first two stored versions correct eight interleaved column groups.
/// Missing or differently encoded tables take the native affine-only fallback.
pub(super) fn column_gain(metadata: &Metadata) -> Result<Option<column_gain::Prepared>> {
    if !matches!(
        metadata.processing_version,
        Some([3, 0, 0, 0] | [3, 0, 0, 1])
    ) {
        return Ok(None);
    }
    let Some(table) = metadata.matrix("ColumnGains_HI") else {
        return Ok(None);
    };
    if table.encoding != crate::source::MatrixEncoding::Float32 {
        return Ok(None);
    }
    let width = metadata.width / 2;
    if width == 0
        || !width.is_multiple_of(8)
        || table.dimensions != [6, width]
        || table.values.len() != width * 6
    {
        return Err("invalid column gain table dimensions".into());
    }
    // Native group order is [1,5,2,6,3,7,4,8]. This inverse maps physical
    // segments to their interleaved metadata group.
    let groups = [0usize, 2, 4, 6, 1, 3, 5, 7];
    let mut gains = Vec::with_capacity(width * 6);
    for x in 0..width {
        let group = groups[x / (width / 8)];
        let index = group + 8 * (x % (width / 8));
        for channel in 0..6 {
            let gain = table.values[channel * width + index] as f32;
            let factor = (1. - gain) / (1. + gain);
            if !factor.is_finite() {
                return Err("invalid column gain coefficient".into());
            }
            gains.push(factor);
        }
    }
    Ok(Some(column_gain::Prepared::new(
        [u32::try_from(width)?, u32::try_from(metadata.height / 2)?],
        raw_offset(metadata)? as u32,
        gains,
    )?))
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

/// Resolves an edit choice against the actual camera catalogue. Daylight and
/// Sunlight name the same preset in the application and CAMF respectively.
pub(super) fn balance_alias<'a>(
    list: &'a crate::source::PropertyList,
    selected: &str,
) -> Result<&'a str> {
    list.names()
        .find(|name| name.eq_ignore_ascii_case(selected))
        .or_else(|| {
            let synonym = if selected.eq_ignore_ascii_case("Daylight") {
                "Sunlight"
            } else if selected.eq_ignore_ascii_case("Sunlight") {
                "Daylight"
            } else {
                return None;
            };
            list.names().find(|name| name.eq_ignore_ascii_case(synonym))
        })
        .ok_or_else(|| format!("missing selected white balance {selected}").into())
}

/// Compose the selected WB and mode matrix in the reconstruction's code basis.
/// Keep the narrowed ISO factors separate to preserve reference rounding.
pub(super) fn color_matrix(metadata: &Metadata, selected: &str, mode: &str) -> Result<[f64; 9]> {
    let capture_iso = metadata.scalar("CaptureISO")?;
    if !capture_iso.is_finite() || capture_iso <= 0. {
        return Err("invalid raw-color capture ISO".into());
    }
    let iso_gain = (capture_iso / REFERENCE_SENSOR_ISO) as f32;
    let inverse_iso = 1. / iso_gain;
    let corrections = metadata
        .lists
        .get("WhiteBalanceColorCorrections")
        .ok_or("missing white-balance color corrections")?;
    let name = corrections
        .get(balance_alias(corrections, selected)?)
        .unwrap();
    let matrix = metadata.fixed::<9>(name)?;
    if matrix.iter().any(|v| !v.is_finite() || v.abs() < 1e-9) {
        return Err("unsupported raw-color matrix validation fallback".into());
    }
    Ok(crate::nodes::color::color_mode::prepare(metadata, mode)?
        .camera_matrix(matrix.map(|value| value * f64::from(iso_gain)))
        .map(|value| value * f64::from(inverse_iso)))
}

/// Selected gains after camera temperature adjustment and green normalization.
/// Recorded white balance remains owned by preprocessing, independently of this edit.
pub(super) fn white_balance(metadata: &Metadata, selected: &str) -> Result<[f64; 3]> {
    let list = metadata
        .lists
        .get("WhiteBalanceGains")
        .ok_or("missing white-balance gains")?;
    let gain_name = list.get(balance_alias(list, selected)?).unwrap();
    let raw = metadata.fixed::<3>(gain_name)?;
    if raw.iter().any(|v| !v.is_finite() || v.abs() < 1e-9) {
        return Err("invalid selected white-balance gains".into());
    }
    let mut temperature = metadata
        .matrix("TempGainFact")
        .map(|m| m.values.as_slice())
        .unwrap_or(&[1.; 3])
        .to_vec();
    if temperature.len() != 3
        || temperature
            .iter()
            .any(|v| !v.is_finite() || !(0.5..=1.5).contains(v))
    {
        temperature = vec![1.; 3];
    }
    let adjusted = std::array::from_fn::<_, 3, _>(|i| raw[i] * temperature[i]);
    Ok(adjusted.map(|v| v / adjusted[1]))
}

fn scalar(metadata: &Metadata, name: &str, fallback: f64) -> Result<f64> {
    if metadata.matrix(name).is_some() {
        metadata.scalar(name)
    } else {
        Ok(fallback)
    }
}
