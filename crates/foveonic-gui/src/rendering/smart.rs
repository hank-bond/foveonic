//! Publishes native-detail intent and preserves compatible image presentation without sequencing jobs.

use super::{
    matching::{same_authored_edits, same_image_geometry},
    worker,
};
use crate::App;
use foveonic::render::{Scale, Settings};
use std::path::Path;
// Keep small pans inside the native patch without another render.
const DETAIL_OVERSCAN_PIXELS: u32 = 64;

/// Expand the visible image region by the existing native-pixel margin.
fn render_region(visible: [u32; 4], window: [u32; 4]) -> [u32; 4] {
    [
        visible[0]
            .saturating_sub(DETAIL_OVERSCAN_PIXELS)
            .max(window[0]),
        visible[1]
            .saturating_sub(DETAIL_OVERSCAN_PIXELS)
            .max(window[1]),
        visible[2]
            .saturating_add(DETAIL_OVERSCAN_PIXELS)
            .min(window[2]),
        visible[3]
            .saturating_add(DETAIL_OVERSCAN_PIXELS)
            .min(window[3]),
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

fn refinement(
    first: super::request::Request,
    visible_region: Option<[u32; 4]>,
) -> super::request::Refinement {
    let preview = (first.settings.preview_region.is_some()
        && first.rendition == crate::editor::Rendition::Current
        && first.output == "final"
        && !first.comparison)
        .then(|| {
            let mut preview = first.clone();
            preview.revision += 1;
            preview.auxiliary_preview = true;
            preview.detail = false;
            preview.preview_scale = Scale::new(8).expect("thumbnail scale");
            preview.settings.working_scale = preview.preview_scale;
            preview.settings.preview_region = None;
            preview
        });
    let whole = first.settings.preview_region.map(|_| {
        let mut whole = first.clone();
        whole.revision += 1 + u64::from(preview.is_some());
        whole.detail = false;
        whole.settings.preview_region = None;
        whole
    });
    super::request::Refinement {
        first,
        visible_region,
        preview,
        whole,
    }
}

impl App {
    pub(crate) fn detail_frame(&self) -> Option<&worker::Frame> {
        detail_frame(&self.before, &self.multi_edit, self.selected.as_deref())
    }

    pub(crate) fn smart_refinement_active(&self) -> bool {
        !self.framing_preview_active()
            && self.output == "final"
            && !self.comparison.requested()
            && (self.viewport().explicit_zoom() || self.detail_frame().is_some())
            && self.selected.as_ref().is_some_and(|path| {
                self.preview_scale_for(path, self.preferences.processing_quality) == Scale::NATIVE
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
        if self.loupe_inspecting(path) {
            return None;
        }
        self.compatible_detail(path, base, requested, rendition)
    }

    /// Presentation accepts partial overlap. Request settlement separately
    /// requires complete coverage of the newly visible region.
    pub(crate) fn compatible_detail(
        &self,
        path: &Path,
        base: Option<&worker::Frame>,
        requested: &Settings,
        rendition: crate::editor::Rendition,
    ) -> Option<&worker::Frame> {
        let detail = self.rendition_detail(path, rendition)?;
        let calibration = self
            .noise_calibration
            .selected(self.preferences.noise_profile);
        (detail.path == path
            && detail.output == "final"
            && detail.rendition == rendition
            && !detail.framing_preview
            && detail.comparison.is_none()
            && detail.requested.processing_quality == self.preferences.processing_quality
            && same_image_geometry(&detail.requested, requested)
            && detail.calibration_signature.as_deref()
                == calibration.as_ref().map(|profile| profile.signature())
            && self
                .worker
                .cache
                .source_origin(path)
                .is_none_or(|origin| origin == detail.origin)
            && base.is_none_or(|base| detail.can_overlay(base)))
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

    /// Resolve visible native bounds using the photo's viewport and compatible whole frame.
    fn photo_visible_region(
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
                && frame.requested.processing_quality == self.preferences.processing_quality
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
        Some(rect)
    }

    /// Current detail follows the complete photo set, independently of active selection.
    pub(crate) fn photo_refinement(
        &self,
        path: &Path,
        settings: Settings,
    ) -> Option<PhotoRefinement> {
        self.photo_refinement_for(path, settings, crate::editor::Rendition::Current)
    }

    pub(crate) fn photo_refinement_for(
        &self,
        path: &Path,
        mut settings: Settings,
        rendition: crate::editor::Rendition,
    ) -> Option<PhotoRefinement> {
        if self.loupe_inspecting(path) {
            return Some(PhotoRefinement::Deferred);
        }
        if !self
            .displayed_paths
            .iter()
            .any(|displayed| displayed == path)
            || !self.viewports.view(Some(path)).explicit_zoom()
            || self.preview_scale_for(path, self.preferences.processing_quality) != Scale::NATIVE
            || (self.selected.as_deref() == Some(path) && self.framing_preview_active())
        {
            return None;
        }
        let base = self.rendition_frame(path, rendition);
        if rendition != crate::editor::Rendition::Current && base.is_none() {
            return None;
        }
        settings = self.preferences.render_settings(&settings);
        if base.is_some_and(|frame| {
            frame.artifact.settings.working_scale == Scale::NATIVE
                && same_authored_edits(&frame.requested, &settings)
                && if rendition == crate::editor::Rendition::Current {
                    self.reusable_photograph(path, frame, &settings)
                } else {
                    self.before_frame_matches(path, frame, &settings)
                }
        }) {
            return None;
        }
        if base.is_some() && self.viewports.view(Some(path)).visible_pixels().is_none() {
            // Selection discards old screen bounds. Wait for this photo to be
            // drawn instead of replacing its pending region with fallback Whole.
            return Some(PhotoRefinement::Deferred);
        }
        settings.working_scale = Scale::NATIVE;
        let visible_region = self.photo_visible_region(path, base, &settings, rendition);
        settings.preview_region = visible_region
            .zip(base)
            .map(|(visible, frame)| render_region(visible, frame.display_window(false)));
        let first = super::request::Request {
            auxiliary_preview: false,

            detail: settings.preview_region.is_some(),
            framing_preview: false,
            calibration: self
                .noise_calibration
                .selected(self.preferences.noise_profile),
            rendition,
            revision: self.revision + 1,
            path: path.to_owned(),
            settings,
            preview_scale: foveonic::render::Scale::NATIVE,
            processing_quality: self.preferences.processing_quality,
            output: "final".into(),
            comparison: false,
            export: None,
        };
        Some(PhotoRefinement::Ready(Box::new(refinement(
            first,
            visible_region,
        ))))
    }

    pub(crate) fn cancel_smart(&self) {
        if let Some(path) = &self.selected {
            self.worker.cancel_refinement(path, self.before.active);
        }
    }

    /// Framing uses its own whole-image presentation while normal photos share refinement.
    pub(crate) fn update_framing_preview(&mut self) {
        if !self.sidecars.ready() || self.exporting {
            return;
        }
        let zoomed = self.viewport_mut().take_explicit_zoom();
        if self.framing_preview_active() && zoomed {
            if let Some(path) = &self.selected {
                let quality = self.preview_scale_for(path, self.preferences.processing_quality);
                if self.frame.as_ref().is_some_and(|frame| {
                    frame.artifact.settings.working_scale.divisor() > quality.divisor()
                }) {
                    self.submit_with_quality(None, None, self.preferences.processing_quality);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regional_current_reserves_independent_preview_before_native_completion() {
        let first = super::super::request::Request {
            auxiliary_preview: false,
            path: "photo.x3f".into(),
            revision: 10,
            settings: Settings {
                preview_region: Some([10, 20, 100, 200]),
                output_size: Some([3000, 2000]),
                exposure: 0.75,
                ..Default::default()
            },
            preview_scale: Scale::NATIVE,
            processing_quality: foveonic::render::ProcessingQuality::Speed,
            detail: true,
            framing_preview: false,
            calibration: None,
            rendition: crate::editor::Rendition::Current,
            output: "final".into(),
            comparison: false,
            export: None,
        };
        let sequence = refinement(first.clone(), Some([30, 40, 80, 180]));
        assert_eq!(sequence.visible_region, Some([30, 40, 80, 180]));
        let preview = sequence.preview.unwrap();
        assert_eq!(preview.revision, 11);
        assert!(preview.auxiliary_preview);
        assert!(!preview.detail);
        assert_eq!(preview.preview_scale.divisor(), 8);
        assert!(preview.settings.preview_region.is_none());
        assert_eq!(preview.settings.output_size, first.settings.output_size);
        assert!(same_authored_edits(&preview.settings, &first.settings));
        assert_eq!(preview.processing_quality, first.processing_quality);
        assert_eq!(sequence.whole.unwrap().revision, 12);
        let mut before = first.clone();
        before.rendition = crate::editor::Rendition::Camera;
        let sequence = refinement(before, None);
        assert!(sequence.preview.is_none());
        assert_eq!(sequence.whole.unwrap().revision, 11);
        let mut whole = first;
        whole.detail = false;
        whole.settings.preview_region = None;
        let sequence = refinement(whole, None);
        assert!(sequence.preview.is_none());
        assert!(sequence.whole.is_none());
    }

    #[test]
    fn detail_margin_stays_inside_the_native_image() {
        assert_eq!(
            render_region([100, 100, 400, 300], [0, 0, 1000, 800]),
            [36, 36, 464, 364]
        );
        assert_eq!(
            render_region([900, 700, 1000, 800], [0, 0, 1000, 800]),
            [836, 636, 1000, 800]
        );
        assert_eq!(
            render_region([100, 130, 180, 190], [100, 120, 300, 250]),
            [100, 120, 244, 250]
        );
    }
}
