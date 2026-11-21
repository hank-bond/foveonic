//! Exercises observed render interleavings without threads, timers or a GPU.

use super::*;
use crate::editor::Rendition;
use crate::rendering::request::RequestId;
use foveonic::render::ProcessingQuality;
use foveonic::render::Settings;

fn request(path: &str, revision: u64, exposure: f32) -> Arc<RenderRequest> {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.exposure = exposure;
    Arc::new(RenderRequest {
        detail: false,
        framing_preview: false,
        calibration: None,
        rendition: Rendition::Current,
        id: RequestId(revision),
        path: path.into(),
        settings,
        preview_scale: foveonic::render::Scale::new(8).unwrap(),
        processing_quality: ProcessingQuality::Speed,
        output: "final".into(),
        comparison: false,
    })
}
fn desire(coordinator: &mut RenderCoordinator, request: Arc<RenderRequest>) -> Stamp {
    let target = Target::of(&request);
    coordinator.desire(request, None, Instant::now(), false);
    coordinator.desired(&target).unwrap().stamp.clone()
}

fn render_result(stamp: Stamp, request: &RenderRequest) -> Reply {
    Reply {
        handling: None,
        stamp,
        request: std::sync::Arc::new(request.clone()),
        metadata: None,
        settings: request.settings.clone(),
        result: crate::rendering::worker::RenderOutcome::Cancelled,
        numeric_settings: None,
        reset_settings: None,
    }
}

#[test]
fn result_handler_settles_current_failures_without_settling_newer_or_sibling_requests() {
    let mut coordinator = RenderCoordinator::default();
    let a = request("a.x3f", 1, 0.);
    let mut old = render_result(desire(&mut coordinator, a.clone()), &a);
    old.result = crate::rendering::worker::RenderOutcome::Failed("cannot decode source".into());
    let b = request("b.x3f", 1, 0.);
    let mut sibling = render_result(desire(&mut coordinator, b.clone()), &b);
    sibling.result = crate::rendering::worker::RenderOutcome::Failed("cannot load source".into());
    let a = request("a.x3f", 2, 0.5);
    let latest = render_result(desire(&mut coordinator, a.clone()), &a);

    assert_eq!(
        coordinator.handle_result(&old),
        ResultUpdate {
            current: false,
            accepted: false
        }
    );
    assert_eq!(coordinator.plan().tasks.len(), 2);
    assert_eq!(
        coordinator.handle_result(&sibling),
        ResultUpdate {
            current: true,
            accepted: false
        }
    );
    assert!(coordinator.busy());
    assert_eq!(coordinator.plan().tasks[0].stamp, latest.stamp);
    assert!(!coordinator.is_settled_and_admitted(sibling.stamp.id, sibling.request.id));
    assert!(coordinator.handle_result(&latest).current);
    assert!(!coordinator.busy());
}

#[test]
fn removing_b_while_a_remains_active_rejects_b_from_the_same_cohort() {
    let mut coordinator = RenderCoordinator::default();
    let a = desire(&mut coordinator, request("a.x3f", 10, 0.));
    let b = desire(&mut coordinator, request("b.x3f", 10, 0.));
    assert_eq!(coordinator.finish(&a), Completion::Current);
    assert!(coordinator.admit(&a, &SourceOrigin::default()));
    coordinator.reconcile(
        &Photos {
            loading: vec!["a.x3f".into()],
            history: vec![],
            ..Default::default()
        },
        &[],
    );
    assert_eq!(coordinator.finish(&b), Completion::Removed);
    assert!(!coordinator.admit(&b, &SourceOrigin::default()));
    assert!(!coordinator.busy());
    assert_eq!(coordinator.desired(&a.target).unwrap().stamp, a);
}

#[test]
fn retained_photos_keep_only_completed_intents_outside_the_edit_targets() {
    let paths: [PathBuf; 2] = ["a.x3f".into(), "b.x3f".into()];
    let mut coordinator = RenderCoordinator::default();
    let a = desire(&mut coordinator, request("a.x3f", 1, 0.));
    let b = desire(&mut coordinator, request("b.x3f", 2, 0.));
    coordinator.finish(&a);
    coordinator.reconcile(
        &Photos {
            loading: vec![],
            history: paths.to_vec(),
            ..Default::default()
        },
        &[],
    );
    assert_eq!(coordinator.finish(&b), Completion::Removed);
    assert!(coordinator.desired(&a.target).is_some());
    assert!(!coordinator.busy());

    coordinator.reconcile(
        &Photos {
            loading: vec![],
            history: paths[1..].to_vec(),
            ..Default::default()
        },
        &[],
    );
    assert!(coordinator.desired(&a.target).is_none());
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
                    coordinator.is_settled_and_admitted(latest.id, latest.request_id),
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
    let original = request("a.x3f", 1, 0.);
    let old = desire(&mut coordinator, original);
    assert_eq!(next_task(&coordinator.plan()).unwrap().stamp, old);
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
    let task = next_task(&coordinator.plan()).unwrap();
    assert_eq!(task.stamp, latest);
    assert_eq!(task.request.settings.crop.bounds, [0.1, 0.2, 0.9, 0.8]);
    assert_eq!(coordinator.finish(&latest), Completion::Current);
    assert!(!coordinator.busy());
}

#[test]
fn cached_images_require_the_exact_settled_and_admitted_delivery() {
    let mut coordinator = RenderCoordinator::default();
    let original = desire(&mut coordinator, request("a.x3f", 1, 0.));
    assert!(!coordinator.is_settled_and_admitted(original.id, original.request_id));
    coordinator.finish(&original);
    assert!(!coordinator.is_settled_and_admitted(original.id, original.request_id));
    assert!(coordinator.admit(&original, &SourceOrigin::default()));
    assert!(coordinator.is_settled_and_admitted(original.id, original.request_id));

    let unchanged = desire(&mut coordinator, request("a.x3f", 2, 0.));
    assert_eq!(unchanged, original);
    assert!(coordinator.is_settled_and_admitted(original.id, original.request_id));

    let changed = desire(&mut coordinator, request("a.x3f", 3, 0.5));
    assert!(!coordinator.is_settled_and_admitted(original.id, original.request_id));
    coordinator.finish(&changed);
    assert!(coordinator.admit(&changed, &SourceOrigin::default()));
    assert!(coordinator.is_settled_and_admitted(changed.id, changed.request_id));

    let mut composition = (*request("a.x3f", 4, 0.5)).clone();
    composition.settings.rotation.degrees = 3.;
    let composition = desire(&mut coordinator, Arc::new(composition));
    assert_eq!(composition.id, changed.id);
    assert!(!coordinator.is_settled_and_admitted(changed.id, changed.request_id));
    coordinator.finish(&composition);
    assert!(coordinator.admit(&composition, &SourceOrigin::default()));
    assert!(coordinator.is_settled_and_admitted(composition.id, composition.request_id));
    coordinator.clear();
    assert!(!coordinator.is_settled_and_admitted(composition.id, composition.request_id));
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
    coordinator.reconcile(
        &Photos {
            loading: vec![],
            history: vec![],
            ..Default::default()
        },
        &[],
    );
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
    let old = coordinator.desire(first, Some(first_origin), Instant::now(), false);
    let new = coordinator.desire(
        request("a.x3f", 2, 0.),
        Some(SourceOrigin::default()),
        Instant::now(),
        false,
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
    let new = coordinator.desire(
        request("a.x3f", 2, 0.),
        Some(b.clone()),
        Instant::now(),
        false,
    );
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
    let same = coordinator.desire(
        request("a.x3f", 2, 0.),
        Some(a.clone()),
        Instant::now(),
        false,
    );
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
        false,
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
    coordinator.desire(old_request, Some(old_origin.clone()), Instant::now(), false);
    let old = coordinator.desired(&target).unwrap().stamp.clone();
    let new_origin = SourceOrigin::default();
    coordinator.desire(
        request("a.x3f", 2, 0.),
        Some(new_origin.clone()),
        Instant::now(),
        false,
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
    assert!(coordinator.is_settled_and_admitted(latest.id, latest.request_id));
}

#[test]
fn relaxed_sampling_does_not_share_an_inflight_pixel_task() {
    let mut coordinator = RenderCoordinator::default();
    let mut first = request("a.x3f", 1, 0.).as_ref().clone();
    first.preview_scale = foveonic::render::Scale::new(4).unwrap();
    let old = desire(&mut coordinator, Arc::new(first.clone()));
    first.id = RequestId(2);
    first.preview_scale = foveonic::render::Scale::new(8).unwrap();
    let new = desire(&mut coordinator, Arc::new(first));
    assert_ne!(old.id, new.id);
    assert!(coordinator.desired(&new.target).unwrap().seed.is_none());
    assert_eq!(coordinator.finish(&old), Completion::Superseded);
}

fn next_task(plan: &WorkPlan) -> Option<Arc<Task>> {
    let tier = plan.tasks.iter().map(|task| plan.priority(task)).min()?;
    plan.tasks
        .iter()
        .find(|task| plan.priority(task) == tier)
        .cloned()
}

#[test]
fn each_task_maps_to_one_scheduler_purpose() {
    use foveonic::render::Purpose as Job;
    let plan = WorkPlan::default();
    let task = |path: &str, purpose: Purpose, speculative: bool| Task {
        stamp: Stamp {
            id: TaskId(1),
            target: Target {
                path: path.into(),
                purpose,
                patch: false,
            },
            request_id: RequestId(1),
        },
        request: request(path, 1, 0.),
        origin: None,
        queued_at: Instant::now(),
        seed: None,
        speculative,
    };
    let inspection = |comparison| Purpose::Inspection("final".into(), comparison);
    for (task, purpose) in [
        (task("a.x3f", Purpose::Current, false), Job::Edit),
        (task("a.x3f", inspection(false), false), Job::Edit),
        (task("a.x3f", Purpose::Camera, false), Job::View),
        (task("a.x3f", Purpose::Defaults, false), Job::View),
        (task("a.x3f", inspection(true), false), Job::View),
        (task("a.x3f", Purpose::Current, true), Job::Speculative),
    ] {
        let target = &task.stamp.target;
        assert_eq!(plan.purpose(&task), purpose, "{target:?}");
    }
}

#[test]
fn finished_images_in_use_keep_their_work_and_others_stop() {
    let mut coordinator = RenderCoordinator::default();
    let origin = SourceOrigin::default();
    let mut patch = (*request("a.x3f", 1, 0.)).clone();
    patch.detail = true;
    patch.preview_scale = foveonic::render::Scale::NATIVE;
    patch.settings.preview_region = Some([0, 0, 100, 100]);
    let target = Target::of(&patch);
    let finished = coordinator.desire(Arc::new(patch.clone()), None, Instant::now(), false);
    let stamp = coordinator.desired(&target).unwrap().stamp.clone();
    assert_eq!(stamp.id, finished);
    coordinator.finish(&stamp);
    assert!(coordinator.admit(&stamp, &origin));
    let photos = [PathBuf::from("a.x3f")];
    coordinator.reconcile(
        &Photos {
            planned: photos.to_vec(),
            ..Default::default()
        },
        std::slice::from_ref(&patch),
    );
    assert!(
        coordinator.desired(&target).is_some(),
        "a finished patch in use stays desired"
    );
    coordinator.reconcile(
        &Photos {
            planned: photos.to_vec(),
            ..Default::default()
        },
        &[],
    );
    assert!(coordinator.desired(&target).is_none());
}

#[test]
fn a_replaced_files_first_image_is_admitted_when_the_request_does_not_know_its_origin() {
    let mut coordinator = RenderCoordinator::default();
    let (old, new) = (SourceOrigin::default(), SourceOrigin::default());
    let first = desire(&mut coordinator, request("a.x3f", 1, 0.));
    assert!(coordinator.admit(&first, &old));
    // Delivery binds the source the image came from.
    coordinator.bind_origin(&first, &old);
    // The file is replaced on disk before the next request knows its origin.
    let second = desire(&mut coordinator, request("a.x3f", 2, 0.5));
    assert!(
        coordinator.admit(&second, &new),
        "the newest file's image is the photo's image, so its pixels can seed later work"
    );
}

#[test]
fn a_late_image_from_a_replaced_file_does_not_bind_its_origin_to_newer_work() {
    let mut coordinator = RenderCoordinator::default();
    let (old, new) = (SourceOrigin::default(), SourceOrigin::default());
    let first = desire(&mut coordinator, request("a.x3f", 1, 0.));
    // A quarter turn reuses the first request's pixel work, so both share a task.
    let mut turned = (*request("a.x3f", 2, 0.)).clone();
    let crop = &mut turned.settings.crop;
    turned.settings.rotation.turn(true, crop);
    let second = desire(&mut coordinator, Arc::new(turned));
    assert_eq!(first.id, second.id);
    // The first request's image, from the old file, arrives after the second request.
    coordinator.bind_origin(&first, &old);
    assert!(
        coordinator.admit(&second, &new),
        "the newer request's image, from the replaced file, is admitted"
    );
}

#[test]
fn work_follows_the_plan_order_not_when_it_was_first_desired() {
    let mut coordinator = RenderCoordinator::default();
    let whole = request("b.x3f", 1, 0.);
    desire(&mut coordinator, whole.clone());
    let mut patch = (*request("a.x3f", 2, 0.)).clone();
    patch.detail = true;
    patch.settings.preview_region = Some([0, 0, 64, 64]);
    let patch = Arc::new(patch);
    desire(&mut coordinator, patch.clone());
    coordinator.order_as(&[(*patch).clone(), (*whole).clone()]);
    let order: Vec<_> = coordinator
        .plan()
        .tasks
        .iter()
        .map(|task| task.stamp.target.clone())
        .collect();
    assert_eq!(order, [Target::of(&patch), Target::of(&whole)]);
}

#[test]
fn the_viewed_and_editing_photos_images_and_unfinished_work_are_eviction_protected() {
    let mut coordinator = RenderCoordinator::default();
    let (edited, shown) = (request("a.x3f", 1, 0.), request("b.x3f", 2, 0.));
    // The viewed photo's image is current, so it has no work and no task.
    let viewed = request("c.x3f", 3, 0.);
    let edited_stamp = desire(&mut coordinator, edited.clone());
    let shown_stamp = desire(&mut coordinator, shown.clone());
    let in_use = [(*edited).clone(), (*shown).clone(), (*viewed).clone()];
    let photos = Photos {
        planned: vec![edited.path.clone(), shown.path.clone(), viewed.path.clone()],
        eviction_protected: vec![viewed.path.clone(), edited.path.clone()],
        ..Default::default()
    };
    coordinator.reconcile(&photos, &in_use);
    let protected = |coordinator: &RenderCoordinator| coordinator.plan().eviction_protected.clone();
    assert_eq!(
        protected(&coordinator),
        [Target::of(&edited), Target::of(&viewed), Target::of(&shown)],
        "unfinished work keeps the shown photo too"
    );
    coordinator.finish(&edited_stamp);
    coordinator.finish(&shown_stamp);
    assert_eq!(
        protected(&coordinator),
        [Target::of(&edited), Target::of(&viewed)],
        "a finished photo that is only shown can release its working photo"
    );
}
