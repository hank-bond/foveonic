//! Defines shared photographic settings and validates their values.

pub use crate::color_space::TiffColorSpace;
pub use crate::nodes::monochrome::Controls as MonochromeControls;
pub use crate::nodes::sensor::color_field::{
    Approach as ColorFieldApproach, Controls as ColorFieldControls, HighlightMode,
};
pub use crate::nodes::{
    finish::tone::Controls as ToneControls, sensor::repair::Threshold as RepairThreshold,
};
use crate::source::Result;

#[cfg(test)]
#[path = "settings_test.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rendering {
    #[default]
    Color,
    Monochrome,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub rendering: Rendering,
    pub monochrome: MonochromeControls,
    pub white_balance: String,
    pub color_mode: String,
    /// Stops applied to linear RGB before the selected rendering mode's tone curve.
    pub exposure: f32,
    pub tone: ToneControls,
    /// Color saturation after tone. Zero is grayscale and one leaves it unchanged.
    pub saturation: f32,
    /// ColorDQ gain after camera color conversion. Zero skips this correction.
    /// None chooses the camera's automatic gain when reconstruction is on.
    /// Some keeps an explicit override, including zero for Off.
    pub color_dq_gain: Option<f32>,
    /// Ordered camera-record repair, using the current cutoff or an explicit minimum badness.
    pub repair: Option<RepairThreshold>,
    /// Detect and correct remaining channel extrema with reference-style despeckling.
    pub despeckle: bool,
    /// Restore clipped-layer color from original camera saturation maps, independently of tone highlights.
    pub highlight_restoration: bool,
    /// Camera threshold offset in stops. None selects Auto. Positive values delay neutralization.
    pub highlight_threshold_ev: Option<f32>,
    pub highlight_mode: HighlightMode,
    pub color_field: ColorFieldControls,
    /// Estimate and subtract native additive per-row layer bias before reconstruction.
    pub row_correction: bool,
    /// Color noise and detail settings. None leaves color reconstruction off.
    pub reconstruction: Option<crate::noise::Controls>,
    /// None selects the camera/noise base. Some(0) is Off, positive is manual.
    pub sharpen_amount: Option<f32>,
    /// TIFF-only destination. None preserves the camera's working space.
    pub tiff_color_space: Option<TiffColorSpace>,
    /// Final dimensions after orientation. None keeps the active crop's native size.
    pub output_size: Option<[u32; 2]>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            rendering: Rendering::Color,
            monochrome: MonochromeControls::default(),
            // Camera choices are resolved from the selected X3F, not a global preset.
            white_balance: String::new(),
            color_mode: String::new(),
            exposure: 0.,
            tone: ToneControls::default(),
            saturation: 1.,
            color_dq_gain: None,
            repair: Some(RepairThreshold::Camera),
            despeckle: true,
            highlight_restoration: true,
            highlight_threshold_ev: None,
            highlight_mode: HighlightMode::Classic,
            color_field: ColorFieldControls::default(),
            row_correction: true,
            reconstruction: None,
            sharpen_amount: Some(0.),
            tiff_color_space: None,
            output_size: None,
        }
    }
}

impl Settings {
    pub fn new(white_balance: impl Into<String>, color_mode: impl Into<String>) -> Self {
        Self {
            white_balance: white_balance.into(),
            color_mode: color_mode.into(),
            ..Self::default()
        }
    }

    pub fn is_monochrome(&self) -> bool {
        self.rendering == Rendering::Monochrome
    }

    pub(crate) fn sensor_noise_controls(&self) -> crate::noise::Controls {
        if self.is_monochrome() {
            crate::noise::Controls {
                chroma: self.monochrome.noise,
                banding: self.monochrome.banding,
                ..Default::default()
            }
        } else {
            self.reconstruction.unwrap_or_default()
        }
    }

    pub fn sharpen_selected(&self) -> bool {
        self.sharpen_amount.is_none_or(|amount| amount > 0.)
    }

    /// Resolve Auto to the authored recipe's sharpening default. Output size
    /// and residual Detail do not change it, including when reconstruction is off.
    pub fn effective_sharpen_amount(&self, metadata: &crate::source::Metadata) -> Result<f32> {
        match self.sharpen_amount {
            Some(amount) => Ok(amount),
            // Classic's reference detail default is unit strength; its recipe
            // does not use the Merrill noise descriptors to select sharpening.
            None if metadata.processing_family().ok()
                == Some(crate::source::ProcessingFamily::Classic) =>
            {
                Ok(1.)
            }
            None => Ok(crate::noise::couplings(
                metadata,
                &self.white_balance,
                self.sensor_noise_controls(),
            )?
            .sharpen_base),
        }
    }

    pub fn color_dq_selected(&self) -> bool {
        !self.is_monochrome()
            && self
                .color_dq_gain
                .map_or(self.reconstruction.is_some(), |v| v > 0.)
    }

    pub fn validate(&self) -> Result<()> {
        if self.white_balance.is_empty() || (!self.is_monochrome() && self.color_mode.is_empty()) {
            return Err(if self.is_monochrome() {
                "--wb is required"
            } else {
                "--wb and --mode are required"
            }
            .into());
        }
        self.validate_values()
    }

    /// Validate the complete request using only this camera's active controls.
    pub fn validate_for_camera(&self, metadata: &crate::source::Metadata) -> Result<()> {
        let capabilities = super::ControlCapabilities::from_metadata(metadata)?;
        self.validate_values()?;
        capabilities.validate(self)?;
        if capabilities.white_balance_active(self) && self.white_balance.is_empty() {
            return Err("white balance is required for this rendering".into());
        }
        if capabilities.color_mode_active(self) && self.color_mode.is_empty() {
            return Err("color mode is required for this rendering".into());
        }
        Ok(())
    }

    /// Inspection can show sensor data before WB or mode is known. Check numbers
    /// here and let stages that use those camera choices resolve them later.
    pub fn validate_values(&self) -> Result<()> {
        self.monochrome.validate()?;
        self.color_field.validate()?;
        if !self.exposure.is_finite()
            || !self.exposure.exp2().is_finite()
            || self.exposure.exp2() <= 0.
        {
            return Err("invalid exposure".into());
        }
        if [self.tone.contrast, self.tone.shadows, self.tone.highlights]
            .iter()
            .any(|v| !(-2. ..=2.).contains(v))
        {
            return Err("contrast, shadows, and highlights must be in -2..2".into());
        }
        if !self.saturation.is_finite()
            || self.saturation < 0.
            || self
                .sharpen_amount
                .is_some_and(|v| !v.is_finite() || v < 0.)
        {
            return Err("saturation and sharpening must be finite and nonnegative".into());
        }
        if self.color_dq_gain.is_some_and(|v| !v.is_finite() || v < 0.) {
            return Err("ColorDQ gain must be finite and nonnegative".into());
        }
        crate::nodes::sensor::highlight::validate_threshold(self.highlight_threshold_ev)?;
        if let Some(threshold) = self.repair {
            threshold.validate()?;
        }
        if let Some(controls) = self.reconstruction {
            controls.validate()?;
        }
        if self.output_size.is_some_and(|size| size.contains(&0)) {
            return Err("output dimensions must be nonzero".into());
        }
        Ok(())
    }
}
