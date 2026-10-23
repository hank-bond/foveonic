//! Refines the first Quattro detail viewport once before completing the whole Full frame.

use crate::{preferences, worker, App};
use eframe::egui;
use foveonic::render::{PreviewQuality, Settings};
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
pub fn same_edits(a: &Settings, b: &Settings) -> bool {
    let mut a = a.clone();
    let mut b = b.clone();
    a.preview_quality = PreviewQuality::Full;
    b.preview_quality = PreviewQuality::Full;
    a.preview_region = None;
    b.preview_region = None;
    a == b
}

impl App {
    pub(super) fn smart_detail(&self, pixels_per_point: f32) -> Option<&worker::Frame> {
        if self.preferences.preview_quality != preferences::PreviewMode::Smart
            || self.frame.as_ref().is_some_and(|f| {
                f.artifact.settings.preview_quality == foveonic::render::PreviewQuality::Full
            })
            || self.output != "final"
            || self.comparison.requested()
            || self
                .viewport
                .zoom_percent(pixels_per_point)
                .is_some_and(|zoom| zoom <= 100.)
        {
            return None;
        }
        self.detail.as_ref().filter(|detail| {
            self.selected.as_ref() == Some(&detail.path)
                && same_edits(&detail.artifact.settings, &self.settings)
        })
    }

    pub(super) fn update_smart(&mut self, ctx: &egui::Context) {
        if self.preferences.preview_quality != preferences::PreviewMode::Smart
            || self.exporting
            || self.output != "final"
            || self.comparison.requested()
            || (self.detail.is_none()
                && !self
                    .viewport
                    .zoom_percent(ctx.pixels_per_point())
                    .is_some_and(|zoom| zoom > 100.))
        {
            return;
        }
        let Some(frame) = self.frame.as_ref().filter(|f| {
            f.artifact
                .settings
                .fast_preview_available(&f.artifact.metadata)
                && same_edits(&f.artifact.settings, &self.settings)
        }) else {
            return;
        };
        if self.busy
            || frame.artifact.settings.preview_quality == foveonic::render::PreviewQuality::Full
        {
            return;
        }
        // Deliver the first detail tile before submitting the whole frame. Keep
        // that tile visible while the serial image worker completes Full.
        if self.detail.is_some() {
            self.submit_with_quality(None, None, preferences::PreviewMode::Full);
            return;
        }
        let Some(visible) = self.viewport.visible_pixels() else {
            return;
        };
        let visible = [
            visible.top_left[0],
            visible.top_left[1],
            visible.bottom_right[0] + 1,
            visible.bottom_right[1] + 1,
        ];
        if let Some(region) = self.smart.next(visible, frame.display_extent(false)) {
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
