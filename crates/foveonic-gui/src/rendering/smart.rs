//! Refines reduced overviews into Full, using a first viewport tile where the recipe supports it.

use super::{
    matching::{same_authored_edits, same_image_geometry},
    worker,
};
use crate::{preferences, App};
use foveonic::render::Scale;
// Keep a small margin around the clicked viewport while the whole frame catches up.
const DETAIL_OVERSCAN_PIXELS: u32 = 64;

#[derive(Default)]
pub struct Controller {
    requested: Option<[u32; 4]>,
}
impl Controller {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    /// Refine once per source/edit sequence. Panning never queues another tile.
    pub fn next(&mut self, visible: [u32; 4], extent: [u32; 2]) -> Option<[u32; 4]> {
        if self.requested.is_some() {
            return None;
        }
        let target = [
            visible[0].saturating_sub(DETAIL_OVERSCAN_PIXELS),
            visible[1].saturating_sub(DETAIL_OVERSCAN_PIXELS),
            visible[2]
                .saturating_add(DETAIL_OVERSCAN_PIXELS)
                .min(extent[0]),
            visible[3]
                .saturating_add(DETAIL_OVERSCAN_PIXELS)
                .min(extent[1]),
        ];
        self.requested = Some(target);
        Some(target)
    }
}

pub fn contains(outer: [u32; 4], inner: [u32; 4]) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
}

impl App {
    pub(crate) fn smart_detail(&self) -> Option<&worker::Frame> {
        if self.multi_photo_overview()
            || self.preferences.preview_quality != preferences::PreviewMode::Smart
            || self.framing_preview_active()
            || self.output != "final"
            || self.comparison.requested()
            || !self.selected.as_ref().is_some_and(|path| {
                self.preview_quality_for(path, preferences::PreviewMode::Smart)
                    == preferences::PreviewMode::Full
            })
        {
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
    pub(crate) fn smart_edit_request(
        &mut self,
    ) -> Option<(preferences::PreviewMode, Option<[u32; 4]>)> {
        if self.multi_photo_overview()
            || self.preferences.preview_quality != preferences::PreviewMode::Smart
            || self.framing_preview_active()
            || self.output != "final"
            || self.comparison.requested()
            || !self.viewport().explicit_zoom()
            || !self.selected.as_ref().is_some_and(|path| {
                self.preview_quality_for(path, preferences::PreviewMode::Smart)
                    == preferences::PreviewMode::Full
            })
        {
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
            return Some((preferences::PreviewMode::Full, None));
        }
        let window = frame.display_window(false);
        let rect = [
            visible.top_left[0] + window[0],
            visible.top_left[1] + window[1],
            visible.bottom_right[0] + 1 + window[0],
            visible.bottom_right[1] + 1 + window[1],
        ];
        self.smart
            .next(rect, frame.uncropped_extent())
            .map(|region| (preferences::PreviewMode::Full, Some(region)))
    }

    pub(crate) fn update_smart(&mut self) {
        if self.framing_preview_active() {
            if !self.busy
                && self.viewport_mut().take_explicit_zoom()
                && self.preferences.preview_quality == preferences::PreviewMode::Smart
            {
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
        if !self.busy {
            self.viewport_mut().take_explicit_zoom();
        }
        if self.preferences.preview_quality != preferences::PreviewMode::Smart
            || self.framing_preview_active()
            || self.exporting
            || self.output != "final"
            || self.comparison.requested()
            || !self.selected.as_ref().is_some_and(|path| {
                self.preview_quality_for(path, preferences::PreviewMode::Smart)
                    == preferences::PreviewMode::Full
            })
            || (self.detail.is_none() && !self.viewport().explicit_zoom())
        {
            return;
        }
        if !self.busy
            && self.detail.as_ref().is_some_and(|detail| {
                let full_completed = self.frame.as_ref().is_some_and(|frame| {
                    frame.revision > detail.revision
                        && frame.artifact.settings.working_scale == Scale::NATIVE
                        && same_authored_edits(&frame.requested, &self.view_settings())
                });
                !full_completed
                    && self.selected.as_ref() == Some(&detail.path)
                    && detail.rendition == self.before.active
                    && same_authored_edits(&detail.requested, &self.view_settings())
            })
        {
            self.submit_with_quality(None, None, preferences::PreviewMode::Full);
            return;
        }
        let Some(frame) = self.frame.as_ref().filter(|f| {
            f.rendition == self.before.active
                && same_authored_edits(&f.requested, &self.view_settings())
        }) else {
            return;
        };
        if self.busy || frame.artifact.settings.working_scale == foveonic::render::Scale::NATIVE {
            return;
        }
        // Recipes without regional support refine the whole frame directly.
        // Otherwise retain the first detail tile while Full completes.
        if self.detail.is_some()
            || (frame.framing_preview && frame.artifact.settings.output_size.is_some())
            || !frame
                .artifact
                .settings
                .regional_preview_available(&frame.artifact.metadata)
        {
            self.submit_with_quality(None, None, preferences::PreviewMode::Full);
            return;
        }
        let Some(visible) = self.viewport().visible_pixels() else {
            return;
        };
        let window = frame.display_window(false);
        let visible = [
            visible.top_left[0] + window[0],
            visible.top_left[1] + window[1],
            visible.bottom_right[0] + 1 + window[0],
            visible.bottom_right[1] + 1 + window[1],
        ];
        let extent = if frame.artifact.settings.output_size.is_some() {
            frame.display_extent(false)
        } else {
            frame.uncropped_extent()
        };
        if let Some(region) = self.smart.next(visible, extent) {
            self.submit_request(None, Some(region));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_detail_is_immediate_and_panning_never_restarts_it() {
        let mut state = Controller::default();
        assert_eq!(
            state.next([100, 100, 400, 300], [1000, 800]),
            Some([36, 36, 464, 364])
        );
        assert_eq!(state.next([500, 400, 800, 600], [1000, 800]), None);
        state.reset();
        assert_eq!(
            state.next([900, 700, 1000, 800], [1000, 800]),
            Some([836, 636, 1000, 800])
        );
    }
}
