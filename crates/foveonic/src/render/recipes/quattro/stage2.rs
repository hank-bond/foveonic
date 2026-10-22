//! Prepares and composes ordinary DP color shading, raw response and chroma sectors.

use crate::{
    nodes::color::raw_color_blend::{Controls, Prepared},
    source::{Metadata, Result},
};

const REFERENCE_ISO: f64 = 100.;
const HUE_STRENGTH: [f32; 7] = [0., 0.7, 1., 0.5, 0., 1., 0.];
const ADAPTIVE_KNOTS: [usize; 10] = [0, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096];
const ADAPTIVE_CONTROLS: [f32; 10] = [15., 15., 8., 5., 3., 2.5, 3.25, 4., 4., 4.];

/// The helper follows outer Stage2 color shading and precedes its multi-axis
/// correction. This constructor selects the demonstrated single-input route.
pub(super) fn prepare(
    metadata: &Metadata,
    highlight_amount: f32,
    selected: &str,
    mode: &str,
) -> Result<Prepared> {
    let extent = super::calibration::ordinary_extent(metadata)?;
    if !highlight_amount.is_finite() || !(0. ..=1.).contains(&highlight_amount) {
        return Err("invalid highlight-neutralization amount".into());
    }
    if metadata.matrix("ISOrange").is_some() && metadata.scalar("ISOrange")? != 0. {
        return Err("raw-color ISO override is not implemented".into());
    }
    let capture_iso = metadata.scalar("CaptureISO")?;
    if !capture_iso.is_finite() || capture_iso <= 0. {
        return Err("invalid raw-color capture ISO".into());
    }
    let iso_gain = (capture_iso / REFERENCE_ISO) as f32;
    let sensor_iso = if metadata.matrix("SensorISO").is_some() {
        metadata.scalar("SensorISO")?
    } else {
        0.
    };
    let normalization = if sensor_iso == 0. {
        1.
    } else {
        (sensor_iso / REFERENCE_ISO) as f32
    };
    let wb = super::calibration::white_balance(metadata, selected)?;
    let matrix = super::calibration::color_matrix(metadata, selected, mode)?;
    let controls = Controls {
        extent,
        white_balance: wb.map(|v| v as f32),
        normalization,
        neutral_low: 2400_f32.mul_add(highlight_amount, 8000.),
        neutral_high: 3000_f32.mul_add(highlight_amount, 11000.),
        matrix,
        input_divisor: 1.,
        output_gain: iso_gain,
        brightness_factor: if super::calibration::is_ordinary_sd(metadata) {
            1.
        } else {
            4.
        },
        brightness_offset: if super::calibration::is_ordinary_sd(metadata) {
            150.
        } else {
            200.
        },
        brightness_gain: 1.,
        second_exponent: 1.,
        residual_scale: 16.,
        hue_strength: HUE_STRENGTH,
    };
    // Ordinary DP and interchangeable-lens requests select different
    // brightness attenuation fits in the same raw-color worker.
    let exponent = if super::calibration::is_ordinary_sd(metadata) {
        8.
    } else {
        4.
    };
    let (brightness, signed_response, adaptive) = tables(exponent);
    Prepared::new(controls, brightness, signed_response, adaptive)
}

/// Replaces the recorded ordinary shading with the selected rendering shading.
/// The same immutable preparation can serve later retained-source consumers.
pub(super) fn prepare_shading(
    metadata: &Metadata,
    selected: &str,
) -> Result<Option<crate::nodes::color::spatial_color_gain::Prepared>> {
    use crate::nodes::color::spatial_color_gain::Prepared;
    let extent = super::calibration::ordinary_extent(metadata)?;
    // The ordinary sd catalogue has no legacy color-shading tables. The native
    // table generator returns no correction for this catalogue.
    if super::calibration::is_ordinary_sd(metadata)
        && !metadata.lists.contains_key("ColorShadingGainRatio")
    {
        return Ok(None);
    }
    let aliases = metadata
        .lists
        .get("ColorShadingGainRatio")
        .ok_or("missing color-shading aliases")?;
    let selected = super::calibration::balance_alias(aliases, selected)?;
    // Preserve these inspection boundaries when the selected and recorded
    // balances coincide; their compensation grids are then exact identities.
    Prepared::new(
        extent,
        compensation_grid(metadata, selected, "R")?,
        compensation_grid(metadata, selected, "B")?,
    )
    .map(Some)
}

fn compensation_grid(
    metadata: &Metadata,
    selected: &str,
    channel: &str,
) -> Result<crate::nodes::color::spatial_color_gain::Grid> {
    use super::color_shading::{color, mix};
    use crate::nodes::color::spatial_color_gain::Grid;
    let aliases = metadata
        .lists
        .get("ColorShadingGainRatio")
        .ok_or("missing color-shading aliases")?;
    let selected = color(
        metadata,
        super::calibration::balance_alias(aliases, selected)?,
        channel,
    )?;
    let recorded = color(metadata, super::hlp::recorded_balance(metadata)?, channel)?;
    if recorded.values.contains(&0.) {
        return Err("zero recorded color-shading gain".into());
    }
    let ratio = mix(selected, recorded, |selected, recorded| selected / recorded)?;
    Ok(Grid {
        extent: [ratio.width as u32, ratio.height as u32],
        values: ratio.values,
    })
}

pub(super) struct Signals {
    pub processed: usize,
    pub filtered: usize,
    pub calibrated: usize,
    pub blue: usize,
}

pub(super) struct Rendering<'a> {
    pub white_balance: &'a str,
    pub color_mode: &'a str,
    pub response: Prepared,
}

/// Preserves the shaded source history separately from the processed color image.
pub(super) fn build_signals(
    graph: &mut crate::graph::Graph,
    metadata: &Metadata,
    filtered: usize,
    calibrated: usize,
    blue: usize,
    rendering: Rendering<'_>,
) -> Result<Signals> {
    use crate::{graph::Operation, nodes::color::cell_gain};
    use std::sync::Arc;
    let selected = rendering.white_balance;
    let mode = rendering.color_mode;
    let compensation = prepare_shading(metadata, selected)?.map(Arc::new);
    let (filtered, calibrated, blue) = if let Some(prepared) = compensation {
        let filtered = graph.add(
            Operation::SpatialColorGain {
                prepared: prepared.clone(),
                output: "shading.rgb",
            },
            &[filtered],
        )?;
        let calibrated = graph.add(
            Operation::SpatialColorGain {
                prepared,
                output: "shading.source-after",
            },
            &[calibrated],
        )?;
        let blue = graph.add(
            Operation::CellGain(Arc::new(cell_gain::Prepared::new(
                [metadata.width as u32, metadata.height as u32],
                compensation_grid(metadata, selected, "B")?,
            )?)),
            &[blue],
        )?;
        (filtered, calibrated, blue)
    } else {
        (filtered, calibrated, blue)
    };
    let response = graph.add(
        Operation::RawColorBlend(Arc::new(rendering.response)),
        &[filtered],
    )?;
    let processed = if super::mcc::enabled(metadata) {
        graph.add(
            Operation::SectorColor(Arc::new(super::mcc::prepare(metadata, mode)?)),
            &[response],
        )?
    } else {
        response
    };
    Ok(Signals {
        processed,
        filtered,
        calibrated,
        blue,
    })
}

fn tables(brightness_exponent: f64) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut brightness: Vec<_> = (0..900)
        .map(|index| {
            let x = ((index as f32).mul_add(-0.0033333334, 4.) - 1.) / 3.;
            let powered = f64::from(x).powf(brightness_exponent);
            ((3_f64.mul_add(powered, 1.) as f32) - 1.) / 3.
        })
        .collect();
    brightness.push(0.);
    let mut signed_response = Vec::with_capacity(3072);
    for _ in 0..3 {
        signed_response.extend((0..1024).map(|index| {
            let angle = (0.5 / ((4. / 0.8) * 16.)) * 0.5 * f64::from(index);
            let window = if angle < std::f64::consts::PI {
                (angle.cos() + 1.) * 0.5
            } else {
                0.
            };
            ((4. * window) * ((0.8 * f64::from(index)) / (4. * 16.)).tanh()) as f32
        }));
    }
    let mut adaptive = Vec::with_capacity(4096);
    for segment in 0..9 {
        let low = ADAPTIVE_KNOTS[segment];
        let high = ADAPTIVE_KNOTS[segment + 1];
        let a = ADAPTIVE_CONTROLS[segment];
        let b = ADAPTIVE_CONTROLS[segment + 1];
        adaptive.extend(
            (low..high).map(|index| (b - a).mul_add((index - low) as f32 / (high - low) as f32, a)),
        );
    }
    adaptive[4095] = ADAPTIVE_CONTROLS[9];
    (brightness, signed_response, adaptive)
}

#[cfg(test)]
#[path = "stage2_test.rs"]
mod tests;
