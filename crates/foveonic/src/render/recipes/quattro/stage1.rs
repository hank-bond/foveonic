//! Prepares ordinary full-size Quattro two-pass noise controls and exposure-guided filtering.

use crate::source::{MatrixEncoding, Metadata, Result};

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

// Index each pass's strength by the discrete chroma-noise step.
const FIRST_PASS_STRENGTHS: [f32; 5] = [1., 0.5, 0.1, 0.05, 0.02];
const SECOND_PASS_STRENGTHS: [f32; 5] = [1.5, 1., 0.5, 0.1, 0.06];
const LONG_EXPOSURE_FIRST_PASS_STRENGTHS: [f32; 5] = [0.5, 0.25, 0.05, 0.025, 0.01];

// Stored ambient-code origin, EV per code and EV at that origin.
const AMBIENT_EV_MAPPING: [f64; 3] = [48., 0.125, 5.];
const DEFAULT_AMBIENT_MEAN: f64 = 120.;
const MAXIMUM_AMBIENT_ADJUSTMENT_EV: f64 = 3.;

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
/// The color chroma slider selects these strengths. The reference descriptor's
/// internal slider order differs from its public adjustment order: public chroma
/// is at packed offset 0xf0, while public luminance is at 0xf8. This helper
/// replaces the descriptor's noise model before filtering.
pub(super) fn prepare(metadata: &Metadata, chroma_step: u8) -> Result<Stage1Controls> {
    if chroma_step > MAXIMUM_NOISE_STEP {
        return Err("chroma noise step must be between zero and four".into());
    }
    let extent = super::calibration::ordinary_extent(metadata)?;
    let version = metadata.fixed::<3>("CamsysVersion")?;
    if version[0] * 1_000_000. + version[1] * 1000. + version[2] < FIRST_SUPPORTED_GAIN_VERSION
        || optional(metadata, "ISOrange", 0.)? != 0.
    {
        return Err("unsupported Stage1 legacy gain or ISO override".into());
    }
    let capture_iso = positive(metadata.scalar("CaptureISO")?, "CaptureISO")?;
    let iso_gain = (capture_iso / REFERENCE_ISO) as f32;
    // Noise filtering uses the fixed daylight calibration even when the edit
    // selects another illuminant. Stage2 owns the selected rendering balance.
    let white_balance = super::calibration::white_balance(metadata, "Sunlight")?;
    let estimate = estimated_iso(metadata, capture_iso, iso_gain)?;
    let shutter = finite(metadata.scalar("CaptureShutter")?, "CaptureShutter")? as f32;
    let temperature = finite(metadata.scalar("SensorTemperature")?, "SensorTemperature")? as f32;
    // CaptureShutter retains its CAMF units here, independently of Exif seconds.
    let score = (temperature * TEMPERATURE_SCORE_SCALE).exp() * (shutter / SHUTTER_SCORE_DIVISOR);
    if !score.is_finite() {
        return Err("nonfinite Stage1 exposure score".into());
    }
    let index = usize::from(chroma_step);
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
        extent,
        white_balance,
        estimated_iso: estimate,
        bilateral_strengths: [first[index], second[index]],
        long_exposure,
        minimum_blend,
    })
}

fn estimated_iso(metadata: &Metadata, capture_iso: f64, iso_gain: f32) -> Result<u32> {
    let aperture = photographic_property(metadata, "APERTURE")?;
    let shutter = photographic_property(metadata, "SHUTTER")?;
    let mut reference_ev =
        ((aperture * aperture / shutter).ln() - f64::from(iso_gain.ln())) / std::f64::consts::LN_2;
    if !reference_ev.is_finite() {
        reference_ev = 0.;
    }
    let mean = ambient_mean(metadata)?;
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

fn ambient_mean(metadata: &Metadata) -> Result<f64> {
    let sd = metadata.scalar("CAMERAID")? == 40.;
    let version = metadata.fixed::<3>("CamsysVersion")?;
    let version = version[0] * 1_000_000. + version[1] * 1000. + version[2];
    if sd && version < 1_000_000. {
        return Ok(DEFAULT_AMBIENT_MEAN);
    }
    if sd && version > 1_000_000. {
        let Some(ambient) = metadata.matrix("AESensorValueAmbient256") else {
            return Ok(DEFAULT_AMBIENT_MEAN);
        };
        if ambient.encoding != MatrixEncoding::Uint8
            || ambient.values.len() != 256
            || ambient
                .values
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=255.).contains(v) || v.fract() != 0.)
        {
            return Err("invalid sd ambient exposure map".into());
        }
        // The fixed 16×16 to 11×7 reference map truncates both interpolation
        // fractions to zero. These are its selected source coordinates.
        const COLUMNS: [usize; 11] = [0, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15];
        const ROWS: [usize; 7] = [0, 3, 5, 8, 10, 13, 15];
        let sum: u32 = ROWS
            .into_iter()
            .flat_map(|y| COLUMNS.map(|x| (ambient.values[y * 16 + x] as u32) >> 2))
            .sum();
        return Ok(f64::from(sum) / 77.);
    }
    let Some(ambient) = metadata.matrix("AESensorValueAmbient") else {
        return Ok(DEFAULT_AMBIENT_MEAN);
    };
    if ambient.encoding != MatrixEncoding::Int16
        || ambient.values.is_empty()
        || ambient.values.iter().any(|v| !v.is_finite())
    {
        return Err("invalid ambient exposure table".into());
    }
    let divisor = if sd { 77. } else { ambient.values.len() as f64 };
    Ok(ambient.values.iter().sum::<f64>() / divisor)
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

// The ordinary helper replaces metadata noise coefficients with this fixed model.
// Rows are constant, linear and quadratic terms in R/G/B order.
const FILTER_NOISE_MODEL: [f64; 9] = [
    97.448, 60.657, 5.21075, 0.1934, 0.11, 0.10855, -0.0000005, -0.0000002, 0.0000002,
];
const FILTER_VARIANCE_MIX: [f64; 9] = [
    0.1440004023245225,
    0.3828545816329895,
    0.014439858093753227,
    0.10394208867656735,
    0.35594662117735154,
    0.01666031421879885,
    0.09933340423738898,
    0.3828538306536156,
    0.10021776990222239,
];

/// Prepares the two-pass numerical filter. Its input must be the complete
/// preprocessing result; extraction or HLP calibration alone is insufficient.
pub(super) fn prepare_filter(
    metadata: &Metadata,
    chroma_step: u8,
) -> Result<crate::nodes::color::two_scale_noise::Prepared> {
    numerical(&prepare(metadata, chroma_step)?)
}

fn numerical(controls: &Stage1Controls) -> Result<crate::nodes::color::two_scale_noise::Prepared> {
    use crate::nodes::color::{rgb_prefilter, two_scale_noise};
    let iteration = |index: usize| -> Result<rgb_prefilter::Prepared> {
        let (intensity, range) = conditioning(
            if index == 0 { 0.3 } else { 0.75 },
            index == 1 && controls.long_exposure,
        );
        let extent = if index == 0 {
            controls.extent
        } else {
            controls.extent.map(|v| v / 2)
        };
        let scale = if index == 0 { 1. } else { 0.25 };
        rgb_prefilter::Prepared::new(
            extent,
            controls.white_balance,
            FILTER_NOISE_MODEL.map(|v| v * scale),
            FILTER_VARIANCE_MIX,
            intensity,
            range,
            2047. / (8. / controls.bilateral_strengths[index]),
            if index == 0 { 5 } else { 15 },
        )
    };
    // The selected outer caller supplies three unit highlight-preservation gains.
    let prepared =
        two_scale_noise::Prepared::new(iteration(0)?, iteration(1)?, brightness(), [1.; 3])?;
    if controls.estimated_iso < 400 {
        return Ok(prepared);
    }
    // The wrapper undoes the Double WB noise scaling before edge guidance.
    let model = std::array::from_fn(|i| {
        let gain = controls.white_balance[i % 3];
        match i / 3 {
            0 => (FILTER_NOISE_MODEL[i] * (gain * gain)) / (gain * gain),
            1 => (FILTER_NOISE_MODEL[i] * gain) / gain,
            _ => FILTER_NOISE_MODEL[i],
        }
    });
    let tanh = (0..1024)
        .map(|i| ((i as f32 / -1023.) * 3.).tanh().clamp(-1., 1.))
        .collect();
    prepared.with_adaptive(
        [model, model.map(|v| v * 0.25)],
        tanh,
        if controls.long_exposure {
            controls.minimum_blend
        } else {
            0.
        },
    )
}

pub(super) fn conditioning(cutoff: f32, special: bool) -> (Vec<f32>, Vec<f32>) {
    let (multiplier, slope) = if special {
        (5., 0.00034760847_f32)
    } else {
        (31., 0.0021551724_f32)
    };
    let mut intensity: Vec<_> = (0..14384)
        .map(|index| {
            let x = ((index as f32).mul_add(slope, 1.) - 1.) / multiplier;
            multiplier.mul_add(x.powf(4.), 1.)
        })
        .collect();
    intensity.push(multiplier + 1.);
    let range = (0..2048)
        .map(|index| {
            let value = (index as f32 * -0.0039081583_f32).exp();
            if value >= cutoff {
                value
            } else {
                0.
            }
        })
        .collect();
    (intensity, range)
}

fn brightness() -> Vec<f32> {
    (0..14344)
        .map(|index| {
            let x = ((index as f32).mul_add(-0.00020916127, 4.) - 1.) / 3.;
            (3_f32.mul_add(x.powf(64.), 1.) - 1.) / 3.
        })
        .collect()
}

#[cfg(test)]
#[path = "stage1_numerical_test.rs"]
mod numerical_tests;

#[cfg(test)]
#[path = "stage1_test.rs"]
mod tests;
