//! Prepares metadata controls for signed drift, shield offsets, and polynomial correction.

use super::reference::*;

use crate::{
    nodes::sensor::fixed_point::{
        Correction, COLOR_AVERAGE, EXTRA_DRIFT, POST_POLYNOMIAL, ROW_OFFSETS,
    },
    source::{Metadata, Result},
};

/// Resolve functional correction controls without making the GPU operation aware
/// of camera models. Image-dependent shield measurements remain on the GPU.
pub(crate) fn prepare_metadata(metadata: &Metadata, correction: &mut Correction) -> Result<()> {
    let scalar = |name: &str, fallback: f64| -> Result<f64> {
        if metadata.matrix(name).is_some() {
            metadata.scalar(name)
        } else {
            Ok(fallback)
        }
    };
    let vector = |name: &str, count: usize| -> Result<Vec<f64>> {
        if metadata.matrix(name).is_some() {
            let values = metadata.vector(name)?;
            if values.len() != count {
                return Err(format!("invalid {name} correction dimensions").into());
            }
            Ok(values.to_vec())
        } else {
            Ok(vec![0.; count])
        }
    };
    let pair = |value: f64| [value as f32, (value - f64::from(value as f32)) as f32];
    let width = metadata.width as u32;
    let height = metadata.height as u32;
    let origin = vector("KeepImageArea", 4)?;
    let estimate =
        scalar("UsePreComputedDrift", 0.)? == 0. || scalar("EnableMovieDriftCorrection", 0.)? != 0.;
    let preserve_slope = scalar("UseSlopeInDrift", 1.)? != 0.;
    // Stored drift interleaves slope and intercept by channel. The GPU receives
    // complete RGB slope and intercept vectors for each shield.
    let mut drift = [0.; 12];
    if estimate {
        for (i, name) in ["DarkShieldTop", "DarkShieldBottom"].iter().enumerate() {
            let rectangle = vector(name, 4)?;
            correction.params.shields[i] = [
                (rectangle[0] - origin[0]).clamp(0., f64::from(width)) as u32,
                (rectangle[1] - origin[1]).clamp(0., f64::from(height)) as u32,
                (rectangle[2] + 1. - origin[0]).clamp(0., f64::from(width)) as u32,
                (rectangle[3] + 1. - origin[1]).clamp(0., f64::from(height)) as u32,
            ];
        }
    } else {
        let stored = vector("DarkDrift", 12)?;
        for (i, index) in [0, 2, 4, 1, 3, 5, 6, 8, 10, 7, 9, 11]
            .into_iter()
            .enumerate()
        {
            drift[i] = stored[index];
        }
    }
    for (i, value) in drift.into_iter().enumerate() {
        let [hi, lo] = pair(value);
        correction.params.drift[i / 2][(i % 2) * 2] = hi;
        correction.params.drift[i / 2][(i % 2) * 2 + 1] = lo;
    }
    let spans = vector("DarkShieldColRange", 4)?;
    correction.params.row_spans =
        std::array::from_fn(|i| spans[i].clamp(1., f64::from(width - 1)) as u32);
    let average = scalar("UseColorAverageRowOffset", 0.)? != 0.;
    let default_scale = if average { 0.75 } else { 0.9 };
    let default_decay = if average { 13. } else { 22. };
    let mut scale = scalar("RowCorrectionGain", default_scale)?;
    let mut decay = scalar("RowCorrectionScale", default_decay)?;
    // This older metadata pair selects the reference defaults, rather than
    // literal gain .8 and decay12.
    if decay == 12. && (scale - 0.8).abs() < 0.01 {
        scale = default_scale;
        decay = default_decay;
    }
    let active = vector("ActiveImageArea", 4)?;
    let top = (active[1] - origin[1]).clamp(0., f64::from(height)) as u32;
    let bottom = if metadata.matrix("ActiveImageArea").is_some() {
        (active[3] + 1. - origin[1]).clamp(0., f64::from(height)) as u32
    } else {
        height
    };
    let row_enabled = spans[1] - spans[0] >= 3.
        && spans[3] - spans[2] >= 3.
        && scale > 0.
        && decay > 0.
        && bottom.saturating_sub(top) > 9;
    correction.params.drift_controls =
        [top, bottom, u32::from(estimate), u32::from(preserve_slope)];
    let [scale_hi, scale_lo] = pair(scale);
    let [alpha_hi, alpha_lo] = pair(if decay > 0. {
        1. - (-1. / decay).exp()
    } else {
        0.
    });
    correction.params.row_filter = [scale_hi, scale_lo, alpha_hi, alpha_lo];
    // The correction kernel consumes signed Q14 polynomial coefficients.
    let polynomial_scale = Q14_SCALE;
    let polynomial = vector("PostPolyMatrix", 27)?;
    for (i, coefficient) in polynomial.iter().enumerate() {
        correction.params.polynomial[i / 3][i % 3] = (coefficient * polynomial_scale) as i32;
    }
    // Bits select extra drift, row estimation, RGB projection, and polynomial.
    correction.params.controls[1] =
        (u32::from(estimate || drift.iter().any(|v| *v != 0.) || row_enabled) * EXTRA_DRIFT)
            | (u32::from(row_enabled) * ROW_OFFSETS)
            | (u32::from(average) * COLOR_AVERAGE)
            | (u32::from(polynomial.iter().any(|v| *v != 0.)) * POST_POLYNOMIAL);
    Ok(())
}
