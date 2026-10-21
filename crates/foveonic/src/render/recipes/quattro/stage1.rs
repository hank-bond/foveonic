//! Prepares the ordinary full-size two-pass noise controls from DP2 Quattro metadata.

use crate::source::{MatrixEncoding, Metadata, Result};

const DP2_CAMERA_ID: f64 = 81.;
const QUATTRO_PROCESS_MAJOR: u16 = 3;
const DP2_SENSOR_EXTENT: [usize; 2] = [5888, 3672];
const DP2_RGB_EXTENT: [u32; 2] = [2944, 1836];
const MAXIMUM_NOISE_STEP: u8 = 4;
const REFERENCE_ISO: f64 = 100.;
const MAXIMUM_STRENGTH_FIT_ISO: f64 = 6400.;

// CAMF versions are major*1,000,000 + minor*1,000 + revision.
const FIRST_SUPPORTED_GAIN_VERSION: f64 = 2000.;
// These score controls retain the stored shutter and temperature units.
const TEMPERATURE_SCORE_SCALE: f32 = 0.0866434;
const SHUTTER_SCORE_DIVISOR: f32 = 1000.;
const LONG_EXPOSURE_SCORE: f32 = 160_000.;
// Multiplier, score span and offset of the clamped minimum-blend ramp.
const LONG_EXPOSURE_BLEND_RAMP: [f32; 3] = [0.8, 320_000., 0.2];

// Index each pass's strength by the discrete luma-noise step.
const FIRST_PASS_STRENGTHS: [f32; 5] = [1., 0.5, 0.1, 0.05, 0.02];
const SECOND_PASS_STRENGTHS: [f32; 5] = [1.5, 1., 0.5, 0.1, 0.06];
const LONG_EXPOSURE_FIRST_PASS_STRENGTHS: [f32; 5] = [0.5, 0.25, 0.05, 0.025, 0.01];

// Stored ambient-code origin, EV per code and EV at that origin.
const AMBIENT_EV_MAPPING: [f64; 3] = [48., 0.125, 5.];
const DEFAULT_AMBIENT_MEAN: f64 = 120.;
const MAXIMUM_AMBIENT_ADJUSTMENT_EV: f64 = 3.;

/// Discrete positions of the three reference noise controls, from zero through four.
#[derive(Clone, Copy, Debug)]
pub(super) struct NoiseSteps {
    pub chroma: u8,
    pub luma: u8,
    pub residual: u8,
}

/// Numerical inputs to the ordinary two-pass filter, independent of camera identity.
/// Input and output are low-resolution calibrated RGB in camera codes, without WB applied.
#[derive(Clone, Debug)]
pub(super) struct Stage1Controls {
    pub extent: [u32; 2],
    pub white_balance: [f64; 3],
    pub estimated_iso: u32,
    pub bilateral_strengths: [f32; 2],
    pub long_exposure: bool,
    pub minimum_blend: f32,
}

/// Prepare the demonstrated single-input HI color route. This does not perform filtering.
/// Chroma and residual steps affect other noise descriptor consumers. This particular
/// helper replaces its noise model and uses only the luma step to select its strengths.
pub(super) fn prepare(metadata: &Metadata, steps: NoiseSteps) -> Result<Stage1Controls> {
    if [steps.chroma, steps.luma, steps.residual]
        .into_iter()
        .any(|v| v > MAXIMUM_NOISE_STEP)
    {
        return Err("noise control steps must be between zero and four".into());
    }
    if metadata.scalar("CAMERAID")? != DP2_CAMERA_ID
        || metadata
            .processing_version
            .is_none_or(|v| v[0] != QUATTRO_PROCESS_MAJOR)
        || [metadata.width, metadata.height] != DP2_SENSOR_EXTENT
    {
        return Err("Stage1 control preparation currently requires full-size DP2 Quattro".into());
    }
    let version = metadata.fixed::<3>("CamsysVersion")?;
    if version[0] * 1_000_000. + version[1] * 1000. + version[2] < FIRST_SUPPORTED_GAIN_VERSION
        || optional(metadata, "ISOrange", 0.)? != 0.
    {
        return Err("unsupported Stage1 legacy gain or ISO override".into());
    }
    let capture_iso = positive(metadata.scalar("CaptureISO")?, "CaptureISO")?;
    let iso_gain = (capture_iso / REFERENCE_ISO) as f32;
    let white_balance = sunlight(metadata)?;
    let estimate = estimated_iso(metadata, capture_iso, iso_gain)?;
    let shutter = finite(metadata.scalar("CaptureShutter")?, "CaptureShutter")? as f32;
    let temperature = finite(metadata.scalar("SensorTemperature")?, "SensorTemperature")? as f32;
    // CaptureShutter retains its CAMF units here, independently of Exif seconds.
    let score = (temperature * TEMPERATURE_SCORE_SCALE).exp() * (shutter / SHUTTER_SCORE_DIVISOR);
    if !score.is_finite() {
        return Err("nonfinite Stage1 exposure score".into());
    }
    let index = usize::from(steps.luma);
    let long_exposure = score > LONG_EXPOSURE_SCORE;
    let mut first = FIRST_PASS_STRENGTHS;
    let mut second = SECOND_PASS_STRENGTHS;
    let minimum_blend = if long_exposure {
        first = LONG_EXPOSURE_FIRST_PASS_STRENGTHS;
        // Long-exposure second-pass strength fit over log2(ISO/reference ISO).
        // Preserve Float32 logarithm/narrowing and the subsequent double fused order.
        let iso_stops = ((capture_iso.clamp(REFERENCE_ISO, MAXIMUM_STRENGTH_FIT_ISO) as f32
            / REFERENCE_ISO as f32)
            .ln() as f64
            / std::f64::consts::LN_2) as f32 as f64;
        let polynomial = 0.000202_f64.mul_add(iso_stops.powi(4), iso_stops.powi(3) * -0.002618);
        let polynomial = 0.009848_f64.mul_add(iso_stops * iso_stops, polynomial);
        let polynomial = (-0.002074_f64).mul_add(iso_stops, polynomial) + 0.007;
        // The last two scale entries originate as Float32 values before widening.
        let fit_scales = [5., 2.5, 1., f64::from(0.4_f32), f64::from(0.166_f32)];
        second[index] = (polynomial * fit_scales[index]) as f32;
        let [gain, span, offset] = LONG_EXPOSURE_BLEND_RAMP;
        (((score - LONG_EXPOSURE_SCORE) * gain / span) + offset).clamp(0., 1.)
    } else {
        // The flag controls whether the helper uses this stored value.
        1.
    };
    Ok(Stage1Controls {
        extent: DP2_RGB_EXTENT,
        white_balance,
        estimated_iso: estimate,
        bilateral_strengths: [first[index], second[index]],
        long_exposure,
        minimum_blend,
    })
}

fn sunlight(metadata: &Metadata) -> Result<[f64; 3]> {
    let list = metadata
        .lists
        .get("WhiteBalanceGains")
        .ok_or("missing white-balance gains")?;
    let gain_name = list
        .get("Sunlight")
        .or_else(|| list.get("Daylight"))
        .ok_or("missing Sunlight gains")?;
    let raw = metadata.fixed::<3>(gain_name)?;
    if raw.iter().any(|v| !v.is_finite() || v.abs() < 1e-9) {
        return Err("invalid Sunlight gains".into());
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

fn estimated_iso(metadata: &Metadata, capture_iso: f64, iso_gain: f32) -> Result<u32> {
    let aperture = photographic_property(metadata, "APERTURE")?;
    let shutter = photographic_property(metadata, "SHUTTER")?;
    let mut reference_ev =
        ((aperture * aperture / shutter).ln() - f64::from(iso_gain.ln())) / std::f64::consts::LN_2;
    if !reference_ev.is_finite() {
        reference_ev = 0.;
    }
    let mean = if let Some(ambient) = metadata.matrix("AESensorValueAmbient") {
        if ambient.encoding != MatrixEncoding::Int16 || ambient.values.is_empty() {
            return Err("unsupported ambient exposure table".into());
        }
        if ambient.values.iter().any(|v| !v.is_finite()) {
            return Err("invalid ambient exposure table".into());
        }
        ambient.values.iter().sum::<f64>() / ambient.values.len() as f64
    } else {
        DEFAULT_AMBIENT_MEAN
    };
    let [code_origin, ev_per_code, ev_at_origin] = AMBIENT_EV_MAPPING;
    let actual_ev = (mean - code_origin) * ev_per_code + ev_at_origin;
    let difference = reference_ev - actual_ev;
    let base = optional(metadata, "ExpEstimate", 0.)? + optional(metadata, "ExpNet", 0.)?;
    let mut adjustment = base + if reference_ev < 0. { difference } else { 0. };
    if actual_ev < optional(metadata, "EstimatedISOFlashAmbient", 8.)?
        && difference > optional(metadata, "EstimatedISODiffEV", 3.)?
    {
        adjustment = base + difference.min(MAXIMUM_AMBIENT_ADJUSTMENT_EV);
    }
    let estimate =
        f64::from((capture_iso / f64::from(adjustment.exp2() as f32)) as f32).max(capture_iso);
    if !estimate.is_finite() || estimate > u32::MAX as f64 {
        return Err("invalid estimated ISO".into());
    }
    Ok(estimate as u32)
}

fn photographic_property(metadata: &Metadata, name: &str) -> Result<f64> {
    let value = metadata
        .properties
        .get(name)
        .ok_or_else(|| format!("missing capture {name}"))?
        .parse::<f64>()?;
    positive(value, name)
}
fn optional(metadata: &Metadata, name: &str, default: f64) -> Result<f64> {
    match metadata.matrix(name) {
        Some(_) => finite(metadata.scalar(name)?, name),
        None => Ok(default),
    }
}
fn finite(value: f64, name: &str) -> Result<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("nonfinite {name}").into())
    }
}
fn positive(value: f64, name: &str) -> Result<f64> {
    if value.is_finite() && value > 0. {
        Ok(value)
    } else {
        Err(format!("invalid {name}").into())
    }
}

#[cfg(test)]
#[path = "stage1_test.rs"]
mod tests;
