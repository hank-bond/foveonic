//! Exercises observed render interleavings without threads, timers or a GPU.

use super::*;
use crate::editor::Rendition;
use crate::preferences::PreviewMode;
use foveonic::render::Settings;

fn request(path: &str, revision: u64, exposure: f32) -> Arc<Request> {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.exposure = exposure;
    Arc::new(Request {
        framing_preview: false,
        calibration: None,
        rendition: Rendition::Current,
        revision,
        path: path.into(),
        settings,
        preview_quality: PreviewMode::Thumb,
        preview_policy: PreviewMode::Thumb,
        output: "final".into(),
        comparison: false,
        export: None,
    })
}
fn desire(coordinator: &mut RenderCoordinator, request: Arc<Request>) -> Stamp {
    let target = Target::of(&request);
    coordinator.desire(request, None, Instant::now());
    coordinator.desired(&target).unwrap().stamp.clone()
}

fn render_result(stamp: Stamp, request: &Request) -> Reply {
    Reply {
        request: Arc::new(request.clone()),
        handling: None,
        stamp,
        path: request.path.clone(),
        rendition: request.rendition,
        revision: request.revision,
        metadata: None,
        settings: request.settings.clone(),
        result: Ok(None),
        numeric_settings: None,
        reset_settings: None,
        exported: request.export.clone(),
    }
}

fn result_view(reply: &Reply) -> ViewContext<'_> {
    ViewContext {
        active: Some(&reply.path),
        rendition: reply.rendition,
        output: "final",
        comparison: false,
        framing_preview: false,
        exporting: false,
        calibration: None,
        settings: &reply.settings,
        overview_selected: true,
        overview_settings: Some(&reply.settings),
        displayed_revision: None,
        detail_revision: None,
    }
}

#[test]
fn result_handler_settles_current_failures_without_settling_newer_or_sibling_requests() {
    let mut coordinator = RenderCoordinator::default();
    let a = request("a.x3f", 1, 0.);
    let mut old = render_result(desire(&mut coordinator, a.clone()), &a);
    old.result = Err("cannot decode source".into());
    let b = request("b.x3f", 1, 0.);
    let mut sibling = render_result(desire(&mut coordinator, b.clone()), &b);
    sibling.result = Err("cannot load source".into());
    let a = request("a.x3f", 2, 0.5);
    let latest = render_result(desire(&mut coordinator, a.clone()), &a);

    assert_eq!(
        coordinator.handle_result(&old, &result_view(&old)),
        ResultUpdate {
            current: false,
            update_photo: false,
            destination: None
        }
    );
    assert_eq!(coordinator.plan().tasks.len(), 2);
    assert_eq!(
        coordinator.handle_result(&sibling, &result_view(&sibling)),
        ResultUpdate {
            current: true,
            update_photo: true,
            destination: Some(Destination::Active)
        }
    );
    assert!(coordinator.busy());
    assert_eq!(coordinator.plan().tasks[0].stamp, latest.stamp);
    assert!(!coordinator.is_settled_and_admitted(sibling.stamp.id, sibling.revision));
    assert!(
        coordinator
            .handle_result(&latest, &result_view(&latest))
            .current
    );
    assert!(!coordinator.busy());
}

#[test]
fn result_handler_keeps_photo_feedback_when_view_rejects_but_not_when_target_is_removed() {
    let mut coordinator = RenderCoordinator::default();
    let request = request("a.x3f", 1, 0.);
    let reply = render_result(desire(&mut coordinator, request.clone()), &request);
    let mut view = result_view(&reply);
    view.active = Some(Path::new("other.x3f"));
    view.overview_selected = false;
    assert_eq!(
        coordinator.handle_result(&reply, &view),
        ResultUpdate {
            current: true,
            update_photo: true,
            destination: None
        }
    );
    assert!(!coordinator.busy());
    coordinator.clear();
    assert_eq!(
        coordinator.handle_result(&reply, &view),
        ResultUpdate {
            current: false,
            update_photo: false,
            destination: None
        }
    );
}

#[test]
fn result_handler_routes_frozen_export_without_settling_pending_preview() {
    let mut coordinator = RenderCoordinator::default();
    let preview = request("a.x3f", 1, 0.);
    let pending = desire(&mut coordinator, preview.clone());
    let mut export = (*preview).clone();
    export.export = Some(crate::export::Export {
        path: "output.tiff".into(),
        format: crate::export::ExportFormat::Tiff,
        encoding: Default::default(),
        tiff_color_space: None,
        conflict: Default::default(),
        sections: Default::default(),
    });
    let task = coordinator.frozen_export(Arc::new(export), None, Instant::now());
    let reply = render_result(task.stamp.clone(), &task.request);
    assert_eq!(
        coordinator.handle_result(&reply, &result_view(&reply)),
        ResultUpdate {
            current: true,
            update_photo: true,
            destination: Some(Destination::Active)
        }
    );
    assert!(coordinator.busy());
    assert_eq!(coordinator.plan().tasks[0].stamp, pending);
}

#[test]
fn removing_b_while_a_remains_active_rejects_b_from_the_same_cohort() {
    let mut coordinator = RenderCoordinator::default();
    let a = desire(&mut coordinator, request("a.x3f", 10, 0.));
    let b = desire(&mut coordinator, request("b.x3f", 10, 0.));
    assert_eq!(coordinator.finish(&a), Completion::Current);
    assert!(coordinator.admit(&a, &SourceOrigin::default()));
    coordinator.retain_overviews(&["a.x3f".into()], &[]);
    assert_eq!(coordinator.finish(&b), Completion::Removed);
    assert!(!coordinator.admit(&b, &SourceOrigin::default()));
    assert!(!coordinator.busy());
    assert_eq!(coordinator.desired(&a.target).unwrap().stamp, a);
}

#[test]
fn selecting_one_editor_target_in_an_overview_reuses_its_unchanged_thumb_task() {
    use crate::viewport_selection::{Gesture, Request as SelectionRequest, Selection};

    let paths = vec!["a.x3f".into(), "b.x3f".into()];
    let mut selection = Selection::default();
    selection.sync_browser(&paths, 8);
    let mut coordinator = RenderCoordinator::default();
    let a = desire(&mut coordinator, request("a.x3f", 1, 0.));
    let b = desire(&mut coordinator, request("b.x3f", 2, 0.));
    coordinator.finish(&a);
    coordinator.finish(&b);

    selection.apply(
        SelectionRequest {
            path: paths[1].clone(),
            gesture: Gesture::Replace,
        },
        8,
    );
    assert_eq!(selection.paths(), &paths[1..]);
    assert!(crate::multi_photo_overview(&paths, Some(&paths[1])));
    coordinator.retain_overviews(selection.paths(), &paths);
    coordinator.activate(&paths[1]);
    let restored = desire(&mut coordinator, request("b.x3f", 3, 0.));
    assert_eq!(restored, b);
    assert!(!coordinator.busy());
    assert!(coordinator.plan().tasks.is_empty());
    coordinator.retain_overviews(&paths[..1], &paths);
    coordinator.activate(&paths[0]);
    let restored_a = desire(&mut coordinator, request("a.x3f", 4, 0.));
    assert_eq!(restored_a, a);
    coordinator.retain_overviews(&paths, &paths);
    let restored_b = desire(&mut coordinator, request("b.x3f", 5, 0.));
    assert_eq!(restored_b, b);
    assert!(
        !coordinator.busy(),
        "unchanged re-added targets need no work"
    );
    assert!(!crate::multi_photo_overview(&paths[1..], Some(&paths[1])));
}

#[test]
fn retained_photos_keep_only_completed_intents_outside_the_edit_targets() {
    let paths = vec!["a.x3f".into(), "b.x3f".into()];
    let mut coordinator = RenderCoordinator::default();
    let a = desire(&mut coordinator, request("a.x3f", 1, 0.));
    let b = desire(&mut coordinator, request("b.x3f", 2, 0.));
    coordinator.finish(&a);
    coordinator.retain_overviews(&[], &paths);
    assert_eq!(coordinator.finish(&b), Completion::Removed);
    assert!(coordinator.desired(&a.target).is_some());
    assert!(!coordinator.busy());

    coordinator.retain_overviews(&[], &paths[1..]);
    assert!(coordinator.desired(&a.target).is_none());
}

#[test]
fn before_retirement_preserves_unfinished_current_photos() {
    let mut coordinator = RenderCoordinator::default();
    let a = desire(&mut coordinator, request("a.x3f", 1, 0.));
    let b = desire(&mut coordinator, request("b.x3f", 1, 0.));
    coordinator.finish(&a);
    let mut before = Arc::try_unwrap(request("a.x3f", 2, 0.)).ok().unwrap();
    before.rendition = Rendition::Camera;
    let before = desire(&mut coordinator, Arc::new(before));
    coordinator.retire_before(Path::new("a.x3f"));
    assert_eq!(coordinator.finish(&before), Completion::Removed);
    assert!(coordinator.busy());
    assert_eq!(coordinator.plan().tasks.len(), 1);
    assert_eq!(coordinator.plan().tasks[0].stamp, b);
    assert_eq!(coordinator.finish(&b), Completion::Current);
    assert!(!coordinator.busy());
}

#[test]
fn editing_a_does_not_supersede_b_and_late_a_cannot_settle_the_new_intent() {
    let mut coordinator = RenderCoordinator::default();
    let old_a = desire(&mut coordinator, request("a.x3f", 1, 0.));
    let b = desire(&mut coordinator, request("b.x3f", 1, 0.));
    let a = desire(&mut coordinator, request("a.x3f", 2, 0.5));
    assert_ne!(old_a.id, a.id);
    assert_eq!(coordinator.finish(&old_a), Completion::Superseded);
    assert_eq!(coordinator.finish(&b), Completion::Current);
    assert!(coordinator.busy());
    assert_eq!(coordinator.plan().tasks.len(), 1);
    assert_eq!(coordinator.plan().tasks[0].stamp, a);
    assert_eq!(coordinator.finish(&a), Completion::Current);
    assert!(!coordinator.busy());
}

#[test]
fn crop_and_rotation_keep_pixel_work_but_require_latest_composition_delivery() {
    let mut coordinator = RenderCoordinator::default();
    let mut cursor = Cursor::default();
    let original = request("a.x3f", 1, 0.);
    let old = desire(&mut coordinator, original);
    assert_eq!(cursor.next(&coordinator.plan()).unwrap().stamp, old);
    let mut composed = Arc::try_unwrap(request("a.x3f", 2, 0.)).ok().unwrap();
    composed.settings.crop.bounds = [0.1, 0.2, 0.9, 0.8];
    composed.settings.rotation.turns = 1;
    let latest = desire(&mut coordinator, Arc::new(composed));
    assert_eq!(
        old.id, latest.id,
        "composition does not obsolete useful pixels"
    );
    assert_eq!(coordinator.finish(&old), Completion::Superseded);
    assert!(coordinator.busy(), "the latest view has not been delivered");
    let task = cursor.next(&coordinator.plan()).unwrap();
    assert_eq!(task.stamp, latest);
    assert_eq!(task.request.settings.crop.bounds, [0.1, 0.2, 0.9, 0.8]);
    assert_eq!(coordinator.finish(&latest), Completion::Current);
    assert!(!coordinator.busy());
}

#[test]
fn cached_images_require_the_exact_settled_and_admitted_delivery() {
    let mut coordinator = RenderCoordinator::default();
    let original = desire(&mut coordinator, request("a.x3f", 1, 0.));
    assert!(!coordinator.is_settled_and_admitted(original.id, original.revision));
    coordinator.finish(&original);
    assert!(!coordinator.is_settled_and_admitted(original.id, original.revision));
    assert!(coordinator.admit(&original, &SourceOrigin::default()));
    assert!(coordinator.is_settled_and_admitted(original.id, original.revision));

    let unchanged = desire(&mut coordinator, request("a.x3f", 2, 0.));
    assert_eq!(unchanged, original);
    assert!(coordinator.is_settled_and_admitted(original.id, original.revision));

    let changed = desire(&mut coordinator, request("a.x3f", 3, 0.5));
    assert!(!coordinator.is_settled_and_admitted(original.id, original.revision));
    coordinator.finish(&changed);
    assert!(coordinator.admit(&changed, &SourceOrigin::default()));
    assert!(coordinator.is_settled_and_admitted(changed.id, changed.revision));

    let mut composition = (*request("a.x3f", 4, 0.5)).clone();
    composition.settings.rotation.degrees = 3.;
    let composition = desire(&mut coordinator, Arc::new(composition));
    assert_eq!(composition.id, changed.id);
    assert!(!coordinator.is_settled_and_admitted(changed.id, changed.revision));
    coordinator.finish(&composition);
    assert!(coordinator.admit(&composition, &SourceOrigin::default()));
    assert!(coordinator.is_settled_and_admitted(composition.id, composition.revision));
    coordinator.clear();
    assert!(!coordinator.is_settled_and_admitted(composition.id, composition.revision));
}

#[test]
fn identical_intents_and_memory_observation_do_not_restart_settled_pixels() {
    let mut coordinator = RenderCoordinator::default();
    let original = request("a.x3f", 1, 0.);
    let stamp = desire(&mut coordinator, original.clone());
    coordinator.finish(&stamp);
    let repeated = desire(&mut coordinator, original);
    assert_eq!(repeated, stamp);
    assert!(!coordinator.busy());
    for _ in 0..4 {
        assert!(coordinator.plan().tasks.is_empty());
        assert_eq!(coordinator.desired(&stamp.target).unwrap().stamp, stamp);
    }
}

#[test]
fn replacement_plans_preserve_fair_progress_and_settle_every_latest_target() {
    let mut coordinator = RenderCoordinator::default();
    let mut cursor = Cursor::default();
    let paths = ["a.x3f", "b.x3f", "c.x3f"];
    for path in paths {
        desire(&mut coordinator, request(path, 1, 0.));
    }
    let mut serviced = Vec::new();
    for revision in 2..=13 {
        let running = cursor.next(&coordinator.plan()).unwrap();
        serviced.push(running.stamp.target.path.clone());
        // A user update overtakes this execution before it completes. The
        // executor may ignore advisory cancellation and still publish progress.
        for path in paths {
            desire(
                &mut coordinator,
                request(path, revision, revision as f32 * 0.1),
            );
        }
        assert_eq!(coordinator.finish(&running.stamp), Completion::Superseded);
        assert!(coordinator.admit(&running.stamp, &SourceOrigin::default()));
    }
    assert_eq!(
        serviced,
        paths
            .into_iter()
            .cycle()
            .take(12)
            .map(PathBuf::from)
            .collect::<Vec<_>>()
    );
    let mut settled = Vec::new();
    while let Some(task) = cursor.next(&coordinator.plan()) {
        assert_eq!(coordinator.finish(&task.stamp), Completion::Current);
        assert!(coordinator.admit(&task.stamp, &SourceOrigin::default()));
        settled.push(task.stamp.target.path.clone());
    }
    settled.sort();
    assert_eq!(settled, paths.map(PathBuf::from));
    assert!(!coordinator.busy());
}

#[test]
fn late_progress_cannot_replace_an_already_admitted_newer_result() {
    let mut coordinator = RenderCoordinator::default();
    let old = desire(&mut coordinator, request("a.x3f", 10, 0.));
    let new = desire(&mut coordinator, request("a.x3f", 11, 0.5));
    assert!(coordinator.admit(&new, &SourceOrigin::default()));
    assert!(!coordinator.admit(&old, &SourceOrigin::default()));
    assert!(!coordinator.admit(&new, &SourceOrigin::default()));
}

#[test]
fn removed_then_readded_photo_cannot_reuse_the_retired_pixel_task_id() {
    let mut coordinator = RenderCoordinator::default();
    let old = desire(&mut coordinator, request("a.x3f", 1, 0.));
    coordinator.retain_overviews(&[], &[]);
    let new = desire(&mut coordinator, request("a.x3f", 2, 0.));
    assert_ne!(old.id, new.id);
    assert_eq!(coordinator.finish(&old), Completion::Superseded);
    assert!(coordinator.busy());
}

#[test]
fn verified_source_replacement_restarts_pixels_for_the_same_photo_purpose() {
    let mut coordinator = RenderCoordinator::default();
    let first = request("a.x3f", 1, 0.);
    let target = Target::of(&first);
    let first_origin = SourceOrigin::default();
    let old = coordinator.desire(first, Some(first_origin), Instant::now());
    let new = coordinator.desire(
        request("a.x3f", 2, 0.),
        Some(SourceOrigin::default()),
        Instant::now(),
    );
    assert_ne!(old, new);
    assert_eq!(coordinator.desired(&target).unwrap().stamp.id, new);
    assert!(coordinator.busy());
}

#[test]
fn an_explicit_native_pass_without_composition_change_gets_new_pixel_work() {
    let mut coordinator = RenderCoordinator::default();
    let old = desire(&mut coordinator, request("a.x3f", 1, 0.));
    let mut native = Arc::try_unwrap(request("a.x3f", 2, 0.)).ok().unwrap();
    native.preview_quality = PreviewMode::Full;
    let new = desire(&mut coordinator, Arc::new(native));
    assert_ne!(old.id, new.id);
    assert_eq!(coordinator.finish(&old), Completion::Superseded);
    assert!(coordinator.busy());
}

#[test]
fn continuous_composition_followups_cannot_starve_another_pending_photo() {
    let mut coordinator = RenderCoordinator::default();
    let mut cursor = Cursor::default();
    let a = desire(&mut coordinator, request("a.x3f", 1, 0.));
    let b = desire(&mut coordinator, request("b.x3f", 2, 0.));
    assert_eq!(cursor.next(&coordinator.plan()).unwrap().stamp, a);
    let compose = |revision, left| {
        let mut next = Arc::try_unwrap(request("a.x3f", revision, 0.))
            .ok()
            .unwrap();
        next.settings.crop.bounds = [left, 0., 1., 1.];
        Arc::new(next)
    };
    let first = desire(&mut coordinator, compose(3, 0.1));
    assert_eq!(first.id, a.id);
    assert_eq!(
        cursor.followup(&coordinator.plan(), &a).unwrap().stamp,
        first
    );
    let latest = desire(&mut coordinator, compose(4, 0.2));
    assert_eq!(latest.id, a.id);
    assert!(cursor.followup(&coordinator.plan(), &first).is_none());
    assert_eq!(cursor.next(&coordinator.plan()).unwrap().stamp, b);
}

#[test]
fn cold_completed_source_binding_prevents_settled_coalescing_after_source_replacement() {
    let mut coordinator = RenderCoordinator::default();
    let cold = desire(&mut coordinator, request("a.x3f", 1, 0.));
    let a = SourceOrigin::default();
    coordinator.bind_origin(&cold, &a);
    assert_eq!(coordinator.finish(&cold), Completion::Current);
    assert!(!coordinator.busy());
    assert_eq!(
        coordinator.desired(&cold.target).unwrap().origin.as_ref(),
        Some(&a)
    );
    let b = SourceOrigin::default();
    let new = coordinator.desire(request("a.x3f", 2, 0.), Some(b.clone()), Instant::now());
    assert_ne!(new, cold.id);
    assert!(
        coordinator.busy(),
        "known replacement must schedule new source pixels"
    );
    coordinator.bind_origin(&cold, &a);
    assert_eq!(
        coordinator.desired(&cold.target).unwrap().origin.as_ref(),
        Some(&b),
        "late source binding cannot overwrite the replacement's identity"
    );
}

#[test]
fn identical_delivery_coalescing_keeps_a_newly_resolved_source_identity() {
    let mut coordinator = RenderCoordinator::default();
    let cold = desire(&mut coordinator, request("a.x3f", 1, 0.));
    coordinator.finish(&cold);
    let a = SourceOrigin::default();
    let same = coordinator.desire(request("a.x3f", 2, 0.), Some(a.clone()), Instant::now());
    assert_eq!(same, cold.id);
    assert!(!coordinator.busy());
    assert_eq!(
        coordinator.desired(&cold.target).unwrap().origin.as_ref(),
        Some(&a)
    );
    let replacement = coordinator.desire(
        request("a.x3f", 3, 0.),
        Some(SourceOrigin::default()),
        Instant::now(),
    );
    assert_ne!(replacement, same);
    assert!(coordinator.busy());
}

#[test]
fn wrong_source_cannot_advance_admission_or_settle_the_replacement() {
    let mut coordinator = RenderCoordinator::default();
    let old_origin = SourceOrigin::default();
    let old_request = request("a.x3f", 1, 0.);
    let target = Target::of(&old_request);
    coordinator.desire(old_request, Some(old_origin.clone()), Instant::now());
    let old = coordinator.desired(&target).unwrap().stamp.clone();
    let new_origin = SourceOrigin::default();
    coordinator.desire(
        request("a.x3f", 2, 0.),
        Some(new_origin.clone()),
        Instant::now(),
    );
    let latest = coordinator.desired(&target).unwrap().stamp.clone();

    assert_eq!(coordinator.finish(&old), Completion::Superseded);
    assert!(!coordinator.admit(&old, &old_origin));
    assert!(coordinator.busy());
    assert!(!coordinator.admit(&latest, &old_origin));
    assert!(
        coordinator.admit(&latest, &new_origin),
        "rejection must not consume the revision"
    );
    assert_eq!(coordinator.finish(&latest), Completion::Current);
    assert!(!coordinator.busy());
    assert!(coordinator.is_settled_and_admitted(latest.id, latest.revision));
}
