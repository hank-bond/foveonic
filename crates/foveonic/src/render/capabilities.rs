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
            monochrome: family != ProcessingFamily::Quattro,
            noise_adjustments: adjustable,
            highlight_adjustments: adjustable,
            row_correction_adjustable: adjustable,
            repair_threshold_adjustable: adjustable,
            color_dq_adjustable: adjustable,
            color_field_adjustable: adjustable,
        }
    }

    /// White balance remains stored while Classic monochrome selects its own calibration.
    pub fn white_balance_active(&self, settings: &Settings) -> bool {
        self.family != ProcessingFamily::Quattro
            && (self.family != ProcessingFamily::Classic || !settings.is_monochrome())
    }

    /// Classic color modes also select the monochrome sensor mixture and appearance.
    pub fn color_mode_active(&self, settings: &Settings) -> bool {
        self.family != ProcessingFamily::Quattro
            && (self.family == ProcessingFamily::Classic || !settings.is_monochrome())
    }

    /// Classic monochrome skips chromatic highlight reconstruction.
    pub fn highlights_active(&self, settings: &Settings) -> bool {
        self.family != ProcessingFamily::Quattro
            && (self.family != ProcessingFamily::Classic || !settings.is_monochrome())
    }

    /// Restore unsupported saved adjustments to their supported defaults.
    /// Preserve effect on/off selections and photographic edits.
    pub fn normalize(&self, settings: &mut Settings) -> bool {
        let before = (
            settings.rendering,
            settings.monochrome,
            settings.reconstruction,
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
        if !self.noise_adjustments {
            settings.reconstruction = settings.reconstruction.map(|_| Default::default());
        }
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
            return Err("Quattro currently supports sensor-plane inspection only".into());
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
