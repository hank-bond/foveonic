//! Defines shared photographic settings and validates their values.

use super::MerrillBandingControls;
pub use crate::color_space::TiffColorSpace;
pub use crate::nodes::finish::color_editor::Controls as ColorEditorControls;
pub use crate::nodes::finish::dehaze::Controls as DehazeControls;
pub use crate::nodes::finish::perceptual_grade::Controls as PerceptualControls;
pub use crate::nodes::finish::photographic::{
    Controls as PhotographicControls, Method as PhotographicMethod,
    Sharpening as UniversalSharpening,
};
pub use crate::nodes::finish::tone_equalizer::Controls as TonalRanges;
pub use crate::nodes::finish::white_balance::Controls as WhiteBalanceBias;
pub use crate::nodes::monochrome::Controls as MonochromeControls;
pub use crate::nodes::sensor::color_field::{
    Approach as ColorFieldApproach, Controls as ColorFieldControls, HighlightMode,
};
pub use crate::nodes::{
    finish::tone::Controls as ToneControls, sensor::repair::Threshold as RepairThreshold,
};
pub use crate::source::AutoExposureCompensation;
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

/// Merrill color noise cleanup and shield correction strategy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MerrillNoiseMethod {
    #[default]
    Reference,
    MultiscaleRows,
    MultiscaleRowsAndColumns,
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
    pub saturation: bool,
    pub color_editor: bool,
    pub noise: bool,
    pub detail: bool,
    pub sharpening: bool,
    pub corrections: bool,
}
impl Default for ProcessingSections {
    fn default() -> Self {
        Self {
            tone: true,
            saturation: true,
            color_editor: true,
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
    pub dehaze: DehazeControls,
    /// Request the automatic shadow tone as transient control metadata, even at zero Amount.
    #[serde(skip)]
    pub dehaze_estimate: bool,
    /// Batch-only section choices, excluded from editing sidecars.
    #[serde(skip)]
    pub processing_sections: ProcessingSections,
    /// Transient preview policy, excluded from per-photo editing sidecars.
    #[serde(skip)]
    pub preview_quality: PreviewQuality,
    /// Transient oriented native-pixel rectangle for regional preview processing.
    #[serde(skip)]
    pub preview_region: Option<[u32; 4]>,
    /// Transient selected-range color preview, excluded from saved recipes and exports.
    #[serde(skip)]
    pub color_range_preview: Option<usize>,
    /// Shared estimator tuning snapshot. This is not a per-photo edit.
    #[serde(skip)]
    pub adaptive_tone_tuning: super::AdaptiveToneTuning,
    pub rendering: Rendering,
    pub monochrome: MonochromeControls,
    /// Monochrome Smooth/Crisp selection. None follows the file or recipe default.
    pub monochrome_smoothness: Option<f32>,
    pub white_balance: String,
    pub white_balance_bias: WhiteBalanceBias,
    pub color_mode: String,
    /// Stops applied to linear RGB before the selected rendering mode's tone curve.
    pub exposure: f32,
    /// Resolve inverse capture compensation per source instead of using the manual value.
    pub exposure_auto: bool,
    /// Selected capture components, retained when automatic exposure is off.
    pub exposure_auto_compensation: AutoExposureCompensation,
    /// Bypass tone edits without changing their saved values or camera look.
    pub tone_enabled: bool,
    /// Bypass saturation independently of tone and subject-color adjustments.
    pub saturation_enabled: bool,
    /// Bypass optional Corrections while retaining their authored values.
    pub corrections_enabled: bool,
    /// Shared gamut protection for Reference, Universal and Color Range saturation.
    pub prevent_color_clipping: bool,
    /// Bypass Color Editor without changing its saved range adjustments.
    pub color_editor_enabled: bool,
    /// Bypass final sharpening while retaining its amount and Auto choice.
    pub sharpening_enabled: bool,
    pub tone: ToneControls,
    /// Spatial photographic HDR ranges, applied before camera tone and baked into DNG.
    pub tonal_ranges: TonalRanges,
    /// Reference camera saturation. Zero is grayscale and one leaves it unchanged.
    pub saturation: f32,
    pub photographic: PhotographicControls,
    /// ColorDQ gain after camera color conversion. Zero skips this correction.
    /// None chooses the camera's automatic gain when noise cleanup is on.
    /// Some keeps an explicit override, including zero for Off.
    pub color_dq_gain: Option<f32>,
    /// Ordered camera-record repair, using the current cutoff or an explicit minimum badness.
    pub repair: Option<RepairThreshold>,
    /// Detect and correct remaining channel extrema with reference-style despeckling.
    pub despeckle: bool,
    pub merrill_speckles: super::MerrillSpeckleControls,
    pub neutralize_shadows: crate::nodes::finish::shadow_neutralize::Controls,
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
    /// Merrill color noise method, retained while Noise is disabled.
    pub merrill_noise_method: MerrillNoiseMethod,
    /// Optional directional cleanup, retaining settings while inactive.
    pub merrill_banding: MerrillBandingControls,
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
            dehaze: DehazeControls::default(),
            dehaze_estimate: false,
            processing_sections: Default::default(),
            preview_quality: PreviewQuality::Full,
            preview_region: None,
            color_range_preview: None,
            adaptive_tone_tuning: Default::default(),
            rendering: Rendering::Color,
            monochrome: MonochromeControls::default(),
            monochrome_smoothness: None,
            // Camera choices are resolved from the selected X3F, not a global preset.
            white_balance: String::new(),
            white_balance_bias: Default::default(),
            color_mode: String::new(),
            exposure: 0.,
            exposure_auto: false,
            exposure_auto_compensation: Default::default(),
            tone_enabled: true,
            saturation_enabled: true,
            corrections_enabled: true,
            prevent_color_clipping: true,
            color_editor_enabled: true,
            sharpening_enabled: true,
            tone: ToneControls::default(),
            tonal_ranges: TonalRanges::default(),
            saturation: 1.,
            photographic: PhotographicControls::default(),
            color_dq_gain: None,
            repair: Some(RepairThreshold::Camera),
            despeckle: true,
            merrill_speckles: Default::default(),
            neutralize_shadows: Default::default(),
            highlight_restoration: true,
            highlight_amount: None,
            highlight_threshold_ev: None,
            highlight_mode: HighlightMode::Classic,
            color_field: ColorFieldControls::default(),
            row_correction: true,
            merrill_noise_method: MerrillNoiseMethod::Reference,
            merrill_banding: MerrillBandingControls::default(),
            reconstruction: None,
            sharpen_amount: None,
            tiff_color_space: None,
            output_size: None,
        }
    }
}

impl Settings {
    /// Effective requests use manual EV so nested DNG/reference recipes cannot resolve Auto twice.
    pub(crate) fn resolved_exposure(
        &self,
        metadata: &crate::source::Metadata,
    ) -> std::borrow::Cow<'_, Self> {
        if !self.exposure_auto {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut effective = self.clone();
        effective.exposure = self
            .exposure_auto_compensation
            .inverse(crate::source::CaptureExposureCompensation::from_properties(
                &metadata.properties,
            ))
            .unwrap_or(0.);
        effective.exposure_auto = false;
        std::borrow::Cow::Owned(effective)
    }
    /// Photographic values without viewport or output-job policy.
    pub fn photographic_recipe(&self) -> Self {
        super::edits::snapshot(self)
    }

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
        !self.adaptive_tone_active()
            && !self.is_monochrome()
            && (super::recipes::quattro::fast_available(metadata)
                || (self.output_size.is_none()
                    && super::recipes::merrill::fast_available(metadata)))
    }

    pub fn adaptive_tone_active(&self) -> bool {
        self.tone_enabled
            && self.processing_sections.tone
            && self.photographic.tone_method == PhotographicMethod::Universal
            && self.photographic.base_response.is_adaptive()
    }

    pub fn dehaze_auto(&self) -> bool {
        self.corrections_enabled
            && self.processing_sections.corrections
            && self.dehaze.shadow.is_none()
    }

    pub fn dehaze_active(&self) -> bool {
        self.corrections_enabled && self.processing_sections.corrections && self.dehaze.amount != 0.
    }

    pub fn is_monochrome(&self) -> bool {
        self.rendering == Rendering::Monochrome
    }

    pub(crate) fn sensor_noise_controls(&self) -> crate::noise::Controls {
        if self.is_monochrome() {
            crate::noise::Controls {
                chroma: self.monochrome.noise,
                banding: if self.directional_banding_selected() {
                    crate::noise::Controls::default().banding
                } else {
                    self.monochrome.banding
                },
                ..Default::default()
            }
        } else {
            let mut controls = self.reconstruction.unwrap_or_default().base_controls();
            if self.directional_banding_selected() {
                controls.banding = crate::noise::Controls::default().banding;
            }
            controls
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

    /// Effective Color Editor state, after output-section policy is resolved.
    pub(crate) fn color_editor_active(&self) -> bool {
        self.color_editor_enabled
            && self
                .photographic
                .color_editor
                .is_active(self.is_monochrome())
    }

    pub(crate) fn advanced_color_active(&self) -> bool {
        self.color_editor_enabled
            && (self
                .photographic
                .color_editor
                .advanced
                .is_active(self.is_monochrome())
                || self.color_range_preview.is_some())
    }

    pub(crate) fn color_editor_chromatic_active(&self) -> bool {
        self.color_editor_enabled
            && !self.is_monochrome()
            && self.photographic.color_editor.chromatic_active()
    }

    pub(crate) fn reference_saturation(&self) -> f32 {
        if self.saturation_enabled
            && !self.is_monochrome()
            && self.photographic.saturation_method == PhotographicMethod::Reference
        {
            self.saturation
        } else {
            1.
        }
    }
    pub fn universal_saturation(&self) -> bool {
        self.saturation_enabled
            && self.photographic.saturation_method == PhotographicMethod::Universal
    }
    pub(crate) fn universal_saturation_active(&self) -> bool {
        self.universal_saturation() && self.photographic.perceptual.is_active(self.is_monochrome())
    }
    pub(crate) fn linear_chromatic_active(&self) -> bool {
        (self.dehaze_active() && !self.is_monochrome())
            || self.color_editor_chromatic_active()
            || (self.universal_saturation()
                && !self.is_monochrome()
                && self.photographic.perceptual.chromatic_active())
    }

    pub fn color_dq_selected(&self) -> bool {
        !self.is_monochrome()
            && self.color_dq_gain.map_or(
                self.reconstruction
                    .is_some_and(|c| c.merrill_effects().noise),
                |v| v > 0.,
            )
    }

    fn merrill_noise_active(&self) -> bool {
        self.processing_sections.noise
            && (self.is_monochrome()
                || self
                    .reconstruction
                    .is_some_and(|c| c.merrill_effects().noise))
    }

    /// Native layer filtering follows the selected recipe's Noise bypass.
    pub(crate) fn multiscale_noise_selected(&self) -> bool {
        self.merrill_noise_method != MerrillNoiseMethod::Reference && self.merrill_noise_active()
    }

    /// Directional cleanup is independent of the selected native denoiser.
    pub(crate) fn directional_banding_selected(&self) -> bool {
        self.merrill_banding.enabled
            && self.merrill_banding.method == super::MerrillBandingMethod::Directional
            && self.merrill_noise_active()
    }

    pub(crate) fn reference_banding_selected(&self) -> bool {
        self.merrill_banding.enabled
            && self.merrill_banding.method == super::MerrillBandingMethod::Reference
            && self.merrill_noise_active()
    }

    pub(crate) fn reference_rows_selected(&self) -> bool {
        self.row_correction && self.reference_banding_selected()
    }

    /// Merrill's optional tile processing, retaining inactive controls in the recipe.
    pub fn merrill_reconstruction(&self) -> Option<crate::noise::Controls> {
        self.reconstruction
            .filter(|c| c.merrill_effects().needs_tiles())
    }

    /// Resolve section bypasses at the recipe boundary. Artifacts and sidecars
    /// keep the requested values, including inactive edits.
    pub(crate) fn processing_settings(
        &self,
        family: crate::source::ProcessingFamily,
    ) -> std::borrow::Cow<'_, Self> {
        let speckles_enabled = if family == crate::source::ProcessingFamily::Merrill {
            self.processing_sections.noise
                && (self.is_monochrome() || self.reconstruction.is_none_or(|c| c.noise_enabled))
        } else {
            self.corrections_enabled && self.processing_sections.corrections
        };
        if self.tone_enabled
            && self.corrections_enabled
            && self.saturation_enabled
            && self.sharpening_enabled
            && self.processing_sections == ProcessingSections::default()
            && (speckles_enabled || (!self.despeckle && self.neutralize_shadows.is_neutral()))
        {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut effective = self.clone();
        let mut sections = self.processing_sections;
        sections.corrections &= self.corrections_enabled;
        effective.processing_sections = sections;
        effective.despeckle &= speckles_enabled;
        if family == crate::source::ProcessingFamily::Merrill && !speckles_enabled {
            effective.neutralize_shadows.strength = 0.;
        }
        effective.tone_enabled &= sections.tone;
        effective.saturation_enabled &= sections.saturation;
        effective.color_editor_enabled &= sections.color_editor;
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
            effective.dehaze.amount = 0.;
            effective.repair = None;
            effective.highlight_restoration = false;
        }
        if !effective.tone_enabled {
            effective.exposure = 0.;
            effective.exposure_auto = false;
            effective.tone = Default::default();
            effective.tonal_ranges = TonalRanges::default();
            effective.photographic.local_contrast = 0.;
            effective.photographic.brightness = 0.;
            effective.photographic.base_response = Default::default();
        }
        if !effective.saturation_enabled {
            effective.saturation = 1.;
            effective.photographic.perceptual = Default::default();
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
                    || settings.photographic.local_contrast != 0.
                    || settings.photographic.brightness != 0.
                    || settings.photographic.base_response.is_adaptive() =>
            {
                let active = effective.to_mut();
                active.tonal_ranges = Default::default();
                active.photographic.local_contrast = 0.;
                active.photographic.brightness = 0.;
                active.photographic.base_response = Default::default();
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
        settings.color_range_preview = None;
        settings.output_size = None;
        settings.dehaze = Default::default();
        settings.dehaze_estimate = false;
        settings.photographic = PhotographicControls {
            // The selection is retained for appearance range policy. Its spatial
            // gain is baked through the shared linear edit reference.
            base_response: settings.photographic.base_response,
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
        self.dehaze.validate()?;
        self.merrill_speckles.validate()?;
        self.neutralize_shadows.validate()?;
        self.merrill_banding.validate()?;
        self.white_balance_bias.validate()?;
        if self
            .monochrome_smoothness
            .is_some_and(|v| !v.is_finite() || !(0. ..=1.).contains(&v))
        {
            return Err("monochrome Smooth/Crisp must be between zero and one".into());
        }
        self.tonal_ranges.validate()?;
        self.photographic.validate()?;
        self.monochrome.validate()?;
        self.adaptive_tone_tuning.validate()?;
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
