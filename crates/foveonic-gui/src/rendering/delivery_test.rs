//! Checks delivery decisions under edits, view changes and out-of-order completions without a GPU.

use super::*;
use crate::rendering::request;

fn view(settings: &Settings) -> ViewContext<'_> {
    ViewContext {
        active: Some(Path::new("a.x3f")),
        rendition: Rendition::Current,
        output: "final",
        comparison: false,
        framing_preview: false,
        exporting: false,
        calibration: None,
        settings,
        overview_visible: true,
        overview_settings: Some(settings),
        overview_displayed_revision: None,
        overview_detail_revision: None,
        displayed_revision: Some(10),
        detail_revision: None,
    }
}

fn reply(settings: &Settings) -> ReplyInfo<'_> {
    ReplyInfo {
        path: Path::new("a.x3f"),
        purpose: &Purpose::Current,
        rendition: Rendition::Current,
        exported: false,
        image: Some(ImageInfo {
            path: Path::new("a.x3f"),
            output: "final",
            requested: settings,
            calibration: None,
            framing_preview: false,
            comparison: false,
            regional: false,
            revision: 11,
        }),
    }
}

#[test]
fn latest_result_updates_photo_and_active_image() {
    let settings = Settings::default();
    assert_eq!(
        classify(Completion::Current, &reply(&settings), &view(&settings)),
        Decision {
            retain_pixels: true,
            update_photo: true,
            destination: Ok(Destination::Active),
        }
    );
}

#[test]
fn removed_and_changed_calibration_results_cannot_update_photo_or_pixel_history() {
    let settings = Settings::default();
    let incoming = reply(&settings);
    let mut context = view(&settings);
    for (completion, calibration, expected) in [
        (Completion::Removed, None, Rejection::Removed),
        (
            Completion::Current,
            Some("new-profile"),
            Rejection::CalibrationChanged,
        ),
        (
            Completion::Superseded,
            Some("new-profile"),
            Rejection::CalibrationChanged,
        ),
    ] {
        context.calibration = calibration;
        assert_eq!(
            classify(completion, &incoming, &context),
            Decision {
                retain_pixels: false,
                update_photo: false,
                destination: Err(expected),
            }
        );
    }
}

#[test]
fn completed_older_edit_can_advance_feedback_without_changing_requested_photo_state() {
    let old = Settings::default();
    let mut requested = old.clone();
    requested.exposure = 1.;
    let decision = classify(Completion::Superseded, &reply(&old), &view(&requested));
    assert_eq!(decision.destination, Ok(Destination::Active));
    assert!(decision.retain_pixels);
    assert!(!decision.update_photo);
    assert_eq!(requested.exposure, 1.);
}

#[test]
fn progressive_feedback_never_rewinds_a_whole_image_or_detail_tile() {
    let settings = Settings::default();
    let mut incoming = reply(&settings);
    let mut context = view(&settings);
    for displayed in [11, 12] {
        context.displayed_revision = Some(displayed);
        assert_eq!(
            classify(Completion::Superseded, &incoming, &context).destination,
            Err(Rejection::Superseded)
        );
    }
    context.displayed_revision = Some(10);
    context.detail_revision = Some(12);
    incoming.image.as_mut().unwrap().regional = true;
    assert_eq!(
        classify(Completion::Superseded, &incoming, &context).destination,
        Err(Rejection::Superseded)
    );
    incoming.image.as_mut().unwrap().regional = false;
    assert_eq!(
        classify(Completion::Superseded, &incoming, &context).destination,
        Ok(Destination::Active),
        "a whole image may arrive behind a newer detail tile"
    );
}

#[test]
fn obsolete_geometry_root_comparison_or_framing_cannot_become_active_feedback() {
    let settings = Settings::default();
    let incoming = reply(&settings);
    let mut changed = settings.clone();
    changed.crop.bounds = [0.1, 0.1, 0.9, 0.9];
    let mut context = view(&changed);
    assert!(classify(Completion::Superseded, &incoming, &context)
        .destination
        .is_err());
    context.settings = &settings;
    context.output = "sensor";
    let mut inspection = reply(&settings);
    inspection.purpose = &Purpose::Detail;
    assert!(classify(Completion::Superseded, &inspection, &context)
        .destination
        .is_err());
    context.output = "final";
    context.comparison = true;
    assert!(classify(Completion::Superseded, &inspection, &context)
        .destination
        .is_err());
    context.comparison = false;
    context.framing_preview = true;
    assert_eq!(
        classify(Completion::Current, &incoming, &context).destination,
        Err(Rejection::FramingViewChanged)
    );
    context.framing_preview = false;
    context.exporting = true;
    assert_eq!(
        classify(Completion::Superseded, &incoming, &context).destination,
        Err(Rejection::Superseded)
    );
}

#[test]
fn other_selected_photo_updates_its_tile_while_the_active_photo_stays_put() {
    let settings = Settings::default();
    let mut context = view(&settings);
    context.active = Some(Path::new("b.x3f"));
    let decision = classify(Completion::Current, &reply(&settings), &context);
    assert_eq!(decision.destination, Ok(Destination::Overview));
    assert!(decision.update_photo);
    context.overview_visible = false;
    let decision = classify(Completion::Current, &reply(&settings), &context);
    assert_eq!(decision.destination, Err(Rejection::NotVisible));
    assert!(
        decision.retain_pixels,
        "valid nonvisible work can still seed reuse"
    );
}

#[test]
fn current_overview_can_arrive_while_active_photo_is_showing_before_or_inspection() {
    let settings = Settings::default();
    for rendition in [Rendition::Camera, Rendition::Defaults] {
        let mut context = view(&settings);
        context.rendition = rendition;
        assert_eq!(
            classify(Completion::Current, &reply(&settings), &context).destination,
            Ok(Destination::Overview)
        );
    }
    let mut context = view(&settings);
    context.output = "sensor";
    assert_eq!(
        classify(Completion::Current, &reply(&settings), &context).destination,
        Ok(Destination::Overview)
    );
    context.output = "final";
    context.comparison = true;
    assert_eq!(
        classify(Completion::Current, &reply(&settings), &context).destination,
        Ok(Destination::Overview)
    );
}

#[test]
fn overview_requires_matching_framing_but_allows_progressive_exposure() {
    let settings = Settings::default();
    let mut changed = settings.clone();
    changed.exposure = 2.;
    let mut context = view(&changed);
    context.active = Some(Path::new("b.x3f"));
    let incoming = reply(&settings);
    assert_eq!(
        classify(Completion::Superseded, &incoming, &context).destination,
        Ok(Destination::Overview)
    );
    let mut rotated = changed.clone();
    rotated.rotation.degrees = 2.;
    context.overview_settings = Some(&rotated);
    assert_eq!(
        classify(Completion::Current, &incoming, &context).destination,
        Err(Rejection::GeometryChanged)
    );
    context.overview_settings = None;
    assert_eq!(
        classify(Completion::Current, &incoming, &context).destination,
        Err(Rejection::GeometryChanged)
    );
}

#[test]
fn late_before_reply_cannot_replace_restored_current_view() {
    let settings = Settings::default();
    let mut incoming = reply(&settings);
    incoming.rendition = Rendition::Camera;
    incoming.purpose = &Purpose::Camera;
    assert_eq!(
        classify(Completion::Superseded, &incoming, &view(&settings)).destination,
        Err(Rejection::RenditionChanged)
    );
}

#[test]
fn current_errors_are_routed_but_obsolete_errors_have_no_progressive_image() {
    let settings = Settings::default();
    let mut incoming = reply(&settings);
    incoming.image = None;
    let mut context = view(&settings);
    let decision = classify(Completion::Current, &incoming, &context);
    assert_eq!(decision.destination, Ok(Destination::Active));
    assert!(!decision.retain_pixels);
    assert!(decision.update_photo);
    assert_eq!(
        classify(Completion::Superseded, &incoming, &context).destination,
        Err(Rejection::Superseded)
    );
    context.active = Some(Path::new("b.x3f"));
    context.overview_settings = None;
    assert_eq!(
        classify(Completion::Current, &incoming, &context).destination,
        Ok(Destination::Overview),
        "errors must not depend on available image settings"
    );
}

#[test]
fn frozen_export_completion_survives_calibration_changes() {
    let settings = Settings::default();
    let mut incoming = reply(&settings);
    incoming.exported = true;
    incoming.purpose = &Purpose::Export;
    incoming.image = None;
    let mut context = view(&settings);
    context.calibration = Some("new-profile");
    assert_eq!(
        classify(Completion::Current, &incoming, &context).destination,
        Ok(Destination::Active)
    );
}

#[test]
fn overtaken_edit_advances_feedback_then_latest_group_settles_without_rewinding() {
    use crate::{preferences::PreviewMode, rendering::coordinator::RenderCoordinator};
    use foveonic::source::SourceOrigin;
    use std::{sync::Arc, time::Instant};

    let settings = Settings::default();
    let request = |path: &str, revision, exposure| {
        let mut requested = settings.clone();
        requested.exposure = exposure;
        Arc::new(request::Request {
            path: path.into(),
            revision,
            settings: requested,
            rendition: Rendition::Current,
            output: "final".into(),
            framing_preview: false,
            calibration: None,
            preview_quality: PreviewMode::Thumb,
            preview_policy: PreviewMode::Thumb,
            comparison: false,
            export: None,
        })
    };
    let mut coordinator = RenderCoordinator::default();
    coordinator.desire(request("a.x3f", 11, 0.), None, Instant::now());
    let old_a = coordinator.plan().tasks[0].stamp.clone();
    coordinator.desire(request("b.x3f", 11, 0.), None, Instant::now());
    let b = coordinator.plan().tasks[1].stamp.clone();
    coordinator.desire(request("a.x3f", 12, 1.), None, Instant::now());
    let latest_a = coordinator.desired(&old_a.target).unwrap().stamp.clone();
    let mut context = view(&settings);
    let mut incoming = reply(&settings);
    let origin = SourceOrigin::default();

    let decision = classify(coordinator.finish(&old_a), &incoming, &context);
    assert_eq!(decision.destination, Ok(Destination::Active));
    assert!(!decision.update_photo);
    assert!(coordinator.admit(&old_a, &origin));
    context.displayed_revision = Some(11);
    assert!(
        coordinator.busy(),
        "feedback cannot settle the replacement or the other photo"
    );

    incoming.image.as_mut().unwrap().revision = 12;
    assert_eq!(
        classify(coordinator.finish(&latest_a), &incoming, &context).destination,
        Ok(Destination::Active)
    );
    assert!(coordinator.admit(&latest_a, &origin));
    context.displayed_revision = Some(12);
    incoming.image.as_mut().unwrap().revision = 11;
    assert_eq!(
        classify(coordinator.finish(&old_a), &incoming, &context).destination,
        Err(Rejection::Superseded)
    );
    assert!(!coordinator.admit(&old_a, &origin));

    incoming.path = Path::new("b.x3f");
    incoming.image.as_mut().unwrap().path = incoming.path;
    assert_eq!(
        classify(coordinator.finish(&b), &incoming, &context).destination,
        Ok(Destination::Overview)
    );
    assert!(coordinator.admit(&b, &origin));
    assert!(!coordinator.busy());
    coordinator.retain_overviews(&["a.x3f".into()], &[]);
    assert_eq!(
        classify(coordinator.finish(&b), &incoming, &context).destination,
        Err(Rejection::Removed)
    );
    assert_eq!(context.displayed_revision, Some(12));
}

#[test]
fn inactive_detail_uses_its_own_geometry_and_content_order() {
    let settings = Settings::default();
    let mut active = settings.clone();
    active.rotation.turns = 1;
    let mut context = view(&active);
    context.active = Some(Path::new("b.x3f"));
    context.displayed_revision = Some(100);
    context.detail_revision = Some(101);
    context.overview_settings = Some(&settings);
    context.overview_displayed_revision = Some(10);
    context.overview_detail_revision = Some(12);
    let mut incoming = reply(&settings);
    incoming.purpose = &Purpose::Detail;
    incoming.image.as_mut().unwrap().regional = true;
    assert_eq!(
        classify(Completion::Superseded, &incoming, &context).destination,
        Err(Rejection::Superseded)
    );
    incoming.image.as_mut().unwrap().revision = 13;
    assert_eq!(
        classify(Completion::Superseded, &incoming, &context).destination,
        Ok(Destination::Overview)
    );
    // Detail identity survives a worker fallback to a whole-image artifact.
    incoming.image.as_mut().unwrap().regional = false;
    incoming.image.as_mut().unwrap().revision = 11;
    assert_eq!(
        classify(Completion::Superseded, &incoming, &context).destination,
        Err(Rejection::Superseded)
    );
    incoming.purpose = &Purpose::Current;
    assert_eq!(
        classify(Completion::Superseded, &incoming, &context).destination,
        Ok(Destination::Overview),
        "a whole image can advance the base while a newer detail remains visible"
    );
    context.overview_displayed_revision = Some(14);
    assert_eq!(
        classify(Completion::Superseded, &incoming, &context).destination,
        Err(Rejection::Superseded)
    );
}

#[test]
fn view_snapshot_keeps_each_photos_content_order_separate() {
    let settings = Settings::default();
    let snapshot = ViewState {
        active: Some("b.x3f".into()),
        rendition: Rendition::Camera,
        output: "final".into(),
        comparison: false,
        framing_preview: false,
        exporting: false,
        calibration: None,
        settings: settings.clone(),
        overviews: vec![PhotoView {
            path: "a.x3f".into(),
            settings: settings.clone(),
            displayed_revision: Some(10),
            detail_revision: Some(12),
        }],
        displayed_revision: Some(100),
        detail_revision: Some(101),
    };
    let mut incoming = reply(&settings);
    incoming.purpose = &Purpose::Detail;
    incoming.image.as_mut().unwrap().regional = true;
    incoming.image.as_mut().unwrap().revision = 13;
    assert_eq!(
        classify(
            Completion::Superseded,
            &incoming,
            &snapshot.context(incoming.path)
        )
        .destination,
        Ok(Destination::Overview)
    );
    assert!(!snapshot.context(Path::new("removed.x3f")).overview_visible);
}
