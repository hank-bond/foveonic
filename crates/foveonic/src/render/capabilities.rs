//! Resolves supported photographic adjustments and compatible defaults at family boundaries.

use super::{ColorFieldControls, HighlightMode, Rendering, RepairThreshold, Settings};
use crate::source::{Metadata, ProcessingFamily, Result};

/// Supported adjustments for the loaded photo. Family panels choose their layout,
/// while normalization and request validation use this same processing contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlCapabilities {
    pub family: ProcessingFamily,
    pub monochrome: bool,
    pub noise_adjustments: bool,
    /// Highlight method and threshold controls, separate from recipe-specific recovery amounts.
    pub highlight_adjustments: bool,
    pub row_correction_adjustable: bool,
    pub repair_threshold_adjustable: bool,
    pub color_dq_adjustable: bool,
    pub color_field_adjustable: bool,
}

impl ControlCapabilities {
    /// Resolve supported controls after camera metadata becomes available.
    pub fn from_metadata(metadata: &Metadata) -> Result<Self> {
        Ok(Self::from_family(metadata.processing_family()?))
    }

    /// Family defaults available before camera-specific rendering metadata arrives.
    pub fn from_family(family: ProcessingFamily) -> Self {
        let adjustable = family == ProcessingFamily::Merrill;
        Self {
            family,
            monochrome: true,
            noise_adjustments: family != ProcessingFamily::Classic,
            highlight_adjustments: adjustable,
            row_correction_adjustable: adjustable,
            repair_threshold_adjustable: adjustable,
            color_dq_adjustable: adjustable,
            color_field_adjustable: adjustable,
        }
    }

    /// Resolve the authored recipe's optional noise and detail baseline.
    pub fn default_noise_controls(&self) -> crate::noise::Controls {
        if self.family == ProcessingFamily::Quattro {
            super::recipes::quattro::default_noise_controls()
        } else {
            Default::default()
        }
    }

    /// Saved X3F choices override the recipe's fallback noise controls.
    pub fn default_noise_controls_for_metadata(
        &self,
        metadata: &Metadata,
    ) -> Result<crate::noise::Controls> {
        if self.family == ProcessingFamily::Quattro {
            super::recipes::quattro::captured_noise_controls(metadata)
        } else {
            Ok(self.default_noise_controls())
        }
    }

    /// Quattro X3F marks retain the last saved color/monochrome selection.
    pub fn default_rendering(&self, metadata: &Metadata) -> Result<Rendering> {
        if self.family != ProcessingFamily::Quattro {
            return Ok(Rendering::Color);
        }
        super::recipes::quattro::default_rendering(metadata)
    }

    /// Saved header presets take precedence over the original capture descriptions.
    pub fn default_white_balance(&self, metadata: &Metadata) -> Result<String> {
        if self.family == ProcessingFamily::Quattro {
            super::recipes::quattro::default_white_balance(metadata)
        } else {
            metadata.recorded_white_balance()
        }
    }

    pub fn default_color_mode(&self, metadata: &Metadata) -> Result<String> {
        if self.family == ProcessingFamily::Quattro {
            super::recipes::quattro::default_color_mode(metadata)
        } else {
            metadata.recorded_color_mode()
        }
    }

    /// Initialize a new photo before applying saved edits or explicit overrides.
    /// Existing editing settings must not be passed to this initializer.
    pub fn apply_capture_defaults(
        &self,
        settings: &mut Settings,
        metadata: &Metadata,
    ) -> Result<()> {
        if self.family == ProcessingFamily::Quattro {
            super::recipes::quattro::apply_capture_defaults(settings, metadata)?;
        } else if self.family == ProcessingFamily::Merrill {
            settings.reconstruction = Some(self.default_noise_controls());
        }
        Ok(())
    }

    /// Resolve display and reset values without changing an existing editing recipe.
    /// Source-following options become concrete values in this snapshot only.
    pub fn editing_defaults(&self, metadata: &Metadata, rendering: Rendering) -> Result<Settings> {
        if self.family == ProcessingFamily::Quattro {
            return super::recipes::quattro::editing_defaults(metadata, rendering);
        }
        let mut defaults = Settings {
            rendering,
            ..Settings::default()
        };
        self.apply_capture_defaults(&mut defaults, metadata)?;
        Ok(defaults)
    }

    /// Capture properties identify the panel before full rendering metadata arrives.
    /// Complete metadata retains the recipe's supported-variant checks.
    pub fn editing_available(
        &self,
        metadata: Option<&Metadata>,
        camera_model: Option<&str>,
    ) -> bool {
        if self.family == ProcessingFamily::Quattro {
            super::recipes::quattro::editing_available(metadata, camera_model)
        } else {
            true
        }
    }

    /// Inactive white balance remains stored while the recipe selects its own calibration.
    pub fn white_balance_active(&self, settings: &Settings) -> bool {
        self.family == ProcessingFamily::Merrill || !settings.is_monochrome()
    }

    /// Classic color modes also select the monochrome sensor mixture and appearance.
    pub fn color_mode_active(&self, settings: &Settings) -> bool {
        self.family == ProcessingFamily::Classic || !settings.is_monochrome()
    }

    /// Camera modes whose complete recipe is available.
    pub fn color_mode_presets<'a>(&self, metadata: &'a Metadata) -> Vec<&'a str> {
        if self.family == ProcessingFamily::Quattro {
            super::recipes::quattro::color_mode_presets(metadata)
        } else {
            metadata.color_mode_presets()
        }
    }

    /// Classic monochrome skips chromatic highlight reconstruction.
    pub fn highlights_active(&self, settings: &Settings) -> bool {
        self.family != ProcessingFamily::Quattro
            && (self.family != ProcessingFamily::Classic || !settings.is_monochrome())
    }

    /// Restore unsupported saved adjustments to their supported defaults.
    /// Preserve effect on/off selections and photographic edits.
    pub fn normalize(&self, settings: &mut Settings) -> bool {
        if self.family == ProcessingFamily::Quattro {
            return super::recipes::quattro::normalize_photographic(settings);
        }
        let before = (
            settings.rendering,
            settings.monochrome,
            settings.reconstruction,
            settings.highlight_amount,
            settings.monochrome_smoothness,
            settings.highlight_mode,
            settings.highlight_threshold_ev,
            settings.color_field,
            settings.row_correction,
            settings.repair,
            settings.color_dq_gain,
        );
        if !self.monochrome {
            settings.rendering = Rendering::Color;
        }
        if self.family == ProcessingFamily::Classic {
            settings.monochrome = Default::default();
        }
        if self.family == ProcessingFamily::Merrill {
            if let Some(controls) = &mut settings.reconstruction {
                // The former Merrill residual amount has no band-equivalent.
                // Quattro still owns this field as its Smooth/Crisp control.
                controls.detail = crate::noise::Controls::default().detail;
            }
        }
        if !self.noise_adjustments {
            settings.reconstruction = settings.reconstruction.map(|_| Default::default());
        }
        settings.highlight_amount = None;
        settings.monochrome_smoothness = None;
        if !self.highlight_adjustments {
            settings.highlight_mode = HighlightMode::Classic;
            settings.highlight_threshold_ev = None;
        }
        if !self.color_field_adjustable {
            settings.color_field = ColorFieldControls::default();
        }
        if !self.row_correction_adjustable {
            settings.row_correction = true;
        }
        if !self.repair_threshold_adjustable {
            settings.repair = settings.repair.map(|_| RepairThreshold::Camera);
        }
        if !self.color_dq_adjustable {
            settings.color_dq_gain = None;
        }
        before
            != (
                settings.rendering,
                settings.monochrome,
                settings.reconstruction,
                settings.highlight_amount,
                settings.monochrome_smoothness,
                settings.highlight_mode,
                settings.highlight_threshold_ev,
                settings.color_field,
                settings.row_correction,
                settings.repair,
                settings.color_dq_gain,
            )
    }

    /// Reject unsupported requests without silently changing processing choices.
    /// Numeric validity and camera metadata requirements are checked separately.
    pub fn validate(&self, settings: &Settings) -> Result<()> {
        if self.family == ProcessingFamily::Quattro {
            return super::recipes::quattro::validate_photographic(settings);
        }
        if settings.monochrome_smoothness.is_some() {
            return Err("monochrome Smooth/Crisp is unavailable for this camera family".into());
        }
        if settings.highlight_amount.is_some() {
            return Err("this recipe uses its existing highlight threshold controls".into());
        }
        if settings.sharpen_amount.is_some_and(|value| value < 0.) {
            return Err("sharpening amount must be nonnegative for this camera".into());
        }
        if !self.monochrome && settings.is_monochrome() {
            return Err("Classic monochrome rendering is not supported".into());
        }
        if self.family == ProcessingFamily::Classic
            && settings.is_monochrome()
            && settings.monochrome != Default::default()
        {
            return Err("Classic monochrome uses its camera mixture and reconstruction. Merrill monochrome mix, noise and banding overrides are unsupported".into());
        }
        if !self.repair_threshold_adjustable
            && settings
                .repair
                .is_some_and(|value| value != RepairThreshold::Camera)
        {
            return Err("Classic repair uses its exposure-derived cutoff, manual Merrill badness is unsupported".into());
        }
        if !self.highlight_adjustments
            && settings.highlight_restoration
            && (settings.highlight_mode != HighlightMode::Classic
                || settings.highlight_threshold_ev.is_some())
        {
            return Err("Classic rendering currently supports its camera highlight threshold and streaming neutralization".into());
        }
        if !self.row_correction_adjustable && !settings.row_correction {
            return Err(
                "Classic calibration currently requires its camera row-correction contract".into(),
            );
        }
        if !self.color_field_adjustable && settings.color_field != ColorFieldControls::default() {
            return Err("Classic rendering does not support color-field controls".into());
        }
        if !self.color_dq_adjustable && settings.color_dq_gain.is_some() {
            return Err(
                "Classic nonlinear camera color does not yet support a ColorDQ gain override"
                    .into(),
            );
        }
        if !self.noise_adjustments
            && settings
                .reconstruction
                .is_some_and(|controls| controls != crate::noise::Controls::default())
        {
            return Err(
                "Classic reconstruction currently supports the reference control defaults".into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "capabilities_test.rs"]
mod tests;
