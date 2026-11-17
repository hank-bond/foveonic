//! Classifies completed images for pixel reuse, per-photo feedback and current-view delivery.

use super::{coordinator::Completion, matching, request::Purpose, worker};
use crate::{editor::Rendition, App};
use foveonic::render::Settings;
use std::path::{Path, PathBuf};

/// Immutable viewport facts supplied to the coordinator when their values change.
#[derive(Clone, PartialEq)]
pub struct ViewState {
    /// Workspace policy. Active Current inspection independently requires Quality.
    pub processing_quality: foveonic::render::ProcessingQuality,
    pub active: Option<PathBuf>,
    pub rendition: Rendition,
    pub output: String,
    pub comparison: bool,
    pub framing_preview: bool,
    pub exporting: bool,
    pub calibration: Option<String>,
    pub settings: Settings,
    pub overviews: Vec<PhotoView>,
    pub displayed_revision: Option<u64>,
    pub detail_revision: Option<u64>,
}

/// Current photographic content ordering for one demanded photo.
#[derive(Clone, PartialEq)]
pub struct PhotoView {
    pub path: PathBuf,
    pub rendition: Rendition,
    pub settings: Settings,
    pub displayed_revision: Option<u64>,
    pub detail_revision: Option<u64>,
}

impl ViewState {
    pub fn context(&self, path: &Path) -> ViewContext<'_> {
        let overview = self.overviews.iter().find(|photo| photo.path == path);
        ViewContext {
            processing_quality: self.processing_quality,
            active: self.active.as_deref(),
            rendition: self.rendition,
            output: &self.output,
            comparison: self.comparison,
            framing_preview: self.framing_preview,
            exporting: self.exporting,
            calibration: self.calibration.as_deref(),
            settings: &self.settings,
            overview_visible: overview.is_some(),
            overview_settings: overview.map(|photo| &photo.settings),
            overview_rendition: overview.map(|photo| photo.rendition),
            overview_displayed_revision: overview.and_then(|photo| photo.displayed_revision),
            overview_detail_revision: overview.and_then(|photo| photo.detail_revision),
            displayed_revision: self.displayed_revision,
            detail_revision: self.detail_revision,
        }
    }
}

/// Only borrowed image facts are needed to decide delivery. GPU resources stay with the frame.
pub struct ImageInfo<'a> {
    pub auxiliary_preview: bool,
    pub origin: &'a foveonic::source::SourceOrigin,
    pub working_scale: foveonic::render::Scale,
    pub path: &'a Path,
    pub output: &'a str,
    pub requested: &'a Settings,
    pub calibration: Option<&'a str>,
    pub framing_preview: bool,
    pub comparison: bool,
    pub detail: bool,
    pub revision: u64,
}
impl<'a> From<&'a worker::Frame> for ImageInfo<'a> {
    fn from(frame: &'a worker::Frame) -> Self {
        Self {
            auxiliary_preview: frame.auxiliary_preview,
            origin: &frame.origin,
            working_scale: frame.artifact.settings.working_scale,
            path: &frame.path,
            output: &frame.output,
            requested: &frame.requested,
            calibration: frame.calibration_signature.as_deref(),
            framing_preview: frame.framing_preview,
            comparison: frame.comparison.is_some(),
            detail: frame.detail,
            revision: frame.content_revision(),
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
    pub processing_quality: foveonic::render::ProcessingQuality,
    pub active: Option<&'a Path>,
    pub rendition: Rendition,
    pub output: &'a str,
    pub comparison: bool,
    pub framing_preview: bool,
    pub exporting: bool,
    pub calibration: Option<&'a str>,
    pub settings: &'a Settings,
    pub overview_visible: bool,
    pub overview_settings: Option<&'a Settings>,
    pub overview_rendition: Option<Rendition>,
    pub overview_displayed_revision: Option<u64>,
    pub overview_detail_revision: Option<u64>,
    pub displayed_revision: Option<u64>,
    pub detail_revision: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destination {
    Active,
    Overview,
    Preview,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    Removed,
    CalibrationChanged,
    NotVisible,
    GeometryChanged,
    ProcessingQualityChanged,
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
            Self::ProcessingQualityChanged => "processing_quality_changed",
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

/// The coordinator's result handler settles before classification, then retains
/// useful pixels and checks source/revision admission before returning a destination.
pub fn classify(completion: Completion, reply: &ReplyInfo<'_>, view: &ViewContext<'_>) -> Decision {
    let rejection = if completion == Completion::Removed {
        Some(Rejection::Removed)
    } else if reply.image.as_ref().is_some_and(|image| {
        image.requested.processing_quality != expected_processing_quality(reply, view)
    }) {
        Some(Rejection::ProcessingQualityChanged)
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

    if *reply.purpose == Purpose::Preview {
        let settings = if view.active == Some(reply.path) {
            Some(view.settings)
        } else {
            view.overview_settings
        };
        let destination = if completion == Completion::Current
            && settings.is_some_and(|settings| {
                reply.rendition == Rendition::Current
                    && reply.image.as_ref().is_some_and(|image| {
                        auxiliary_preview_matches(
                            image,
                            settings,
                            view.processing_quality,
                            view.calibration,
                            None,
                        )
                    })
            }) {
            Ok(Destination::Preview)
        } else {
            Err(Rejection::Superseded)
        };
        return Decision {
            retain_pixels: destination.is_ok(),
            update_photo: false,
            destination,
        };
    }

    Decision {
        retain_pixels: reply.image.is_some(),
        update_photo: completion == Completion::Current,
        destination: destination(completion, reply, view),
    }
}

pub(crate) fn auxiliary_preview_matches(
    image: &ImageInfo<'_>,
    settings: &Settings,
    quality: foveonic::render::ProcessingQuality,
    calibration: Option<&str>,
    origin: Option<&foveonic::source::SourceOrigin>,
) -> bool {
    image.auxiliary_preview
        && !image.detail
        && !image.framing_preview
        && !image.comparison
        && image.output == "final"
        && image.working_scale.is_reduced()
        && image.requested.processing_quality == quality
        && image.calibration == calibration
        && origin.is_none_or(|origin| origin == image.origin)
        && matching::same_authored_edits(image.requested, settings)
}

/// A matching finer Whole already supplies the navigator without a second preview texture.
pub(crate) fn prefer_auxiliary_preview(
    base: Option<&ImageInfo<'_>>,
    preview: &ImageInfo<'_>,
) -> bool {
    !base.is_some_and(|base| {
        !base.detail
            && !base.framing_preview
            && !base.comparison
            && base.output == "final"
            && base.origin == preview.origin
            && base.calibration == preview.calibration
            && base.requested.processing_quality == preview.requested.processing_quality
            && matching::same_authored_edits(base.requested, preview.requested)
            && base.working_scale.divisor() <= preview.working_scale.divisor()
    })
}

fn expected_processing_quality(
    reply: &ReplyInfo<'_>,
    view: &ViewContext<'_>,
) -> foveonic::render::ProcessingQuality {
    if view.active == Some(reply.path)
        && reply.rendition == Rendition::Current
        && matches!(reply.purpose, Purpose::Inspection(_, _))
    {
        super::quality::for_view(
            view.processing_quality,
            view.rendition,
            view.output,
            view.comparison,
        )
    } else {
        view.processing_quality
    }
}

fn destination(
    completion: Completion,
    reply: &ReplyInfo<'_>,
    view: &ViewContext<'_>,
) -> Result<Destination, Rejection> {
    let photographic_overview = matches!(
        reply.purpose,
        Purpose::Current | Purpose::Detail | Purpose::Camera | Purpose::Defaults
    );
    let current_overview = reply.rendition == Rendition::Current && photographic_overview;
    if view.active != Some(reply.path)
        || (current_overview
            && (view.rendition != Rendition::Current || view.output != "final" || view.comparison))
    {
        if !photographic_overview || !view.overview_visible {
            return Err(Rejection::NotVisible);
        }
        if !current_overview && view.overview_rendition != Some(reply.rendition) {
            return Err(Rejection::RenditionChanged);
        }
        if reply.image.as_ref().is_some_and(|image| {
            image.framing_preview || image.comparison || image.output != "final"
        }) {
            return Err(Rejection::FramingViewChanged);
        }
        if reply.image.as_ref().is_some_and(|image| {
            !view
                .overview_settings
                .is_some_and(|settings| matching::same_image_geometry(settings, image.requested))
        }) {
            return Err(Rejection::GeometryChanged);
        }
        if completion != Completion::Current
            && !reply.image.as_ref().is_some_and(|image| {
                view.overview_displayed_revision
                    .is_none_or(|revision| image.revision > revision)
                    && (*reply.purpose != Purpose::Detail
                        || view
                            .overview_detail_revision
                            .is_none_or(|revision| image.revision > revision))
            })
        {
            return Err(Rejection::Superseded);
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
                && matching::same_image_geometry(image.requested, view.settings)
                && (!view.comparison || image.comparison)
                && view
                    .displayed_revision
                    .is_none_or(|revision| image.revision > revision)
                && (!image.detail
                    || view
                        .detail_revision
                        .is_none_or(|revision| image.revision > revision))
        })
    {
        return Err(Rejection::Superseded);
    }
    Ok(Destination::Active)
}

/// Confirm that a coordinated result still fits the UI's latest intent and view.
pub fn recheck(
    reply: &worker::Reply,
    view: &ViewContext<'_>,
    requested: Option<&Settings>,
) -> super::coordinator::ResultUpdate {
    let mut update = reply
        .handling
        .expect("coordinator handles results before UI delivery");
    let authored_current = requested
        .is_some_and(|settings| matching::same_authored_edits(&reply.request.settings, settings));
    update.current &= authored_current || reply.exported.is_some();
    let completion = if update.current {
        Completion::Current
    } else {
        Completion::Superseded
    };
    let decision = classify(completion, &reply.into(), view);
    update.update_photo &= decision.update_photo;
    update.destination = update.destination.and_then(|_| decision.destination.ok());
    update
}

impl App {
    pub(crate) fn current_photo_frame(&self, path: &Path) -> Option<&worker::Frame> {
        [
            self.frame.as_ref(),
            self.multi_edit.frame(path),
            self.before.current_frame(),
        ]
        .into_iter()
        .flatten()
        .filter(|frame| {
            frame.path == path
                && frame.is_photographic()
                && frame.processing_quality == self.preferences.processing_quality
        })
        .max_by_key(|frame| frame.content_revision())
    }

    fn photo_render_view(&self, path: &Path) -> Option<PhotoView> {
        let rendition = self.photo_rendition(path);
        let current = if self.selected.as_deref() == Some(path) {
            Some(&self.settings)
        } else {
            self.multi_edit.settings(path)
        }?;
        Some(PhotoView {
            path: path.to_owned(),
            rendition,
            settings: self.preferences.render_settings(current),
            displayed_revision: self
                .rendition_frame(path, rendition)
                .map(worker::Frame::content_revision),
            detail_revision: self
                .rendition_detail(path, rendition)
                .map(worker::Frame::content_revision),
        })
    }

    pub(crate) fn publish_render_view(&self) {
        let calibration = self
            .noise_calibration
            .selected(self.preferences.noise_profile);
        self.worker.set_view(ViewState {
            processing_quality: self.preferences.processing_quality,
            active: self.selected.clone(),
            rendition: self
                .selected
                .as_deref()
                .map(|path| self.photo_rendition(path))
                .unwrap_or(Rendition::Current),
            output: self.output.clone(),
            comparison: self.comparison.requested(),
            framing_preview: self.framing_preview_active(),
            exporting: self.exporting,
            calibration: calibration
                .as_ref()
                .map(|profile| profile.signature().to_owned()),
            settings: self.preferences.render_settings(&self.settings),
            overviews: self
                .multi_edit
                .demanded_paths()
                .iter()
                .filter_map(|path| self.photo_render_view(path))
                .collect(),
            displayed_revision: self.frame.as_ref().map(worker::Frame::content_revision),
            detail_revision: self.detail_frame().map(worker::Frame::content_revision),
        });
    }

    pub(crate) fn render_result_for_view(
        &self,
        reply: &worker::Reply,
    ) -> super::coordinator::ResultUpdate {
        let calibration = self
            .noise_calibration
            .selected(self.preferences.noise_profile);
        let authored = self.preferences.render_settings(&self.settings);
        let overview = self.photo_render_view(&reply.path);
        let before_settings = self.photo_view_settings(&reply.path);
        let rendition = self.photo_rendition(&reply.path);
        let view = ViewContext {
            processing_quality: self.preferences.processing_quality,
            active: self.selected.as_deref(),
            rendition: self
                .selected
                .as_deref()
                .map(|path| self.photo_rendition(path))
                .unwrap_or(Rendition::Current),
            output: &self.output,
            comparison: self.comparison.requested(),
            framing_preview: self.framing_preview_active(),
            exporting: self.exporting,
            calibration: calibration.as_ref().map(|profile| profile.signature()),
            // Before changes processing settings but preserves authored framing.
            settings: &authored,
            overview_visible: self.multi_edit.demanded(&reply.path),
            overview_settings: overview.as_ref().map(|photo| &photo.settings),
            overview_rendition: Some(rendition),
            overview_displayed_revision: self
                .rendition_frame(&reply.path, rendition)
                .map(worker::Frame::content_revision),
            overview_detail_revision: self
                .rendition_detail(&reply.path, rendition)
                .map(worker::Frame::content_revision),
            displayed_revision: self.frame.as_ref().map(worker::Frame::content_revision),
            detail_revision: self.detail_frame().map(worker::Frame::content_revision),
        };
        let requested = if reply.rendition == Rendition::Current {
            overview.as_ref().map(|photo| &photo.settings)
        } else {
            before_settings
                .as_ref()
                .filter(|_| reply.rendition == rendition)
        };
        let mut result = recheck(reply, &view, requested);
        if reply.stamp.target.purpose == Purpose::Preview
            && reply
                .result
                .as_ref()
                .ok()
                .and_then(Option::as_ref)
                .is_some_and(|frame| {
                    self.worker
                        .cache
                        .source_origin(&reply.path)
                        .is_some_and(|origin| origin != frame.origin)
                })
        {
            result.current = false;
            result.update_photo = false;
            result.destination = None;
        }
        result
    }
}

#[cfg(test)]
#[path = "delivery_test.rs"]
mod tests;
