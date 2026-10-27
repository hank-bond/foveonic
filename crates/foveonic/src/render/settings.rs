//! Defines shared photographic settings and validates their values.

pub use crate::color_space::TiffColorSpace;
pub use crate::nodes::finish::photographic::{
    Controls as PhotographicControls, Method as PhotographicMethod,
    Sharpening as UniversalSharpening,
};
pub use crate::nodes::finish::tone_equalizer::Controls as TonalRanges;
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

/// Frozen output switches. These preferences do not become per-image edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ProcessingSections {
    pub tone: bool,
    pub noise: bool,
    pub detail: bool,
    pub sharpening: bool,
    pub corrections: bool,
}
impl Default for ProcessingSections {
    fn default() -> Self {
        Self {
            tone: true,
            noise: true,
            detail: true,
            sharpening: true,
            corrections: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Batch-only section choices, excluded from editing sidecars.
    #[serde(skip)]
    pub processing_sections: ProcessingSections,
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
    /// Bypass final sharpening while retaining its amount and Auto choice.
    // Keep the established sidecar field name while the UI calls this Sharpening.
    #[serde(rename = "refinement_enabled")]
    pub sharpening_enabled: bool,
    pub tone: ToneControls,
    /// Spatial photographic HDR ranges, applied before camera tone and baked into DNG.
    pub tonal_ranges: TonalRanges,
    /// Color saturation after tone. Zero is grayscale and one leaves it unchanged.
    pub saturation: f32,
    pub photographic: PhotographicControls,
    /// Discard retired pre-release layer experiments while retaining other edits.
    #[doc(hidden)]
    #[serde(
        default,
        rename = "layers",
        skip_serializing,
        deserialize_with = "discard_retired_layers"
    )]
    pub retired_layers: (),
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
            processing_sections: Default::default(),
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
            sharpening_enabled: true,
            tone: ToneControls::default(),
            tonal_ranges: TonalRanges::default(),
            saturation: 1.,
            photographic: PhotographicControls::default(),
            retired_layers: (),
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
            sharpen_amount: None,
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
        if self.photographic.sharpening.method == PhotographicMethod::Universal {
            Some(0.)
        } else if self.sharpening_enabled {
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
        if self.tone_enabled
            && self.sharpening_enabled
            && self.processing_sections == ProcessingSections::default()
        {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut effective = self.clone();
        let sections = self.processing_sections;
        effective.tone_enabled &= sections.tone;
        effective.sharpening_enabled &= sections.sharpening;
        if let Some(noise) = &mut effective.reconstruction {
            noise.noise_enabled &= sections.noise;
            noise.detail_enabled &= sections.detail;
        }
        effective.monochrome.detail_enabled &= sections.detail;
        if !sections.noise && effective.color_dq_gain.is_some() {
            effective.color_dq_gain = Some(0.);
        }
        if !sections.corrections {
            effective.repair = None;
            effective.despeckle = false;
            effective.highlight_restoration = false;
        }
        if !effective.tone_enabled {
            effective.exposure = 0.;
            effective.saturation = 1.;
            effective.tone = Default::default();
            effective.tonal_ranges = TonalRanges::default();
            effective.photographic.local_contrast = 0.;
        }
        effective.sharpen_amount = effective.processing_sharpen_amount();
        if !effective.sharpening_enabled {
            effective.photographic.sharpening.amount = 0.;
        }
        std::borrow::Cow::Owned(effective)
    }

    /// Each method selects a complete set of tone controls. Resolve the active
    /// values without overwriting the recipe kept in sidecars or the clipboard.
    pub(crate) fn active_tone_settings(&self) -> std::borrow::Cow<'_, Settings> {
        let settings = self;
        let mut effective = std::borrow::Cow::Borrowed(settings);
        match settings.photographic.tone_method {
            PhotographicMethod::Reference
                if !settings.tonal_ranges.is_neutral()
                    || settings.photographic.local_contrast != 0. =>
            {
                let active = effective.to_mut();
                active.tonal_ranges = Default::default();
                active.photographic.local_contrast = 0.;
            }
            PhotographicMethod::Universal
                if settings.tone.shadows != 0. || settings.tone.highlights != 0. =>
            {
                let active = effective.to_mut();
                active.tone.shadows = 0.;
                active.tone.highlights = 0.;
            }
            _ => {}
        }
        effective
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

    /// Select native technical DNG processing. Shared linear edits are baked separately.
    pub(crate) fn dng_settings(&self) -> Self {
        // Resolve the selected method before removing shared photographic controls,
        // so inactive Reference Shadows/Highlights cannot reappear in DNG metadata.
        let mut settings = self.active_tone_settings().into_owned();
        settings.preview_quality = PreviewQuality::Full;
        settings.preview_region = None;
        settings.output_size = None;
        settings.photographic = PhotographicControls {
            sharpening: crate::nodes::finish::photographic::Sharpening {
                method: self.photographic.sharpening.method,
                ..Default::default()
            },
            ..Default::default()
        };
        settings
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
        self.tonal_ranges.validate()?;
        self.photographic.validate()?;
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

fn discard_retired_layers<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<(), D::Error> {
    <serde::de::IgnoredAny as serde::Deserialize>::deserialize(deserializer)?;
    Ok(())
}
