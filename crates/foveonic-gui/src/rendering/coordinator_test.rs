//! Exercises observed render interleavings without threads, timers or a GPU.

use super::*;
use crate::editor::Rendition;
use foveonic::render::ProcessingQuality;
use foveonic::render::Settings;

fn request(path: &str, revision: u64, exposure: f32) -> Arc<Request> {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.exposure = exposure;
    Arc::new(Request {
        detail: false,
        framing_preview: false,
        calibration: None,
        rendition: Rendition::Current,
        revision,
        path: path.into(),
        settings,
        preview_scale: foveonic::render::Scale::new(8).unwrap(),
        processing_quality: ProcessingQuality::Speed,
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
        processing_quality: foveonic::render::ProcessingQuality::Quality,
        active: Some(&reply.path),
        rendition: reply.rendition,
        output: "final",
        comparison: false,
        framing_preview: false,
        exporting: false,
        calibration: None,
        settings: &reply.settings,
        overview_visible: true,
        overview_settings: Some(&reply.settings),
        overview_displayed_revision: None,
        overview_detail_revision: None,
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
        coordinator.handle_result(&old, &result_view(&old)).0,
        ResultUpdate {
            current: false,
            update_photo: false,
            destination: None
        }
    );
    assert_eq!(coordinator.plan().tasks.len(), 2);
    assert_eq!(
        coordinator
            .handle_result(&sibling, &result_view(&sibling))
            .0,
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
            .0
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
    view.overview_visible = false;
    assert_eq!(
        coordinator.handle_result(&reply, &view).0,
        ResultUpdate {
            current: true,
            update_photo: true,
            destination: None
        }
    );
    assert!(!coordinator.busy());
    coordinator.clear();
    assert_eq!(
        coordinator.handle_result(&reply, &view).0,
        ResultUpdate {
            current: false,
            update_photo: false,
            destination: None
        }
    );
}

#[test]
fn final_view_recheck_preserves_photo_update_after_target_leaves_view() {
    let mut coordinator = RenderCoordinator::default();
    let request = request("a.x3f", 1, 0.25);
    let mut reply = render_result(desire(&mut coordinator, request.clone()), &request);
    reply.handling = Some(coordinator.handle_result(&reply, &result_view(&reply)).0);
    let mut view = result_view(&reply);
    view.active = Some(Path::new("other.x3f"));
    view.overview_visible = false;
    let update = delivery::recheck(&reply, &view, Some(&request.settings));
    assert!(update.current && update.update_photo);
    assert_eq!(update.destination, None);

    let mut newer = request.settings.clone();
    newer.exposure = 0.5;
    let update = delivery::recheck(&reply, &view, Some(&newer));
    assert!(!update.current && !update.update_photo);
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
        coordinator.handle_result(&reply, &result_view(&reply)).0,
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
fn every_three_photo_completion_order_settles_only_the_latest_requests() {
    fn permutations(order: &mut [usize], index: usize, check: &mut impl FnMut(&[usize])) {
        if index == order.len() {
            check(order);
            return;
        }
        for next in index..order.len() {
            order.swap(index, next);
            permutations(order, index + 1, check);
            order.swap(index, next);
        }
    }

    let mut checked = 0;
    permutations(&mut [0, 1, 2, 3, 4, 5], 0, &mut |order| {
        let mut coordinator = RenderCoordinator::default();
        let mut stamps = Vec::new();
        for path in ["a.x3f", "b.x3f", "c.x3f"] {
            stamps.push(desire(&mut coordinator, request(path, 1, 0.)));
            stamps.push(desire(&mut coordinator, request(path, 2, 0.5)));
        }
        let mut latest_seen = [false; 3];
        for &index in order {
            let photo = index / 2;
            let latest = index % 2 == 1;
            assert_eq!(
                coordinator.finish(&stamps[index]),
                if latest {
                    Completion::Current
                } else {
                    Completion::Superseded
                },
                "completion order {order:?}"
            );
            assert_eq!(
                coordinator.admit(&stamps[index], &SourceOrigin::default()),
                latest || !latest_seen[photo],
                "admission order {order:?}"
            );
            latest_seen[photo] |= latest;
            assert_eq!(coordinator.busy(), latest_seen.iter().any(|seen| !seen));
            for (photo, seen) in latest_seen.iter().enumerate() {
                let latest = &stamps[photo * 2 + 1];
                assert_eq!(
                    coordinator.is_settled_and_admitted(latest.id, latest.revision),
                    *seen
                );
            }
        }
        checked += 1;
    });
    assert_eq!(checked, 720);
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
    native.preview_scale = foveonic::render::Scale::NATIVE;
    let new = desire(&mut coordinator, Arc::new(native));
    assert_ne!(old.id, new.id);
    assert_eq!(coordinator.finish(&old), Completion::Superseded);
    assert!(coordinator.busy());
}

#[test]
fn loading_one_source_leaves_ready_siblings_and_replacement_jobs_serviceable() {
    let mut coordinator = RenderCoordinator::default();
    let mut cursor = Cursor::default();
    let a = desire(&mut coordinator, request("a.x3f", 1, 0.));
    let b = desire(&mut coordinator, request("b.x3f", 2, 0.));
    assert_eq!(
        cursor
            .next_ready(&coordinator.plan(), |_| false)
            .unwrap()
            .stamp,
        a
    );
    cursor.defer(&a);
    // A job first checks presentation/session reuse. Only a job that actually
    // needed a source waits for that source, rather than blocking its whole path.
    assert_eq!(
        cursor
            .next_ready(&coordinator.plan(), |_| false)
            .unwrap()
            .stamp,
        b
    );
    assert!(cursor.next_ready(&coordinator.plan(), |_| false).is_none());
    let replacement = desire(&mut coordinator, request("a.x3f", 3, 0.25));
    cursor.retain_waiting(&coordinator.plan(), None);
    assert_eq!(
        cursor
            .next_ready(&coordinator.plan(), |_| false)
            .unwrap()
            .stamp,
        replacement
    );
    cursor.defer(&replacement);
    assert!(cursor.next_ready(&coordinator.plan(), |_| false).is_none());
    assert_eq!(
        cursor
            .next_ready(&coordinator.plan(), |_| true)
            .unwrap()
            .stamp,
        replacement
    );
}

#[test]
fn a_deferred_export_waits_for_its_source_independently_of_preview_plans() {
    let mut coordinator = RenderCoordinator::default();
    let mut cursor = Cursor::default();
    let export = coordinator.frozen_export(request("export.x3f", 1, 0.), None, Instant::now());
    cursor.defer(&export.stamp);
    cursor.retain_waiting(&coordinator.plan(), Some(&export.stamp));
    assert!(!cursor.ready(&export.stamp, false));
    assert!(cursor.ready(&export.stamp, true));
    cursor.retain_waiting(&coordinator.plan(), None);
    assert!(
        cursor.ready(&export.stamp, false),
        "removed jobs must not leave source waits behind"
    );
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

fn refinement(rendition: Rendition, revision: u64, continue_whole: bool) -> Refinement {
    let mut first = (*request("a.x3f", revision, 0.)).clone();
    first.rendition = rendition;
    first.processing_quality = ProcessingQuality::Quality;
    first.preview_scale = foveonic::render::Scale::NATIVE;
    first.detail = true;
    first.settings.preview_region = Some([32, 32, 160, 160]);
    let whole = continue_whole.then(|| {
        let mut whole = first.clone();
        whole.revision += 1;
        whole.detail = false;
        whole.settings.preview_region = None;
        whole
    });
    Refinement { first, whole }
}

fn regional_completion(coordinator: &mut RenderCoordinator, stamp: &Stamp, origin: &SourceOrigin) {
    let current = coordinator.finish(stamp) == Completion::Current;
    coordinator.bind_origin(stamp, origin);
    let accepted = current && coordinator.admit(stamp, origin);
    if accepted {
        coordinator
            .desired
            .iter_mut()
            .find(|entry| entry.task.stamp == *stamp)
            .unwrap()
            .regional = true;
    }
    let _ = coordinator.advance_refinement(
        stamp,
        ResultUpdate {
            current,
            update_photo: current,
            destination: accepted.then_some(Destination::Active),
        },
        Some((origin, true)),
    );
}

#[test]
fn speculative_whole_waits_for_all_foreground_and_can_resume_after_yield() {
    let mut coordinator = RenderCoordinator::default();
    let origin = SourceOrigin::default();
    let mut cursor = Cursor::default();
    coordinator.refine(
        refinement(Rendition::Current, 1, true),
        Some(origin.clone()),
        Instant::now(),
    );
    let regional = cursor.next(&coordinator.plan()).unwrap();
    regional_completion(&mut coordinator, &regional.stamp, &origin);
    let whole = coordinator.plan().tasks[0].clone();
    assert!(whole.speculative);
    let b = desire(&mut coordinator, request("b.x3f", 3, 0.));
    let c = desire(&mut coordinator, request("c.x3f", 4, 0.));
    assert_eq!(cursor.next(&coordinator.plan()).unwrap().stamp, b);
    cursor.defer(&b);
    assert_eq!(cursor.next(&coordinator.plan()).unwrap().stamp, c);
    assert_eq!(coordinator.finish(&c), Completion::Current);
    assert!(
        cursor.next_ready(&coordinator.plan(), |_| false).is_none(),
        "source-waiting foreground blocks speculative work"
    );
    assert_eq!(
        cursor
            .next_ready(&coordinator.plan(), |_| true)
            .unwrap()
            .stamp,
        b
    );
    assert_eq!(coordinator.finish(&b), Completion::Current);
    coordinator.pick(
        ColorPickRequest {
            tool: crate::rendering::worker::ColorPickTool::Basic,
            token: 1,
            path: "pick.x3f".into(),
            settings: Settings::new("Sunlight", "Standard"),
            uv: [0.5, 0.5],
        },
        5,
        None,
    );
    let pick = cursor.pick_ready(&coordinator.plan(), true).unwrap();
    cursor.retry_pick();
    assert!(cursor.pick_ready(&coordinator.plan(), false).is_none());
    assert!(
        cursor.next(&coordinator.plan()).is_none(),
        "source-waiting pick blocks speculative work"
    );
    assert_eq!(
        cursor.pick_ready(&coordinator.plan(), true).unwrap().id,
        pick.id
    );
    assert!(coordinator.finish_pick(pick.id));
    assert_eq!(cursor.next(&coordinator.plan()).unwrap().stamp, whole.stamp);
    let new = desire(&mut coordinator, request("d.x3f", 5, 0.));
    assert!(cursor.foreground_pending(&coordinator.plan(), &whole.stamp.target));
    cursor.yield_task(&whole.stamp);
    assert_eq!(cursor.next(&coordinator.plan()).unwrap().stamp, new);
    assert_eq!(coordinator.finish(&new), Completion::Current);
    assert_eq!(cursor.next(&coordinator.plan()).unwrap().stamp, whole.stamp);
    assert_eq!(coordinator.finish(&whole.stamp), Completion::Current);
    assert!(!coordinator.busy());
}

#[test]
fn explicit_whole_promotes_matching_continuation_to_foreground() {
    let mut coordinator = RenderCoordinator::default();
    let origin = SourceOrigin::default();
    coordinator.refine(
        refinement(Rendition::Current, 1, true),
        Some(origin.clone()),
        Instant::now(),
    );
    let regional = coordinator.plan().tasks[0].stamp.clone();
    regional_completion(&mut coordinator, &regional, &origin);
    let whole = coordinator.plan().tasks[0].clone();
    assert!(whole.speculative);
    let cursor = Cursor::default();
    desire(&mut coordinator, request("b.x3f", 3, 0.));
    assert!(cursor.should_yield(&coordinator.plan(), &whole, false));
    coordinator.desire(whole.request.clone(), Some(origin), Instant::now());
    assert!(
        !coordinator
            .plan()
            .task(&whole.stamp.target)
            .unwrap()
            .speculative
    );
    assert!(
        !cursor.should_yield(&coordinator.plan(), &whole, false),
        "promotion applies to a running continuation"
    );
}

#[test]
fn refinement_advances_after_regional_admission_without_ui_ack_or_sibling_settlement() {
    for rendition in [Rendition::Current, Rendition::Camera, Rendition::Defaults] {
        let mut coordinator = RenderCoordinator::default();
        let origin = SourceOrigin::default();
        let _ = coordinator.refine(
            refinement(rendition, 1, true),
            Some(origin.clone()),
            Instant::now(),
        );
        let first = coordinator.plan().tasks[0].stamp.clone();
        let sibling = desire(&mut coordinator, request("b.x3f", 3, 0.));
        assert_eq!(coordinator.plan().tasks.len(), 2);
        regional_completion(&mut coordinator, &first, &origin);
        let tasks = coordinator.plan();
        let whole = tasks
            .tasks
            .iter()
            .find(|task| task.request.path == Path::new("a.x3f"))
            .unwrap();
        assert_eq!(whole.stamp.revision, 2);
        assert_eq!(whole.request.settings.preview_region, None);
        assert_eq!(whole.origin.as_ref(), Some(&origin));
        assert!(tasks.tasks.iter().any(|task| task.stamp == sibling));
        assert!(coordinator.busy());
        // No delivered() acknowledgement was needed to make whole-image work runnable.
    }
}

#[test]
fn moving_loupe_replaces_detail_and_preserves_pending_whole_continuation() {
    let mut coordinator = RenderCoordinator::default();
    let origin = SourceOrigin::default();
    coordinator.refine(
        refinement(Rendition::Current, 1, true),
        Some(origin.clone()),
        Instant::now(),
    );
    let initial = coordinator.plan().tasks[0].stamp.clone();
    regional_completion(&mut coordinator, &initial, &origin);
    let whole = coordinator.plan().tasks[0].stamp.clone();
    let mut lens = refinement(Rendition::Current, 3, false).first;
    lens.detail = true;
    lens.settings.preview_region = Some([500, 500, 700, 700]);
    let first_lens = desire(&mut coordinator, Arc::new(lens.clone()));
    assert!(coordinator
        .plan()
        .tasks
        .iter()
        .any(|task| task.stamp == whole));
    lens.revision = 4;
    lens.detail = true;
    lens.settings.preview_region = Some([900, 900, 1100, 1100]);
    let moved = desire(&mut coordinator, Arc::new(lens));
    assert_ne!(first_lens.id, moved.id);
    assert_eq!(coordinator.plan().tasks.len(), 2);
    coordinator.retire_detail(Path::new("a.x3f"));
    assert_eq!(coordinator.plan().tasks.len(), 1);
    assert_eq!(coordinator.plan().tasks[0].stamp, whole);
    assert!(coordinator
        .refine(
            refinement(Rendition::Current, 10, true),
            Some(origin),
            Instant::now()
        )
        .is_none());
    assert_eq!(coordinator.plan().tasks[0].stamp, whole);
}

#[test]
fn loupe_replacing_pending_smart_region_cannot_inherit_its_whole_continuation() {
    let mut coordinator = RenderCoordinator::default();
    coordinator.refine(
        refinement(Rendition::Current, 1, true),
        None,
        Instant::now(),
    );
    let mut lens = refinement(Rendition::Current, 3, false).first;
    lens.detail = true;
    lens.settings.preview_region = Some([500, 500, 700, 700]);
    let stamp = desire(&mut coordinator, Arc::new(lens));
    regional_completion(&mut coordinator, &stamp, &SourceOrigin::default());
    assert!(
        !coordinator.busy(),
        "loupe must not inherit Smart's Whole work"
    );
}

#[test]
fn region_only_and_native_fallback_settle_without_whole_work() {
    for (continuation, regional) in [(false, true), (true, false)] {
        let mut coordinator = RenderCoordinator::default();
        let _ = coordinator.refine(
            refinement(Rendition::Current, 1, continuation),
            None,
            Instant::now(),
        );
        let stamp = coordinator.plan().tasks[0].stamp.clone();
        let origin = SourceOrigin::default();
        coordinator.finish(&stamp);
        assert!(coordinator.admit(&stamp, &origin));
        let _ = coordinator.advance_refinement(
            &stamp,
            ResultUpdate {
                current: true,
                update_photo: true,
                destination: Some(Destination::Active),
            },
            Some((&origin, regional)),
        );
        assert!(!coordinator.busy());
        assert!(coordinator.plan().tasks.is_empty());
        let mut pan = refinement(Rendition::Current, 10, continuation);
        pan.first.detail = true;
        pan.first.settings.preview_region = Some([400, 400, 600, 600]);
        let _ = coordinator.refine(pan, None, Instant::now());
        assert!(
            !coordinator.busy(),
            "panning must not restart a completed sequence"
        );
    }
}

#[test]
fn refinement_replacement_clear_fit_and_before_retirement_cannot_resurrect_whole_work() {
    for action in ["edit", "clear", "fit", "before"] {
        let mut coordinator = RenderCoordinator::default();
        let mut initial = refinement(Rendition::Camera, 1, true);
        if action == "fit" {
            initial = refinement(Rendition::Current, 1, true);
        }
        let _ = coordinator.refine(initial, None, Instant::now());
        let old = coordinator.plan().tasks[0].stamp.clone();
        match action {
            "edit" => {
                let mut newer = refinement(Rendition::Camera, 3, true);
                newer.first.settings.exposure = 1.;
                newer.whole.as_mut().unwrap().settings.exposure = 1.;
                let _ = coordinator.refine(newer, None, Instant::now());
            }
            "clear" => coordinator.clear(),
            "fit" => {
                desire(&mut coordinator, request("a.x3f", 5, 0.));
            }
            "before" => coordinator.retire_before(Path::new("a.x3f")),
            _ => unreachable!(),
        }
        regional_completion(&mut coordinator, &old, &SourceOrigin::default());
        assert!(!coordinator
            .plan()
            .tasks
            .iter()
            .any(|task| task.stamp.revision == 2));
    }
}

#[test]
fn failed_or_source_rejected_refinement_does_not_continue() {
    for failure in ["error", "source"] {
        let mut coordinator = RenderCoordinator::default();
        let expected = SourceOrigin::default();
        let intent = refinement(Rendition::Current, 1, true);
        let _ = coordinator.refine(intent.clone(), Some(expected), Instant::now());
        let stamp = coordinator.plan().tasks[0].stamp.clone();
        if failure == "source" {
            regional_completion(&mut coordinator, &stamp, &SourceOrigin::default());
        } else {
            let mut reply = render_result(stamp, &intent.first);
            reply.result = Err("decode failed".into());
            coordinator.handle_result(&reply, &result_view(&reply));
        }
        assert!(!coordinator.busy());
    }
}

#[test]
fn before_refinement_and_authored_current_requests_keep_independent_lifetimes() {
    let mut coordinator = RenderCoordinator::default();
    let current = desire(&mut coordinator, request("a.x3f", 1, 0.));
    let _ = coordinator.refine(refinement(Rendition::Camera, 2, true), None, Instant::now());
    assert!(coordinator
        .plan()
        .tasks
        .iter()
        .any(|task| task.stamp == current));
    let camera = coordinator
        .plan()
        .tasks
        .iter()
        .find(|task| task.request.rendition == Rendition::Camera)
        .unwrap()
        .stamp
        .clone();
    desire(&mut coordinator, request("a.x3f", 4, 0.5));
    assert!(coordinator
        .plan()
        .tasks
        .iter()
        .any(|task| task.stamp == camera));
    regional_completion(&mut coordinator, &camera, &SourceOrigin::default());
    assert!(coordinator
        .plan()
        .tasks
        .iter()
        .any(|task| task.request.rendition == Rendition::Camera
            && task.request.settings.preview_region.is_none()));
}

#[test]
fn known_source_republication_does_not_start_phantom_progress() {
    let mut coordinator = RenderCoordinator::default();
    assert!(coordinator
        .refine(
            refinement(Rendition::Current, 1, true),
            None,
            Instant::now()
        )
        .is_some());
    let a = SourceOrigin::default();
    assert!(coordinator
        .refine(
            refinement(Rendition::Current, 3, true),
            Some(a.clone()),
            Instant::now()
        )
        .is_none());
    assert_eq!(coordinator.plan().tasks[0].origin.as_ref(), Some(&a));
    assert_eq!(coordinator.plan().tasks[0].stamp.revision, 1);
    let b = SourceOrigin::default();
    let replacement = coordinator
        .refine(
            refinement(Rendition::Current, 5, true),
            Some(b.clone()),
            Instant::now(),
        )
        .unwrap();
    assert_eq!(replacement.stamp.revision, 5);
    assert_eq!(replacement.origin.as_ref(), Some(&b));
}

#[test]
fn completed_region_can_acquire_continuation_without_rerendering_the_region() {
    let mut coordinator = RenderCoordinator::default();
    let _ = coordinator.refine(
        refinement(Rendition::Current, 1, false),
        None,
        Instant::now(),
    );
    let first = coordinator.plan().tasks[0].stamp.clone();
    regional_completion(&mut coordinator, &first, &SourceOrigin::default());
    assert!(!coordinator.busy());
    let whole = coordinator
        .refine(
            refinement(Rendition::Current, 3, true),
            None,
            Instant::now(),
        )
        .unwrap();
    assert_eq!(whole.request.settings.preview_region, None);
    assert_eq!(whole.stamp.revision, 4);
    assert_eq!(coordinator.plan().tasks.len(), 1);
}

#[test]
fn already_completed_whole_does_not_restart_progress_when_refinement_reuses_it() {
    let mut coordinator = RenderCoordinator::default();
    let intent = refinement(Rendition::Current, 1, true);
    let whole = desire(&mut coordinator, Arc::new(intent.whole.clone().unwrap()));
    coordinator.finish(&whole);
    let _ = coordinator.refine(intent, None, Instant::now());
    let first = coordinator.plan().tasks[0].stamp.clone();
    let origin = SourceOrigin::default();
    coordinator.finish(&first);
    coordinator.bind_origin(&first, &origin);
    assert!(coordinator.admit(&first, &origin));
    let started = coordinator.advance_refinement(
        &first,
        ResultUpdate {
            current: true,
            update_photo: true,
            destination: Some(Destination::Active),
        },
        Some((&origin, true)),
    );
    assert!(started.is_none());
    assert!(!coordinator.busy());
}

#[test]
fn demanded_current_details_and_continuations_survive_selection_and_sibling_refresh() {
    let mut coordinator = RenderCoordinator::default();
    let origin = SourceOrigin::default();
    coordinator.activate(Path::new("a.x3f"));
    let _ = coordinator.refine(
        refinement(Rendition::Current, 1, true),
        None,
        Instant::now(),
    );
    let first_a = coordinator.plan().tasks[0].stamp.clone();
    let mut b = refinement(Rendition::Current, 3, true);
    b.first.path = "b.x3f".into();
    b.whole.as_mut().unwrap().path = "b.x3f".into();
    let _ = coordinator.refine(b, None, Instant::now());
    let first_b = coordinator
        .plan()
        .tasks
        .iter()
        .find(|task| task.request.path == Path::new("b.x3f"))
        .unwrap()
        .stamp
        .clone();
    let _ = coordinator.refine(refinement(Rendition::Camera, 5, true), None, Instant::now());
    coordinator.activate(Path::new("b.x3f"));
    assert!(coordinator
        .plan()
        .tasks
        .iter()
        .all(|task| task.request.rendition == Rendition::Current));
    regional_completion(&mut coordinator, &first_a, &origin);
    let paths = ["a.x3f".into(), "b.x3f".into(), "c.x3f".into()];
    coordinator.retain_overviews(&paths, &[]);
    desire(&mut coordinator, request("c.x3f", 7, 0.25));
    assert!(coordinator
        .plan()
        .tasks
        .iter()
        .any(|task| task.request.path == Path::new("a.x3f") && task.stamp.revision == 2));
    assert!(coordinator
        .plan()
        .tasks
        .iter()
        .any(|task| task.stamp == first_b));
    regional_completion(&mut coordinator, &first_b, &SourceOrigin::default());
    assert!(coordinator
        .plan()
        .tasks
        .iter()
        .any(|task| task.request.path == Path::new("b.x3f") && task.stamp.revision == 4));
    coordinator.retain_overviews(&paths[1..], &[paths[0].clone()]);
    assert!(
        !coordinator
            .plan()
            .tasks
            .iter()
            .any(|task| task.request.path == Path::new("a.x3f")),
        "retained history cannot keep an undemanded continuation running"
    );
    coordinator.retain_overviews(&[], &[]);
    assert!(coordinator.plan().tasks.is_empty());
    let replacement = coordinator
        .refine(
            refinement(Rendition::Current, 9, true),
            None,
            Instant::now(),
        )
        .unwrap();
    assert_eq!(replacement.stamp.revision, 9);
}
