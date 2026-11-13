//! Publishes Smart detail intent and preserves compatible image presentation without sequencing jobs.

use super::{
    matching::{same_authored_edits, same_image_geometry},
    worker,
};
use crate::{preferences, App};
use foveonic::render::Scale;
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

impl App {
    /// The active single-photo detail sequence is separate from ordinary set demand.
    pub(crate) fn smart_refinement_active(&self) -> bool {
        !self.multi_photo_overview()
            && self.preferences.preview_quality == preferences::PreviewMode::Smart
            && !self.framing_preview_active()
            && self.output == "final"
            && !self.comparison.requested()
            && (self.viewport().explicit_zoom() || self.detail.is_some())
            && self.selected.as_ref().is_some_and(|path| {
                self.preview_quality_for(path, preferences::PreviewMode::Smart)
                    == preferences::PreviewMode::Full
            })
    }

    pub(crate) fn smart_detail(&self) -> Option<&worker::Frame> {
        if !self.smart_refinement_active() {
            return None;
        }
        self.detail.as_ref().filter(|detail| {
            self.selected.as_ref() == Some(&detail.path)
                && detail.output == self.output
                && detail.rendition == self.before.active
                && detail.framing_preview == self.framing_preview_active()
                && same_image_geometry(&detail.requested, &self.view_settings())
                // A completed newer whole image supersedes this tile even when
                // another edit is already pending. Never rewind to an older base.
                && self.frame.as_ref().is_none_or(|base| base.revision < detail.revision)
        })
    }

    /// Keep the previous image visible while a zoomed edit renders at Full quality.
    fn smart_edit_region(&self) -> Option<[u32; 4]> {
        if !self.smart_refinement_active() || !self.viewport().explicit_zoom() {
            return None;
        }
        let requested = self.view_settings();
        let frame = self.frame.as_ref().filter(|frame| {
            self.selected.as_ref() == Some(&frame.path)
                && frame.output == self.output
                && frame.rendition == self.before.active
                && frame.framing_preview == self.framing_preview_active()
                && frame.artifact.settings.output_size.is_none()
                && requested.output_size.is_none()
                && same_image_geometry(&frame.requested, &requested)
        })?;
        let visible = self.viewport().visible_pixels()?;
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

    pub(crate) fn cancel_smart(&self) {
        if let Some(path) = &self.selected {
            self.worker.cancel_refinement(path);
        }
    }

    pub(crate) fn submit_smart_refinement(&mut self) {
        if !self.sidecars.ready() {
            return;
        }
        let region = self.smart_edit_region();
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
