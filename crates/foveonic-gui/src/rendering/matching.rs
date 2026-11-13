//! Defines shared settings and request comparisons without selecting work or admitting results.

use super::request::Request;
use foveonic::{
    noise_calibration::Calibration,
    render::{Scale, Settings},
};
use std::sync::Arc;

/// Photographic edits include framing and output size. Working grid, region,
/// estimate requests and descriptive range names do not change these edits.
pub(crate) fn same_authored_edits(a: &Settings, b: &Settings) -> bool {
    authored_edits(a) == authored_edits(b)
}

fn authored_edits(settings: &Settings) -> Settings {
    let mut edits = settings.clone();
    edits.working_scale = Scale::NATIVE;
    edits.preview_region = None;
    edits.dehaze_estimate = false;
    edits.photographic.color_editor.advanced.clear_names();
    edits
}

/// A detail tile's mapping remains valid across artistic edits, but not across
/// changes to crop, rotation or output size.
pub(crate) fn same_image_geometry(a: &Settings, b: &Settings) -> bool {
    a.crop == b.crop && a.rotation == b.rotation && a.output_size == b.output_size
}

/// Crop and rotation can reframe existing pixels. Output size remains numerical.
/// Source, processing grid and consumer context must be checked by the caller.
pub(crate) fn same_pixel_settings(a: &Settings, b: &Settings) -> bool {
    let a = authored_edits(a);
    let mut b = authored_edits(b);
    b.crop = a.crop;
    b.rotation = a.rotation;
    a == b
}

/// Requests share an immutable calibration snapshot by allocation identity.
/// Delivered frames separately use the profile signature for display admission.
pub(crate) fn same_calibration_snapshot(
    a: &Option<Arc<Calibration>>,
    b: &Option<Arc<Calibration>>,
) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}

pub(crate) fn same_request_context(a: &Request, b: &Request) -> bool {
    a.path == b.path
        && a.detail == b.detail
        && a.output == b.output
        && a.rendition == b.rendition
        && a.comparison == b.comparison
        && a.preview_policy == b.preview_policy
        && a.export.is_none()
        && b.export.is_none()
        && same_calibration_snapshot(&a.calibration, &b.calibration)
}

pub(crate) fn presentation_changed(a: &Request, b: &Request) -> bool {
    a.framing_preview != b.framing_preview
        || a.settings.crop != b.settings.crop
        || a.settings.rotation != b.settings.rotation
}

/// Composition changes preserve compatible work at its existing grid. Otherwise
/// explicit quality and region requests must agree. Source identity remains a
/// separate coordinator check because a path can be replaced.
pub(crate) fn can_share_pixel_work(a: &Request, b: &Request) -> bool {
    same_request_context(a, b)
        && same_pixel_settings(&a.settings, &b.settings)
        && (presentation_changed(a, b)
            || (a.preview_quality == b.preview_quality
                && a.settings.preview_region == b.settings.preview_region))
}

/// Coalescing an unchanged delivery also requires exact requested settings.
/// Revisions and source identity are managed by the coordinator.
pub(crate) fn same_delivery_request(a: &Request, b: &Request) -> bool {
    same_request_context(a, b)
        && a.framing_preview == b.framing_preview
        && a.settings == b.settings
        && a.preview_quality == b.preview_quality
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{editor::Rendition, preferences::PreviewMode};

    fn request() -> Request {
        Request {
            detail: false,
            framing_preview: false,
            calibration: None,
            rendition: Rendition::Current,
            revision: 1,
            path: "test.x3f".into(),
            settings: Settings::new("Sunlight", "Standard"),
            preview_quality: PreviewMode::Thumb,
            preview_policy: PreviewMode::Smart,
            output: "final".into(),
            comparison: false,
            export: None,
        }
    }

    #[test]
    fn pixel_boundary_preserves_control_source_and_workspace_policy_guards() {
        let a = request();
        let mut b = a.clone();
        b.revision += 1;
        b.preview_quality = PreviewMode::Smart;
        b.framing_preview = true;
        b.settings.crop.bounds = [0.1, 0.2, 0.9, 0.8];
        b.settings.rotation.degrees = 7.;
        assert!(
            same_request_context(&a, &b)
                && same_pixel_settings(&a.settings, &b.settings)
                && presentation_changed(&a, &b)
        );
        b.settings.exposure = 0.5;
        assert!(!same_pixel_settings(&a.settings, &b.settings));
        b.settings = a.settings.clone();
        b.preview_policy = PreviewMode::Full;
        assert!(!same_request_context(&a, &b));
        b.preview_policy = PreviewMode::Smart;
        b.path = "another.x3f".into();
        assert!(!same_request_context(&a, &b));
    }

    #[test]
    fn transient_processing_requests_preserve_edits_but_not_identical_delivery() {
        let a = request();
        let mut b = a.clone();
        b.settings.working_scale = Scale::new(8).unwrap();
        b.detail = true;
        b.settings.preview_region = Some([100, 100, 400, 300]);
        b.settings.dehaze_estimate = true;
        assert!(same_authored_edits(&a.settings, &b.settings));
        assert!(same_pixel_settings(&a.settings, &b.settings));
        assert!(same_image_geometry(&a.settings, &b.settings));
        assert!(!can_share_pixel_work(&a, &b));
        assert!(!same_delivery_request(&a, &b));
        b.detail = false;
        b.settings.preview_region = None;
        assert!(can_share_pixel_work(&a, &b));
        assert!(!same_delivery_request(&a, &b));
    }

    #[test]
    fn framing_reuses_pixels_while_output_resize_requires_processing() {
        let a = request();
        let mut b = a.clone();
        b.framing_preview = true;
        assert!(can_share_pixel_work(&a, &b));
        assert!(!same_delivery_request(&a, &b));
        b.settings.crop.bounds = [0.1, 0.2, 0.9, 0.8];
        b.settings.rotation.degrees = 7.;
        b.preview_quality = PreviewMode::Full;
        assert!(!same_authored_edits(&a.settings, &b.settings));
        assert!(!same_image_geometry(&a.settings, &b.settings));
        assert!(same_pixel_settings(&a.settings, &b.settings));
        assert!(can_share_pixel_work(&a, &b));
        assert!(!same_delivery_request(&a, &b));
        b.settings.output_size = Some([640, 480]);
        assert!(!same_pixel_settings(&a.settings, &b.settings));
        assert!(!can_share_pixel_work(&a, &b));
    }

    #[test]
    fn exposure_preserves_geometry_and_revision_alone_preserves_delivery() {
        let a = request();
        let mut b = a.clone();
        b.revision += 1;
        assert!(same_delivery_request(&a, &b));
        b.preview_quality = PreviewMode::Full;
        assert!(!same_delivery_request(&a, &b));
        b.preview_quality = a.preview_quality;
        b.settings.exposure = 0.25;
        assert!(same_image_geometry(&a.settings, &b.settings));
        assert!(!same_authored_edits(&a.settings, &b.settings));
        assert!(!can_share_pixel_work(&a, &b));
    }
}
