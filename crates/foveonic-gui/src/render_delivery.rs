//! Classifies completed images for pixel reuse, per-photo feedback and current-view delivery.

use crate::{
    editor::Rendition, render_coordinator::Completion, render_matching, render_request::Purpose,
    worker, App,
};
use foveonic::render::Settings;
use std::path::Path;

/// Only borrowed image facts are needed to decide delivery. GPU resources stay with the frame.
pub struct ImageInfo<'a> {
    pub path: &'a Path,
    pub output: &'a str,
    pub requested: &'a Settings,
    pub calibration: Option<&'a str>,
    pub framing_preview: bool,
    pub comparison: bool,
    pub regional: bool,
    pub revision: u64,
}
impl<'a> From<&'a worker::Frame> for ImageInfo<'a> {
    fn from(frame: &'a worker::Frame) -> Self {
        Self {
            path: &frame.path,
            output: &frame.output,
            requested: &frame.requested,
            calibration: frame.calibration_signature.as_deref(),
            framing_preview: frame.framing_preview,
            comparison: frame.comparison.is_some(),
            regional: frame.artifact.settings.preview_region.is_some(),
            revision: frame.revision,
        }
    }
}

pub struct ReplyInfo<'a> {
    pub path: &'a Path,
    pub purpose: &'a Purpose,
    pub rendition: Rendition,
    pub exported: bool,
    pub image: Option<ImageInfo<'a>>,
}
impl<'a> From<&'a worker::Reply> for ReplyInfo<'a> {
    fn from(reply: &'a worker::Reply) -> Self {
        Self {
            path: &reply.path,
            purpose: &reply.stamp.target.purpose,
            rendition: reply.rendition,
            exported: reply.exported.is_some(),
            image: reply
                .result
                .as_ref()
                .ok()
                .and_then(Option::as_ref)
                .map(Into::into),
        }
    }
}

pub struct ViewContext<'a> {
    pub active: Option<&'a Path>,
    pub rendition: Rendition,
    pub output: &'a str,
    pub comparison: bool,
    pub framing_preview: bool,
    pub exporting: bool,
    pub calibration: Option<&'a str>,
    pub settings: &'a Settings,
    pub overview_selected: bool,
    pub overview_settings: Option<&'a Settings>,
    pub displayed_revision: Option<u64>,
    pub detail_revision: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    Active,
    Overview,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    Removed,
    CalibrationChanged,
    NotVisible,
    GeometryChanged,
    FramingViewChanged,
    RenditionChanged,
    Superseded,
}
impl Rejection {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Removed => "target_removed",
            Self::CalibrationChanged => "calibration_changed",
            Self::NotVisible => "not_visible",
            Self::GeometryChanged => "geometry_changed",
            Self::FramingViewChanged => "framing_view_changed",
            Self::RenditionChanged => "rendition_changed",
            Self::Superseded => "superseded_view",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Decision {
    pub retain_pixels: bool,
    pub update_photo: bool,
    pub destination: Result<Destination, Rejection>,
}

/// Settlement happens before classification. Source identity and revision admission
/// remain coordinator-owned and must succeed before applying an image destination.
pub fn classify(completion: Completion, reply: &ReplyInfo<'_>, view: &ViewContext<'_>) -> Decision {
    let rejection = if completion == Completion::Removed {
        Some(Rejection::Removed)
    } else if !reply.exported
        && reply
            .image
            .as_ref()
            .is_some_and(|image| image.calibration != view.calibration)
    {
        Some(Rejection::CalibrationChanged)
    } else {
        None
    };
    if let Some(rejection) = rejection {
        return Decision {
            retain_pixels: false,
            update_photo: false,
            destination: Err(rejection),
        };
    }

    Decision {
        retain_pixels: reply.image.is_some(),
        update_photo: completion == Completion::Current,
        destination: destination(completion, reply, view),
    }
}

fn destination(
    completion: Completion,
    reply: &ReplyInfo<'_>,
    view: &ViewContext<'_>,
) -> Result<Destination, Rejection> {
    let current_overview =
        reply.rendition == Rendition::Current && *reply.purpose == Purpose::Current;
    if view.active != Some(reply.path)
        || (current_overview
            && (view.rendition != Rendition::Current || view.output != "final" || view.comparison))
    {
        if !current_overview || !view.overview_selected {
            return Err(Rejection::NotVisible);
        }
        if reply.image.as_ref().is_some_and(|image| {
            !view.overview_settings.is_some_and(|settings| {
                render_matching::same_image_geometry(settings, image.requested)
            })
        }) {
            return Err(Rejection::GeometryChanged);
        }
        return Ok(Destination::Overview);
    }
    if reply
        .image
        .as_ref()
        .is_some_and(|image| image.framing_preview != view.framing_preview)
    {
        return Err(Rejection::FramingViewChanged);
    }
    if reply.rendition != view.rendition {
        return Err(Rejection::RenditionChanged);
    }
    if completion != Completion::Current
        && !reply.image.as_ref().is_some_and(|image| {
            !view.exporting
                && view.active == Some(image.path)
                && view.output == image.output
                && render_matching::same_image_geometry(image.requested, view.settings)
                && (!view.comparison || image.comparison)
                && view
                    .displayed_revision
                    .is_none_or(|revision| image.revision > revision)
                && (!image.regional
                    || view
                        .detail_revision
                        .is_none_or(|revision| image.revision > revision))
        })
    {
        return Err(Rejection::Superseded);
    }
    Ok(Destination::Active)
}

impl App {
    pub(crate) fn image_delivery(&self, completion: Completion, reply: &worker::Reply) -> Decision {
        let calibration = self
            .noise_calibration
            .selected(self.preferences.noise_profile);
        classify(
            completion,
            &reply.into(),
            &ViewContext {
                active: self.selected.as_deref(),
                rendition: self.before.active,
                output: &self.output,
                comparison: self.comparison.requested(),
                framing_preview: self.framing_preview_active(),
                exporting: self.exporting,
                calibration: calibration.as_ref().map(|profile| profile.signature()),
                // Before changes processing settings but preserves authored framing.
                settings: &self.settings,
                overview_selected: self.edit_selection.contains(&reply.path),
                overview_settings: self.multi_edit.settings(&reply.path),
                displayed_revision: self.frame.as_ref().map(|frame| frame.revision),
                detail_revision: self.detail.as_ref().map(|frame| frame.revision),
            },
        )
    }
}

#[cfg(test)]
#[path = "render_delivery_test.rs"]
mod tests;
