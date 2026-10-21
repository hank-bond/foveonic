//! Resolves Classic camera defaults into numerical recipe controls.

use crate::{nodes::finish::signed_response::ToneAdjustments, source::Metadata};

/// Reference caller defaults. Metadata overrides are applied by each preparer.
pub(super) struct CameraPolicy {
    pub green_column_scale: f64,
    pub blue_axis_mix: f64,
    pub dark_gain_scale: f64,
    alternate_tone: bool,
    attenuate_color_noise: bool,
}

impl CameraPolicy {
    pub fn from_metadata(metadata: &Metadata) -> Self {
        let model = metadata
            .properties
            .get("CAMMODEL")
            .map_or("", String::as_str);
        // Calibration accepts padded model names. Tone and noise selection use
        // the exact stored names, preserving the existing preparation contract.
        let calibration_model = model.trim();
        Self {
            // SD10 reduces a green column filter that matches the red filter.
            green_column_scale: if calibration_model == "SIGMA SD10" {
                0.94
            } else {
                1.
            },
            blue_axis_mix: match calibration_model {
                "SIGMA SD9" => 0.06,
                "SIGMA SD10" => 0.02,
                _ => 0.,
            },
            // Multiplies drift in camera codes and exposure in microseconds.
            dark_gain_scale: if calibration_model == "SIGMA SD9" {
                2.5e-7
            } else {
                0.
            },
            alternate_tone: matches!(model, "SIGMA SD15" | "SIGMA DP1X"),
            attenuate_color_noise: matches!(model, "SIGMA DP2" | "SIGMA DP2S" | "SIGMA DP2X"),
        }
    }

    pub fn tone_adjustments(&self, iso: f64) -> ToneAdjustments {
        if self.alternate_tone {
            ToneAdjustments {
                exposure: 0.6,
                shadows: -0.2,
                contrast: 0.4,
                highlights: 0.2,
            }
        } else {
            ToneAdjustments {
                highlights: if iso <= 50. { 0.8 } else { 0. },
                ..Default::default()
            }
        }
    }

    /// DP2 profiles reduce color noise amplitude above combined gain 10 and 20.
    pub fn color_noise_attenuation(&self, combined_gain: f32) -> f64 {
        if self.attenuate_color_noise && combined_gain > 20. {
            0.25
        } else if self.attenuate_color_noise && combined_gain > 10. {
            0.5
        } else {
            1.
        }
    }
}

/// CaptureShutter units per second for an uppercase, trimmed model name.
pub(super) fn capture_shutter_units(model: &str) -> f64 {
    match model {
        "SIGMA SD9" | "SD9" | "SIGMA SD10" | "SD10" => 1.,
        _ => super::reference::MICROSECONDS_PER_SECOND,
    }
}

/// Calibration's legacy parser recognizes only the full stored camera names.
pub(super) fn calibration_shutter_units(model: &str) -> f64 {
    match model {
        "SIGMA SD9" | "SIGMA SD10" => 1.,
        _ => super::reference::MICROSECONDS_PER_SECOND,
    }
}
