//! Publishes Smart detail intent and preserves compatible image presentation without sequencing jobs.

use super::{
    matching::{same_authored_edits, same_image_geometry},
    worker,
};
use crate::{preferences, App};
use foveonic::render::{Scale, Settings};
use std::path::Path;
// Keep a small margin around the clicked viewport while the whole frame catches up.
const DETAIL_OVERSCAN_PIXELS: u32 = 64;

/// Expand the visible image region by the existing native-pixel margin.
fn render_region(visible: [u32; 4], extent: [u32; 2]) -> [u32; 4] {
    [
        visible[0].saturating_sub(DETAIL_OVERSCAN_PIXELS),
        visible[1].saturating_sub(DETAIL_OVERSCAN_PIXELS),
        visible[2]
            .saturating_add(DETAIL_OVERSCAN_PIXELS)
            .min(extent[0]),
        visible[3]
            .saturating_add(DETAIL_OVERSCAN_PIXELS)
            .min(extent[1]),
    ]
}

pub fn contains(outer: [u32; 4], inner: [u32; 4]) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
}

pub(crate) enum PhotoRefinement {
    Deferred,
    Ready(Box<super::request::Refinement>),
}

pub(crate) fn detail_frame<'a>(
    before: &'a crate::editor::Before,
    photos: &'a crate::multi_edit_runtime::Runtime,
    selected: Option<&Path>,
) -> Option<&'a worker::Frame> {
    if before.active == crate::editor::Rendition::Current {
        photos.detail(selected?)
    } else {
        before.detail.as_ref()
    }
}

impl App {
    pub(crate) fn detail_frame(&self) -> Option<&worker::Frame> {
        detail_frame(&self.before, &self.multi_edit, self.selected.as_deref())
    }

    pub(crate) fn smart_refinement_active(&self) -> bool {
        self.preferences.preview_quality == preferences::PreviewMode::Smart
            && !self.framing_preview_active()
            && self.output == "final"
            && !self.comparison.requested()
            && (self.viewport().explicit_zoom() || self.detail_frame().is_some())
            && self.selected.as_ref().is_some_and(|path| {
                self.preview_quality_for(path, preferences::PreviewMode::Smart)
                    == preferences::PreviewMode::Full
            })
    }

    /// All displayed photographs share the same detail compatibility checks.
    pub(crate) fn photo_detail(
        &self,
        path: &Path,
        base: Option<&worker::Frame>,
        requested: &Settings,
        rendition: crate::editor::Rendition,
    ) -> Option<&worker::Frame> {
        if self.preferences.preview_quality != preferences::PreviewMode::Smart
            || self.loupe_inspecting(path)
        {
            return None;
        }
        let detail = if rendition == crate::editor::Rendition::Current {
            self.multi_edit.detail(path)
        } else {
            self.before.detail.as_ref()
        }?;
        let calibration = self
            .noise_calibration
            .selected(self.preferences.noise_profile);
        (detail.path == path
            && detail.output == "final"
            && detail.rendition == rendition
            && !detail.framing_preview
            && detail.comparison.is_none()
            && same_image_geometry(&detail.requested, requested)
            && detail.calibration_signature.as_deref()
                == calibration.as_ref().map(|profile| profile.signature())
            && self
                .worker
                .cache
                .source_origin(path)
                .is_none_or(|origin| origin == detail.origin)
            && base.is_none_or(|base| {
                base.output == detail.output
                    && base.rendition == detail.rendition
                    && !base.framing_preview
                    && base.comparison.is_none()
                    && base.origin == detail.origin
                    && same_image_geometry(&base.requested, requested)
                    && base.content_revision() < detail.content_revision()
            }))
        .then_some(detail)
    }

    pub(crate) fn smart_detail(&self) -> Option<&worker::Frame> {
        if !self.smart_refinement_active() {
            return None;
        }
        self.photo_detail(
            self.selected.as_deref()?,
            self.frame.as_ref(),
            &self.view_settings(),
            self.before.active,
        )
    }

    /// Use the photo's own viewport and compatible whole frame to request native detail.
    fn photo_edit_region(
        &self,
        path: &Path,
        base: Option<&worker::Frame>,
        requested: &Settings,
        rendition: crate::editor::Rendition,
    ) -> Option<[u32; 4]> {
        let viewport = self.viewports.view(Some(path));
        if !viewport.explicit_zoom() {
            return None;
        }
        let calibration = self
            .noise_calibration
            .selected(self.preferences.noise_profile);
        let frame = base.filter(|frame| {
            frame.path == path
                && frame.output == "final"
                && frame.rendition == rendition
                && !frame.framing_preview
                && frame.comparison.is_none()
                && frame.artifact.settings.output_size.is_none()
                && requested.output_size.is_none()
                && same_image_geometry(&frame.requested, requested)
                && frame.calibration_signature.as_deref()
                    == calibration.as_ref().map(|profile| profile.signature())
                && self
                    .worker
                    .cache
                    .source_origin(path)
                    .is_none_or(|origin| origin == frame.origin)
        })?;
        let visible = viewport.visible_pixels()?;
        if !requested.regional_preview_available(&frame.artifact.metadata) {
            return None;
        }
        let window = frame.display_window(false);
        let rect = [
            visible.top_left[0] + window[0],
            visible.top_left[1] + window[1],
            visible.bottom_right[0] + 1 + window[0],
            visible.bottom_right[1] + 1 + window[1],
        ];
        Some(render_region(rect, frame.uncropped_extent()))
    }

    /// Current detail follows the complete photo set, independently of active selection.
    pub(crate) fn photo_refinement(
        &self,
        path: &Path,
        mut settings: Settings,
    ) -> Option<PhotoRefinement> {
        if self.preferences.preview_quality == preferences::PreviewMode::Smart
            && self.loupe_inspecting(path)
        {
            return Some(PhotoRefinement::Deferred);
        }
        if self.preferences.preview_quality != preferences::PreviewMode::Smart
            || !self
                .displayed_paths
                .iter()
                .any(|displayed| displayed == path)
            || !self.viewports.view(Some(path)).explicit_zoom()
            || self.preview_quality_for(path, preferences::PreviewMode::Smart)
                != preferences::PreviewMode::Full
            || (self.selected.as_deref() == Some(path) && self.framing_preview_active())
        {
            return None;
        }
        let base = self.current_photo_frame(path);
        settings = self.preferences.render_settings(&settings);
        if base.is_some_and(|frame| {
            frame.artifact.settings.working_scale == Scale::NATIVE
                && same_authored_edits(&frame.requested, &settings)
                && self.reusable_photograph(path, frame, &settings)
        }) {
            return None;
        }
        if base.is_some() && self.viewports.view(Some(path)).visible_pixels().is_none() {
            // Selection discards old screen bounds. Wait for this photo to be
            // drawn instead of replacing its pending region with fallback Whole.
            return Some(PhotoRefinement::Deferred);
        }
        settings.working_scale = Scale::NATIVE;
        settings.preview_region =
            self.photo_edit_region(path, base, &settings, crate::editor::Rendition::Current);
        let first = super::request::Request {
            detail: settings.preview_region.is_some(),
            framing_preview: false,
            calibration: self
                .noise_calibration
                .selected(self.preferences.noise_profile),
            rendition: crate::editor::Rendition::Current,
            revision: self.revision + 1,
            path: path.to_owned(),
            settings,
            preview_quality: preferences::PreviewMode::Full,
            preview_policy: preferences::PreviewMode::Smart,
            output: "final".into(),
            comparison: false,
            export: None,
        };
        let whole = first.settings.preview_region.map(|_| {
            let mut whole = first.clone();
            whole.revision += 1;
            whole.detail = false;
            whole.settings.preview_region = None;
            whole
        });
        Some(PhotoRefinement::Ready(Box::new(
            super::request::Refinement { first, whole },
        )))
    }

    pub(crate) fn cancel_smart(&self) {
        if let Some(path) = &self.selected {
            self.worker.cancel_refinement(path, self.before.active);
        }
    }

    pub(crate) fn submit_smart_refinement(&mut self) {
        if !self.sidecars.ready() {
            return;
        }
        let region = self.selected.as_deref().and_then(|path| {
            self.photo_edit_region(
                path,
                self.frame.as_ref(),
                &self.view_settings(),
                self.before.active,
            )
        });
        let Some(first) = self.active_render_request(
            self.revision + 1,
            None,
            region,
            preferences::PreviewMode::Full,
        ) else {
            return;
        };
        let whole = region.map(|_| {
            let mut whole = first.clone();
            whole.revision += 1;
            whole.detail = false;
            whole.settings.preview_region = None;
            whole
        });
        let revision = whole.as_ref().unwrap_or(&first).revision;
        let queued_at = self
            .opening
            .as_ref()
            .map_or_else(std::time::Instant::now, |opening| opening.began);
        self.publish_render_view();
        if self
            .worker
            .refine(super::request::Refinement { first, whole }, queued_at)
        {
            self.revision = revision;
            if let Some(path) = &self.selected {
                crate::timing::requested(revision, path, &self.output, &self.settings);
            }
            self.opening = None;
            self.busy = self.worker.busy();
        }
    }

    /// Publish detail intent. Completion and continuation run in the coordinator.
    pub(crate) fn update_smart(&mut self) {
        if !self.sidecars.ready() || self.exporting {
            return;
        }
        let zoomed = self.viewport_mut().take_explicit_zoom();
        if self.before.active == crate::editor::Rendition::Current && !self.framing_preview_active()
        {
            return;
        }
        if self.framing_preview_active() {
            if zoomed && self.preferences.preview_quality == preferences::PreviewMode::Smart {
                if let Some(path) = &self.selected {
                    let quality = self.preview_quality_for(path, preferences::PreviewMode::Smart);
                    if self.frame.as_ref().is_some_and(|frame| {
                        frame.artifact.settings.working_scale.divisor()
                            > quality.working_scale().divisor()
                    }) {
                        self.submit_with_quality(None, None, quality);
                    }
                }
            }
            return;
        }
        if !self.smart_refinement_active() {
            return;
        }
        if self.frame.as_ref().is_some_and(|frame| {
            self.selected.as_ref() == Some(&frame.path)
                && frame.rendition == self.before.active
                && frame.artifact.settings.working_scale == Scale::NATIVE
                && same_authored_edits(&frame.requested, &self.view_settings())
        }) {
            return;
        }
        self.submit_smart_refinement();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detail_margin_stays_inside_the_native_image() {
        assert_eq!(
            render_region([100, 100, 400, 300], [1000, 800]),
            [36, 36, 464, 364]
        );
        assert_eq!(
            render_region([900, 700, 1000, 800], [1000, 800]),
            [836, 636, 1000, 800]
        );
    }
}
