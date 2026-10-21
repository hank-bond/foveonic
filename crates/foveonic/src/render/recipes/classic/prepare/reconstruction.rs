//! Prepares Classic chroma smoothing and gain-preserving logarithmic reconstruction controls.

use super::super::reference::RECURSIVE_CHROMA_THRESHOLD;
use super::{capture_iso, scalar, vector, Metadata, Result};
use crate::nodes::color::{
    chroma::{self, Support},
    log_reconstruction,
};

pub(crate) fn prepare_chroma(
    metadata: &Metadata,
    gains: [f64; 3],
    monochrome: bool,
) -> Result<Vec<chroma::Prepared>> {
    let extent = [metadata.width as u32, metadata.height as u32];
    let iso = capture_iso(metadata)?;
    let mut stages = Vec::new();
    let blur = scalar(metadata, "ChromaBlur", 0.)?;
    if blur != 0. {
        let dq = if metadata.matrix("ChromaDQ").is_some() {
            metadata.fixed::<3>("ChromaDQ")?
        } else {
            [0.; 3]
        };
        let statistics = if !monochrome && scalar(metadata, "CCMatrix_Flag", 0.)? == 1. {
            1.3
        } else {
            1.
        };
        let mean = gains
            .iter()
            .zip(dq)
            .map(|(g, d)| statistics * g * d)
            .sum::<f64>()
            / 3.;
        let sensor = scalar(metadata, "SensorISO", 100.)?;
        let sensor = if sensor == 0. { 100. } else { sensor };
        let pre = (iso as f32 / sensor as f32).min(2.);
        let regularizer =
            (statistics * f64::from(pre * 10.)) as u32 * scalar(metadata, "SpatialDQ", 0.)? as u32;
        let shift = scalar(metadata, "ChromaSumShift", 3.)? as u32;
        for (amplitude, support) in [(mean * 2., Support::Binomial3), (mean, Support::Box5)] {
            if monochrome && matches!(support, Support::Box5) {
                continue;
            }
            let amplitude = if monochrome {
                amplitude * 2.
            } else {
                amplitude
            };
            let p = chroma::Prepared::new(
                extent,
                [amplitude; 3],
                [0.8; 3],
                regularizer,
                shift,
                support,
            )?;
            stages.push(p);
        }
    }
    Ok(stages)
}

/// Large chroma smoothing follows camera color and FIR reconstruction.
pub(crate) fn prepare_recursive_chroma(
    metadata: &Metadata,
    gains: [f64; 3],
) -> Result<Option<chroma::Prepared>> {
    if scalar(metadata, "ChromaBlur", 0.)? < RECURSIVE_CHROMA_THRESHOLD {
        return Ok(None);
    }
    let sensor = scalar(metadata, "SensorISO", 100.)?;
    let ratio = capture_iso(metadata)? as f32 / if sensor == 0. { 100. } else { sensor } as f32;
    let pre = ratio.min(2.);
    let dq = vector(metadata, "ChromaDQ", [0.; 3])?;
    // This late color-domain pass uses the full ISO ratio, independently of
    // the earlier sensor-domain statistics multiplier.
    let scale = f64::from(ratio + ratio) / 3.;
    let amplitudes = std::array::from_fn(|c| scale * (gains[c] / f64::from(pre)) * dq[c] * 0.5);
    let regularizer =
        (scalar(metadata, "SpatialDQ", 0.)? as u32).wrapping_mul((ratio * 10.) as u32) >> 1;
    Ok(Some(chroma::Prepared::new(
        [metadata.width as u32, metadata.height as u32],
        amplitudes,
        [0.8; 3],
        regularizer,
        scalar(metadata, "ChromaSumShift", 3.)? as u32,
        Support::Recursive,
    )?))
}

/// Preserve the reference color normalization when spatial filtering is bypassed.
pub(crate) fn prepare_reconstruction_gain(
    metadata: &Metadata,
    matrix_green_sum: f32,
    digital_gain: f32,
) -> Result<Option<crate::nodes::color::code_gain::Prepared>> {
    if scalar(metadata, "FIR_DisableStep", 0.)? != 0. {
        return Ok(None);
    }
    Ok(Some(crate::nodes::color::code_gain::Prepared::new(
        [metadata.width as u32, metadata.height as u32],
        log_reconstruction::output_gain(matrix_green_sum, digital_gain),
    )?))
}

pub(crate) fn prepare_reconstruction(
    metadata: &Metadata,
    matrix_green_sum: f32,
    digital_gain: f32,
) -> Result<Option<log_reconstruction::Prepared>> {
    let extent = [metadata.width as u32, metadata.height as u32];
    if scalar(metadata, "FIR_DisableStep", 0.)? == 0. {
        // Recovered FIR descriptor defaults. A stored zero also selects the
        // default, matching the reference caller's parameter resolution.
        let parameters = [
            ("FIR_ProportionLocalMeanRef", 0.0011),
            ("FIR_ProportionLocalMeanEdgeRef", 0.0019),
            ("FIR_ProportionLocalMeanImgWithEdgeRef", 0.019),
            ("FIR_Locality", 1.),
            ("FIR_SigmoidOffset", 0.55),
            ("FIR_ScaleFactorEdgeRef", 5.),
            ("FIR_ScaleFactorLocalMeanEdgeRef", 7.),
            ("FIR_EdgeMapExponent", 0.7),
        ];
        let mut controls = [0.; 10];
        for (i, (name, default)) in parameters.into_iter().enumerate() {
            let value = scalar(metadata, name, default)?;
            controls[i] = if value == 0. { default } else { value } as f32;
        }
        controls[8] = matrix_green_sum;
        controls[9] = digital_gain;
        let fir = log_reconstruction::Prepared::new(extent, controls)?;
        return Ok(Some(fir));
    }
    Ok(None)
}
