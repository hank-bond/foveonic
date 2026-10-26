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

/// Workspace rendering quality. Exports and inspection select Full.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PreviewQuality {
    #[default]
    Full,
    /// Reduced working grid with the requested Noise and Detail controls.
    Fast,
    /// Reduced working grid that may omit optional reconstruction for Smart feedback.
    Draft,
}
impl PreviewQuality {
    pub fn is_reduced(self) -> bool {
        self != Self::Full
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Transient preview policy, excluded from per-photo editing sidecars.
    #[serde(skip)]
    pub preview_quality: PreviewQuality,
    /// Transient oriented native-pixel rectangle for regional preview processing.
    #[serde(skip)]
    pub preview_region: Option<[u32; 4]>,
    pub rendering: Rendering,
    pub monochrome: MonochromeControls,
    /// Monochrome Smooth/Crisp selection. None follows the file or recipe default.
    pub monochrome_smoothness: Option<f32>,
    pub white_balance: String,
    pub color_mode: String,
    /// Stops applied to linear RGB before the selected rendering mode's tone curve.
    pub exposure: f32,
    /// Bypass tone/color edits without changing their saved values or camera look.
    pub tone_enabled: bool,
    /// Bypass optional final refinement while retaining its amount and Auto choice.
    pub refinement_enabled: bool,
    pub tone: ToneControls,
    /// Color saturation after tone. Zero is grayscale and one leaves it unchanged.
    pub saturation: f32,
    /// ColorDQ gain after camera color conversion. Zero skips this correction.
    /// None chooses the camera's automatic gain when noise cleanup is on.
    /// Some keeps an explicit override, including zero for Off.
    pub color_dq_gain: Option<f32>,
    /// Ordered camera-record repair, using the current cutoff or an explicit minimum badness.
    pub repair: Option<RepairThreshold>,
    /// Detect and correct remaining channel extrema with reference-style despeckling.
    pub despeckle: bool,
    /// Restore clipped-layer color from original camera saturation maps, independently of tone highlights.
    pub highlight_restoration: bool,
    /// Normalized bright-color correction amount. None follows the file or recipe default.
    pub highlight_amount: Option<f32>,
    /// Camera threshold offset in stops. None selects Auto. Positive values delay neutralization.
    pub highlight_threshold_ev: Option<f32>,
    pub highlight_mode: HighlightMode,
    pub color_field: ColorFieldControls,
    /// Estimate and subtract native additive per-row layer bias before reconstruction.
    pub row_correction: bool,
    /// Optional color noise and detail settings. None selects the recipe baseline.
    /// Mandatory sensor reconstruction still runs when these adjustments are off.
    pub reconstruction: Option<crate::noise::Controls>,
    /// None selects the recipe base. Some is a manual amount. Zero skips final
    /// sharpening in Classic/Merrill and retains baseline proportional detail in Quattro.
    pub sharpen_amount: Option<f32>,
    /// TIFF-only destination. None preserves the camera's working space.
    pub tiff_color_space: Option<TiffColorSpace>,
    /// Final dimensions after orientation. None keeps the active crop's native size.
    pub output_size: Option<[u32; 2]>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            preview_quality: PreviewQuality::Full,
            preview_region: None,
            rendering: Rendering::Color,
            monochrome: MonochromeControls::default(),
            monochrome_smoothness: None,
            // Camera choices are resolved from the selected X3F, not a global preset.
            white_balance: String::new(),
            color_mode: String::new(),
            exposure: 0.,
            tone_enabled: true,
            refinement_enabled: true,
            tone: ToneControls::default(),
            saturation: 1.,
            color_dq_gain: None,
            repair: Some(RepairThreshold::Camera),
            despeckle: true,
            highlight_restoration: true,
            highlight_amount: None,
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

    /// Supported color recipes preserve native framing on their reduced working grid.
    pub fn fast_preview_available(&self, metadata: &crate::source::Metadata) -> bool {
        !self.is_monochrome()
            && (super::recipes::merrill::fast_available(metadata)
                || super::recipes::quattro::fast_available(metadata))
    }

    /// Color recipes can refine native viewport detail before the whole frame.
    pub fn regional_preview_available(&self, metadata: &crate::source::Metadata) -> bool {
        !self.is_monochrome()
            && (super::recipes::quattro::fast_available(metadata)
                || (self.output_size.is_none()
                    && super::recipes::merrill::fast_available(metadata)))
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
            self.reconstruction.unwrap_or_default().base_controls()
        }
    }

    pub fn sharpen_selected(&self) -> bool {
        self.processing_sharpen_amount()
            .is_none_or(|amount| amount > 0.)
    }

    /// Resolve Auto to the authored recipe's sharpening default. Output size
    /// and texture amounts do not change it, including when reconstruction is off.
    pub fn effective_sharpen_amount(&self, metadata: &crate::source::Metadata) -> Result<f32> {
        match self.processing_sharpen_amount() {
            Some(amount) => Ok(amount),
            None => super::recipes::default_sharpen_amount(self, metadata),
        }
    }

    fn processing_sharpen_amount(&self) -> Option<f32> {
        if self.refinement_enabled {
            self.sharpen_amount
        } else {
            Some(0.)
        }
    }

    pub fn color_dq_selected(&self) -> bool {
        !self.is_monochrome()
            && self.color_dq_gain.map_or(
                self.reconstruction
                    .is_some_and(|c| c.merrill_effects().noise),
                |v| v > 0.,
            )
    }

    /// Merrill's optional tile processing, retaining inactive controls in the recipe.
    pub fn merrill_reconstruction(&self) -> Option<crate::noise::Controls> {
        self.reconstruction
            .filter(|c| c.merrill_effects().needs_tiles())
    }

    /// Resolve section bypasses at the recipe boundary. Artifacts and sidecars
    /// keep the requested values, including inactive edits.
    pub(crate) fn processing_settings(&self) -> std::borrow::Cow<'_, Self> {
        if self.tone_enabled && self.refinement_enabled {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut effective = self.clone();
        if !self.tone_enabled {
            effective.exposure = 0.;
            effective.saturation = 1.;
            effective.tone = Default::default();
        }
        effective.sharpen_amount = self.processing_sharpen_amount();
        std::borrow::Cow::Owned(effective)
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
        if self
            .monochrome_smoothness
            .is_some_and(|v| !v.is_finite() || !(0. ..=1.).contains(&v))
        {
            return Err("monochrome Smooth/Crisp must be between zero and one".into());
        }
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
            || self.sharpen_amount.is_some_and(|v| !v.is_finite())
        {
            return Err("saturation must be nonnegative and sharpening must be finite".into());
        }
        if self
            .highlight_amount
            .is_some_and(|v| !(0. ..=1.).contains(&v))
        {
            return Err("highlight correction amount must be in 0..1".into());
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
