//! Describes photo render requests and resolves their purpose and preview geometry independently of execution.

use crate::{editor::Rendition, export::Export};
use foveonic::render::ProcessingQuality;
use foveonic::{
    render::{ControlCapabilities, Scale, Settings},
    source::{Metadata, Result},
};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone)]
pub struct Request {
    /// Native inspection pixels are independent of the main preview, including Whole fallback.
    pub detail: bool,
    pub framing_preview: bool,
    pub calibration: Option<Arc<foveonic::noise_calibration::Calibration>>,
    pub rendition: Rendition,
    pub revision: u64,
    pub path: PathBuf,
    pub settings: Settings,
    pub preview_scale: Scale,
    /// Processing policy, independent of preview resolution and refinement passes.
    pub processing_quality: ProcessingQuality,
    pub output: String,
    pub comparison: bool,
    pub export: Option<Export>,
}

/// Frozen refinement intent. The UI reserves both presentation revisions, while
/// the coordinator decides when the optional whole-image request becomes runnable.
#[derive(Clone)]
pub struct Refinement {
    pub first: Request,
    pub whole: Option<Request>,
}
impl Refinement {
    /// Panning does not restart the initial region for unchanged photo edits.
    pub fn same_sequence(&self, other: &Self) -> bool {
        super::matching::same_request_context(&self.first, &other.first)
            && super::matching::same_authored_edits(&self.first.settings, &other.first.settings)
            && self.first.processing_quality == other.first.processing_quality
            && self.whole.is_some() == other.whole.is_some()
    }
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
                Rendition::Current if self.detail => Purpose::Detail,
                Rendition::Current => Purpose::Current,
            }
        }
    }

    fn photographic_preview(&self) -> bool {
        self.export.is_none() && self.output == "final" && !self.comparison
    }

    /// Requested processing scale before camera-specific smallest-grid sizing or reuse
    /// of an existing framing plane. Inspection, comparison, export and detail
    /// require native samples, independently of the workspace preview choice.
    pub fn nominal_working_scale(&self) -> Scale {
        if !self.photographic_preview() || self.detail {
            Scale::NATIVE
        } else {
            self.preview_scale
        }
    }

    /// Reduced final views share the reduced working set, including both policies,
    /// Camera and Defaults. Native detail recovers its source through the loader.
    pub fn uses_reduced_session(&self) -> bool {
        self.preview_scale.is_reduced() && self.photographic_preview() && !self.detail
    }

    /// Apply processing geometry to camera-resolved settings without changing
    /// authored controls or the workspace preview policy.
    pub fn apply_preview(&self, settings: &mut Settings, metadata: &Metadata) -> Result<()> {
        settings.processing_quality = if self.photographic_preview() {
            self.processing_quality
        } else {
            ProcessingQuality::Quality
        };
        settings.working_scale = self.nominal_working_scale();
        if settings.working_scale.is_reduced() {
            settings.working_scale = camera_preview_scale(self.preview_scale, metadata)?;
        }
        if self.photographic_preview() && self.detail {
            // Detail is sampled in native image coordinates, independently of final output sizing.
            settings.output_size = None;
        }
        if !self.photographic_preview() || !settings.regional_preview_available(metadata) {
            settings.preview_region = None;
        }
        Ok(())
    }

    /// Both policies can display completed finer sampling without processing it again.
    /// Composition changes separately preserve compatible photographic pixels.
    pub fn accepts_scale(&self, scale: Scale, metadata: &Metadata) -> Result<bool> {
        let mut settings = self.settings.clone();
        self.apply_preview(&mut settings, metadata)?;
        Ok(super::quality::accepts_scale(settings.working_scale, scale))
    }
}

/// Foreground requests and idle preparation use the same camera scale policy.
/// Camera interpretation remains owned by the processing capabilities.
pub(crate) fn camera_preview_scale(scale: Scale, metadata: &Metadata) -> Result<Scale> {
    if scale.divisor() == 8 {
        Ok(ControlCapabilities::from_metadata(metadata)?.thumb_scale())
    } else {
        Ok(scale)
    }
}

#[cfg(test)]
#[path = "request_test.rs"]
mod tests;
