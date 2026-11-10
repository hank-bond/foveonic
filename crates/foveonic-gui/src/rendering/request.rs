//! Describes photo render requests and resolves their purpose and preview geometry independently of execution.

use crate::{editor::Rendition, export::Export, preferences::PreviewMode};
use foveonic::{
    render::{ControlCapabilities, Scale, Settings},
    source::{Metadata, Result},
};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone)]
pub struct Request {
    pub framing_preview: bool,
    pub calibration: Option<Arc<foveonic::noise_calibration::Calibration>>,
    pub rendition: Rendition,
    pub revision: u64,
    pub path: PathBuf,
    pub settings: Settings,
    pub preview_quality: PreviewMode,
    /// Workspace choice, independent of Smart refinement passes.
    pub preview_policy: PreviewMode,
    pub output: String,
    pub comparison: bool,
    pub export: Option<Export>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Purpose {
    Current,
    Camera,
    Defaults,
    Detail,
    Inspection(String, bool),
    Export,
}
impl Request {
    /// Consumer identity, independent of the processing grid used to produce it.
    pub fn purpose(&self) -> Purpose {
        if self.export.is_some() {
            Purpose::Export
        } else {
            match self.rendition {
                Rendition::Camera => Purpose::Camera,
                Rendition::Defaults => Purpose::Defaults,
                Rendition::Current if self.output != "final" || self.comparison => {
                    Purpose::Inspection(self.output.clone(), self.comparison)
                }
                Rendition::Current if self.settings.preview_region.is_some() => Purpose::Detail,
                Rendition::Current => Purpose::Current,
            }
        }
    }

    fn photographic_preview(&self) -> bool {
        self.export.is_none() && self.output == "final" && !self.comparison
    }

    /// Requested processing scale before camera-specific Thumb sizing or reuse
    /// of an existing framing plane. Inspection, comparison, export and detail
    /// require native samples, independently of the workspace preview choice.
    pub fn nominal_working_scale(&self) -> Scale {
        if !self.photographic_preview() || self.settings.preview_region.is_some() {
            Scale::NATIVE
        } else {
            self.preview_quality.working_scale()
        }
    }

    /// Smart keeps native history for refinement. Explicit reduced final views
    /// use the reduced working set, including Camera and Defaults renditions.
    pub fn uses_reduced_session(&self) -> bool {
        matches!(
            self.preview_quality,
            PreviewMode::Fast | PreviewMode::Draft | PreviewMode::Thumb
        ) && self.photographic_preview()
            && self.settings.preview_region.is_none()
    }

    /// Apply processing geometry to camera-resolved settings without changing
    /// authored controls or the workspace preview policy.
    pub fn apply_preview(&self, settings: &mut Settings, metadata: &Metadata) -> Result<()> {
        settings.working_scale = self.nominal_working_scale();
        if settings.working_scale.is_reduced() {
            settings.working_scale = camera_preview_scale(self.preview_quality, metadata)?;
        }
        if self.preview_quality != PreviewMode::Smart
            || !self.photographic_preview()
            || !settings.regional_preview_available(metadata)
        {
            settings.preview_region = None;
        }
        Ok(())
    }
}

/// Foreground requests and idle preparation use the same camera scale policy.
/// Camera interpretation remains owned by the processing capabilities.
pub(crate) fn camera_preview_scale(quality: PreviewMode, metadata: &Metadata) -> Result<Scale> {
    if quality == PreviewMode::Thumb {
        Ok(ControlCapabilities::from_metadata(metadata)?.thumb_scale())
    } else {
        Ok(quality.working_scale())
    }
}

#[cfg(test)]
#[path = "request_test.rs"]
mod tests;
