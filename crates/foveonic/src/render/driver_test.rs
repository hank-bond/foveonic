//! Checks real reduced-job admission, retained memory, numerical delivery and retirement.

use super::*;
use crate::{
    inspection::Content,
    profile::{Mode, Timing},
    render::{ReducedPhoto, ReducedPreparation, Scale},
    source::Source,
};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::HashMap, path::PathBuf};

pub(crate) fn fixture() -> Arc<Source> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../scripts/profiling/wide_inputs.json"
    ))
    .unwrap();
    let name = "blue-parking-sign.x3f";
    let entry = manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["file"] == name)
        .unwrap();
    let path = PathBuf::from(
        std::env::var_os("FOVEONIC_FIXTURE_DIR")
            .expect("sync the declared example photographs and set FOVEONIC_FIXTURE_DIR"),
    )
    .join(name);
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&path).unwrap())),
        entry["sha256"].as_str().unwrap(),
        "fixture must match the tracked example-release identity"
    );
    Arc::new(Source::open(path).unwrap())
}

pub(crate) fn settings() -> Settings {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.working_scale = Scale::new(8).unwrap();
    settings.exposure_auto = false;
    settings
        .reconstruction
        .get_or_insert_with(Default::default)
        .chroma = 0;
    settings
}

pub(super) fn session(
    engine: &Engine,
    source: &Arc<Source>,
    settings: &Settings,
) -> Box<ReducedPhoto> {
    let ReducedPreparation::Prepared(session) = engine
        .prepare_reduced_photo(source.clone(), settings, || false, &mut Profile::default())
        .unwrap()
    else {
        panic!("prepared reduced inputs")
    };
    session
}

fn working(engine: &Engine, source: &Arc<Source>, settings: &Settings) -> WorkingSet {
    let mut working = WorkingSet::new();
    working
        .insert_reduced(1, session(engine, source, settings), engine, Purpose::Edit)
        .unwrap();
    working
}

fn profile() -> Profile {
    let mut profile = Profile::new(Mode::Wall);
    profile.host.push(Timing {
        stage: "caller observation",
        milliseconds: 0.,
    });
    profile
}

pub(super) fn image(result: PhotoRender) -> crate::image::RenderedImage {
    let PhotoRender::Rendered(mut artifacts) = result else {
        panic!("completed reduced render")
    };
    assert_eq!(artifacts.len(), 1);
    let Content::Image(image) = artifacts.remove(0).content else {
        panic!("photographic image")
    };
    *image
}

fn native_cache_stages(profile: &Profile) -> Vec<&'static str> {
    profile
        .host
        .iter()
        .filter_map(|timing| {
            matches!(
                timing.stage,
                "prepare session cache" | "finish session cache"
            )
            .then_some(timing.stage)
        })
        .collect()
}

fn progress(event: RenderProgress) -> (usize, usize) {
    match event {
        RenderProgress::Prepared => (0, 0),
        RenderProgress::Stage { completed, total } => (completed, total),
    }
}

fn buffers(working: &WorkingSet, photo: u64) -> Vec<((u64, u64), u64)> {
    working
        .reduced(photo)
        .unwrap()
        .buffers()
        .map(|buffer| (buffer.identity(), buffer.size()))
        .collect()
}

fn begin(
    engine: &Engine,
    working: &mut WorkingSet,
    settings: &Settings,
    profile: &mut Profile,
) -> RenderExecution {
    match engine
        .begin_render_execution(
            working,
            PhotoKey {
                photo: 1,
                resolution: Resolution::Reduced,
            },
            settings,
            &["final"],
            &|| false,
            profile,
            None,
        )
        .unwrap()
    {
        RenderExecutionStart::Ready(execution) => execution,
        _ => panic!("retained reduced session"),
    }
}

/// One step alone in its own tick.
fn step(
    engine: &Engine,
    working: &mut WorkingSet,
    execution: &mut RenderExecution,
    cancel: &dyn Fn() -> bool,
) -> crate::source::Result<bool> {
    let tick = engine.tick();
    let more = execution.step(engine, working, cancel);
    let waited = tick.complete(true)?;
    execution.complete_tick(engine, waited, &|_| {});
    more
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn same_photo_is_rejected_before_validation_profile_or_session_mutation() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let settings = settings();
    let mut working = working(&engine, &source, &settings);
    let mut first_profile = profile();
    let execution = begin(&engine, &mut working, &settings, &mut first_profile);
    let retained_buffers = buffers(&working, 1);
    let activity = engine.gpu_activity();
    let mut invalid = settings.clone();
    invalid.exposure = f32::NAN;
    let mut peer_profile = profile();
    let error = engine
        .begin_render_execution(
            &mut working,
            PhotoKey {
                photo: 1,
                resolution: Resolution::Reduced,
            },
            &invalid,
            &["final"],
            &|| panic!("denied peer must not enter execution"),
            &mut peer_profile,
            None,
        )
        .err()
        .expect("same photo already executes");
    assert!(error.is::<crate::render::PhotoInUse>());
    assert_eq!(buffers(&working, 1), retained_buffers);
    assert!(working.reduced(1).unwrap().last_settings.is_none());
    assert_eq!(peer_profile.mode, Mode::Wall);
    assert_eq!(peer_profile.host.len(), 1);
    assert_eq!(engine.gpu_activity().submissions, activity.submissions);
    assert!(working.remove_photo(1).is_err());
    assert!(matches!(
        execution
            .finish(
                &engine,
                &mut working,
                Err(Box::new(Superseded)),
                &mut first_profile
            )
            .unwrap(),
        PhotoRender::Superseded
    ));
    assert!(working.remove_photo(1).unwrap());
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn job_stopped_between_steps_retires_the_step_it_left_in_flight() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let settings = settings();
    let mut working = working(&engine, &source, &settings);
    let mut profile = profile();
    let mut execution = begin(&engine, &mut working, &settings, &mut profile);
    while !engine.gpu.has_work_in_flight() {
        let tick = engine.tick();
        assert!(execution.step(&engine, &mut working, &|| false).unwrap());
        let waited = tick.complete(false).unwrap();
        execution.complete_tick(&engine, waited, &|_| {});
    }
    assert!(matches!(
        execution
            .finish(
                &engine,
                &mut working,
                Err(Box::new(Superseded)),
                &mut profile
            )
            .unwrap(),
        PhotoRender::Superseded
    ));
    assert!(!engine.gpu.has_work_in_flight());
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn session_execution_matches_ordinary_pixels_geometry_and_completed_progress_under_soft_overage() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let base = settings();
    let mut ordinary = session(&engine, &source, &base);
    let mut working = working(&engine, &source, &base);
    let mut exposure = base.clone();
    exposure.exposure += 0.25;
    let mut noise = exposure.clone();
    noise.reconstruction.as_mut().unwrap().chroma = 1;
    let retained = MemoryReservation::default();
    for requested in [base, exposure, noise] {
        let expected_progress = RefCell::new(Vec::new());
        let expected = image(
            engine
                .render_reduced_photo_with_progress(
                    &mut ordinary,
                    &requested,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                    |event| expected_progress.borrow_mut().push(progress(event)),
                )
                .unwrap(),
        );
        let observed_progress = RefCell::new(Vec::new());
        let mut profile = profile();
        let observed = image(
            working
                .render_reduced_with_progress(
                    &engine,
                    1,
                    &requested,
                    Purpose::Edit,
                    || false,
                    &mut profile,
                    |event| observed_progress.borrow_mut().push(progress(event)),
                )
                .unwrap()
                .unwrap(),
        );
        assert_eq!(
            engine.readback_image(&observed).unwrap(),
            engine.readback_image(&expected).unwrap()
        );
        assert_eq!(observed.description, expected.description);
        assert_eq!(observed.view, expected.view);
        assert_eq!(observed.settings, expected.settings);
        assert_eq!(*observed_progress.borrow(), *expected_progress.borrow());
        assert_eq!(profile.mode, Mode::Wall);
        assert_eq!(profile.host[0].stage, "caller observation");
        assert!(native_cache_stages(&profile).is_empty());
        assert!(!profile.graphs.is_empty());
        assert!(engine.memory_resources(&working, &retained).total_bytes > 1);
    }
    working.set_eviction_protected([PhotoKey {
        photo: 1,
        resolution: Resolution::Reduced,
    }]);
    let trim = engine.trim_memory(&mut working, 1, &retained);
    assert!(trim.over_target_bytes > 0);
    assert!(working.reduced(1).is_some());
    working.clear().unwrap();
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn live_graph_and_scratch_share_one_inventory_with_sessions_and_frontend() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let settings = settings();
    let mut working = working(&engine, &source, &settings);
    let mut profile = profile();
    let mut execution = begin(&engine, &mut working, &settings, &mut profile);
    let mut cursor_only_seen = false;
    loop {
        let more = step(&engine, &mut working, &mut execution, &|| false).unwrap();
        let graph = &execution.recipe.traversal;
        let retained: HashMap<_, _> = working
            .memory_buffers()
            .map(|buffer| (buffer.identity().0, buffer.size()))
            .collect();
        let mut union = retained.clone();
        let mut frontend = MemoryReservation::default();
        frontend.exclusive_bytes = 257;
        for buffer in graph.buffers() {
            cursor_only_seen |= !retained.contains_key(&buffer.identity().0);
            union.insert(buffer.identity().0, buffer.size());
            frontend.add_buffer(buffer);
            frontend.add_buffer(buffer);
        }
        for buffer in working.memory_buffers() {
            frontend.add_buffer(buffer);
        }
        let before = engine.execution_memory_resources(&working, &frontend, &[&execution]);
        assert_eq!(before.gpu_bytes, union.values().sum::<u64>());
        assert_eq!(before.reserved_bytes, 257);
        let session_cpu = engine
            .memory_resources(&working, &Default::default())
            .cpu_bytes;
        assert!(
            before.cpu_bytes > session_cpu,
            "count job-owned containers and live buffer metadata"
        );
        assert_eq!(
            before.cpu_bytes,
            engine
                .execution_memory_resources(&working, &Default::default(), &[&execution])
                .cpu_bytes
        );
        assert_eq!(
            before.total_bytes,
            before.cpu_bytes + before.gpu_bytes + 257
        );
        if !more {
            break;
        }
    }
    assert!(
        cursor_only_seen,
        "exercise buffers owned by the cursor rather than checkpoints"
    );
    assert!(
        execution
            .recipe
            .profile
            .nodes
            .iter()
            .any(|node| node.memory_after_retirement.scratch_bytes > 0),
        "exercise spare scratch in the live inventory"
    );
    image(
        execution
            .finish(&engine, &mut working, Ok(()), &mut profile)
            .unwrap(),
    );
    working.clear().unwrap();
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn cancellation_failure_and_unfinished_retirement_restore_profile_and_allow_retry() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let settings = settings();
    for reason in ["cancel", "failure", "unfinished"] {
        let mut working = working(&engine, &source, &settings);
        let mut profile = profile();
        let mut execution = begin(&engine, &mut working, &settings, &mut profile);
        assert_eq!(
            profile.mode,
            Mode::Off,
            "execution temporarily owns the caller profile"
        );
        let before = engine.gpu_activity().submissions;
        if reason != "unfinished" {
            while engine.gpu_activity().submissions == before {
                assert!(
                    step(&engine, &mut working, &mut execution, &|| false).unwrap(),
                    "interruption must leave a partially advanced graph"
                );
            }
        }
        let outcome = match reason {
            "cancel" => step(&engine, &mut working, &mut execution, &|| true).map(|_| ()),
            "failure" => Err("injected job failure".into()),
            _ => Ok(()),
        };
        let result = execution.finish(&engine, &mut working, outcome, &mut profile);
        if reason == "cancel" {
            assert!(matches!(result.unwrap(), PhotoRender::Superseded));
        } else {
            assert!(result.is_err());
        }
        assert_eq!(profile.mode, Mode::Wall);
        assert_eq!(profile.host[0].stage, "caller observation");
        assert!(
            working.reduced(1).unwrap().last_settings.is_none(),
            "an incomplete job cannot commit requested settings"
        );
        let retry = image(
            working
                .render_reduced(&engine, 1, &settings, Purpose::Edit, || false, &mut profile)
                .unwrap()
                .unwrap(),
        );
        let mut ordinary = session(&engine, &source, &settings);
        let expected = image(
            engine
                .render_reduced_photo(
                    &mut ordinary,
                    &settings,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap(),
        );
        assert_eq!(
            engine.readback_image(&retry).unwrap(),
            engine.readback_image(&expected).unwrap()
        );
        working.clear().unwrap();
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn rejected_preparation_preserves_profile_and_releases_claim_for_retry() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let settings = settings();
    let mut ordinary = session(&engine, &source, &settings);
    let expected = image(
        engine
            .render_reduced_photo(
                &mut ordinary,
                &settings,
                &["final"],
                || false,
                &mut Profile::default(),
            )
            .unwrap(),
    );
    let expected_pixels = engine.readback_image(&expected).unwrap();
    for reason in ["invalid", "prepare-error", "cancel"] {
        let mut working = working(&engine, &source, &settings);
        let mut requested = settings.clone();
        if reason == "invalid" {
            requested.exposure = f32::NAN;
        } else if reason == "prepare-error" {
            requested.color_mode = "unavailable-test-color-mode".into();
        }
        let mut profile = profile();
        let before = engine.gpu_activity().submissions;
        let start = engine.begin_render_execution(
            &mut working,
            PhotoKey {
                photo: 1,
                resolution: Resolution::Reduced,
            },
            &requested,
            &["final"],
            &|| reason == "cancel",
            &mut profile,
            None,
        );
        if reason == "cancel" {
            assert!(matches!(start, Ok(RenderExecutionStart::Superseded)));
        } else {
            assert!(start.is_err(), "preparation must reject this request");
        }
        assert_eq!(
            engine.gpu_activity().submissions,
            before,
            "these rejected requests must not start photographic work"
        );
        assert_eq!(profile.mode, Mode::Wall);
        assert_eq!(profile.host[0].stage, "caller observation");
        assert!(working.reduced(1).unwrap().last_settings.is_none());
        let retry = image(
            working
                .render_reduced(&engine, 1, &settings, Purpose::Edit, || false, &mut profile)
                .unwrap()
                .unwrap(),
        );
        assert_eq!(engine.readback_image(&retry).unwrap(), expected_pixels);
        working.clear().unwrap();
    }
}

fn cost(output_bytes: u64, temporary_bytes: u64, released_bytes: u64) -> NodeCost {
    NodeCost {
        allocation: crate::render::allocation::GpuAllocation::new(output_bytes, temporary_bytes)
            .unwrap(),
        released_bytes,
    }
}

#[test]
fn reservation_covers_the_remaining_peak_and_delivery() {
    let reservation = Reservation {
        // The third node's output reuses the scratch the second node released.
        costs: vec![cost(120, 16, 0), cost(240, 32, 120), cost(60, 0, 240)],
        delivery_bytes: 512,
    };
    // Peaks carry 10% headroom for tile padding.
    assert_eq!(
        reservation.remaining_bytes(0),
        Some((120 + 240 + 32) * 11 / 10 + 512)
    );
    assert_eq!(reservation.remaining_bytes(2), Some(60 * 11 / 10 + 512));
    assert_eq!(reservation.remaining_bytes(3), Some(512));
}

#[test]
fn overflowing_reservation_is_unknown() {
    let reservation = Reservation {
        costs: vec![cost(u64::MAX, 0, 0), cost(1, 0, 0)],
        delivery_bytes: 0,
    };
    assert_eq!(reservation.remaining_bytes(0), None);
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn native_final_matches_ordinary_pixels_geometry_progress_and_reuses_retained_input() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    for quality in [
        super::super::ProcessingQuality::Quality,
        super::super::ProcessingQuality::Speed,
    ] {
        let mut base = settings();
        base.working_scale = Scale::NATIVE;
        base.preview_region = Some([600, 600, 800, 800]);
        base.processing_quality = quality;
        let mut edited = base.clone();
        edited.exposure = 0.25;
        edited.reconstruction.as_mut().unwrap().chroma = 1;
        let mut panned = edited.clone();
        panned.preview_region = Some([900, 700, 1100, 900]);
        let mut ordinary = super::super::NativePhoto::new(source.clone());
        let mut working = WorkingSet::new();
        working
            .insert_native(
                1,
                super::super::NativePhoto::new(source.clone()),
                &engine,
                Purpose::Edit,
            )
            .unwrap();
        for requested in [base, edited, panned] {
            let expected_progress = RefCell::new(Vec::new());
            let mut expected_profile = profile();
            let expected = image(PhotoRender::Rendered(
                engine
                    .render_native_photo_with_progress(
                        &mut ordinary,
                        &requested,
                        &["final"],
                        || false,
                        &mut expected_profile,
                        |event| expected_progress.borrow_mut().push(progress(event)),
                    )
                    .unwrap()
                    .unwrap(),
            ));
            let observed_progress = RefCell::new(Vec::new());
            let mut profile = profile();
            let observed = image(
                engine
                    .render_native(
                        &mut working,
                        1,
                        &requested,
                        &["final"],
                        Purpose::Edit,
                        || false,
                        &mut profile,
                        |event| observed_progress.borrow_mut().push(progress(event)),
                    )
                    .unwrap()
                    .unwrap(),
            );
            assert_eq!(
                engine.readback_image(&observed).unwrap(),
                engine.readback_image(&expected).unwrap()
            );
            assert_eq!(observed.description, expected.description);
            assert_eq!(observed.view, expected.view);
            assert_eq!(observed.settings, expected.settings);
            assert_eq!(*observed_progress.borrow(), *expected_progress.borrow());
            assert_eq!(profile.mode, Mode::Wall);
            assert_eq!(profile.host[0].stage, "caller observation");
            let expected_stages = native_cache_stages(&expected_profile);
            assert_eq!(
                expected_stages,
                ["prepare session cache", "finish session cache"]
            );
            assert_eq!(native_cache_stages(&profile), expected_stages);
            assert!(working
                .native(1)
                .unwrap()
                .changed_parameters(&requested)
                .is_empty());
            assert_eq!(working.with_native(1, |_| Ok(42)).unwrap(), Some(42));
        }
        working.clear().unwrap();
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn native_final_missing_failure_and_submitted_cancel_preserve_attempted_settings_and_retry() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let mut settings = settings();
    settings.working_scale = Scale::NATIVE;
    settings.preview_region = Some([600, 600, 800, 800]);
    let mut working = WorkingSet::new();
    let mut profile = profile();
    let before = engine.gpu_activity();
    assert!(engine
        .render_native(
            &mut working,
            1,
            &settings,
            &["final"],
            Purpose::Edit,
            || panic!("missing input must not execute"),
            &mut profile,
            |_| panic!("missing input must not report progress"),
        )
        .unwrap()
        .is_none());
    assert_eq!(engine.gpu_activity().submissions, before.submissions);
    assert_eq!(profile.host.len(), 1);
    assert!(native_cache_stages(&profile).is_empty());
    working
        .insert_native(
            1,
            super::super::NativePhoto::new(source.clone()),
            &engine,
            Purpose::Edit,
        )
        .unwrap();
    for reason in ["invalid", "cancel", "submitted-cancel"] {
        let mut requested = settings.clone();
        if reason == "invalid" {
            requested.exposure = f32::NAN;
        } else {
            requested.exposure = 0.75;
            requested.reconstruction.as_mut().unwrap().chroma = 2;
        }
        let before = engine.gpu_activity().submissions;
        let stages_before = native_cache_stages(&profile).len();
        let result = engine.render_native(
            &mut working,
            1,
            &requested,
            &["final"],
            Purpose::Edit,
            || {
                reason == "cancel"
                    || (reason == "submitted-cancel" && engine.gpu_activity().submissions > before)
            },
            &mut profile,
            |_| {},
        );
        if reason == "invalid" {
            assert!(result.is_err());
            assert!(working
                .native(1)
                .unwrap()
                .last_settings
                .as_ref()
                .unwrap()
                .exposure
                .is_nan());
        } else {
            assert!(matches!(result.unwrap(), Some(PhotoRender::Superseded)));
            assert!(working
                .native(1)
                .unwrap()
                .changed_parameters(&requested)
                .is_empty());
        }
        if reason == "submitted-cancel" {
            assert!(engine.gpu_activity().submissions > before);
        } else {
            assert_eq!(engine.gpu_activity().submissions, before);
        }
        assert_eq!(profile.mode, Mode::Wall);
        assert_eq!(profile.host[0].stage, "caller observation");
        assert_eq!(
            &native_cache_stages(&profile)[stages_before..],
            ["prepare session cache", "finish session cache"],
            "native cache stages must be retained after {reason}",
        );
        assert_eq!(working.with_native(1, |_| Ok(42)).unwrap(), Some(42));
        if reason == "submitted-cancel" {
            let stages_before = native_cache_stages(&profile).len();
            let observed = image(
                engine
                    .render_native(
                        &mut working,
                        1,
                        &requested,
                        &["final"],
                        Purpose::Edit,
                        || false,
                        &mut profile,
                        |_| {},
                    )
                    .unwrap()
                    .unwrap(),
            );
            let mut ordinary = super::super::NativePhoto::new(source.clone());
            let expected = image(PhotoRender::Rendered(
                engine
                    .render_native_photo(
                        &mut ordinary,
                        &requested,
                        &["final"],
                        || false,
                        &mut Profile::default(),
                    )
                    .unwrap()
                    .unwrap(),
            ));
            assert_eq!(
                engine.readback_image(&observed).unwrap(),
                engine.readback_image(&expected).unwrap()
            );
            assert_eq!(observed.description, expected.description);
            assert_eq!(observed.view, expected.view);
            assert_eq!(observed.settings, expected.settings);
            assert_eq!(
                &native_cache_stages(&profile)[stages_before..],
                ["prepare session cache", "finish session cache"],
            );
        }
    }
    working.clear().unwrap();
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn resolved_warm_chroma_forecast_covers_actual_gpu_allocations_and_transfers_outputs() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let base = settings();
    let mut working = working(&engine, &source, &base);
    image(
        working
            .render_reduced(
                &engine,
                1,
                &base,
                Purpose::Edit,
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap(),
    );
    let mut requested = base.clone();
    requested.reconstruction.as_mut().unwrap().chroma = 1;
    let mut profile = Profile::default();
    let mut execution = begin(&engine, &mut working, &requested, &mut profile);
    let recipe = &execution.recipe;
    let reservation = recipe.reservation.as_ref().expect("resolved continuation");
    let bytes = |cost: &NodeCost| cost.allocation.output_bytes + cost.allocation.temporary_bytes;
    let operations: Vec<_> = reservation
        .costs
        .iter()
        .enumerate()
        .filter(|(_, cost)| bytes(cost) > 0)
        .map(|(index, _)| recipe.prepared.built.graph.nodes[index].operation.kind())
        .collect();
    for audited in [
        "Texture",
        "TiledColor",
        "TiledColorDq",
        "ProjectColorTiles",
        "ResampleRgb",
        "Tone",
    ] {
        assert!(operations.contains(&audited), "{audited} in {operations:?}");
    }
    let graph_forecast: u64 = reservation.costs.iter().map(bytes).sum();
    let delivery = reservation.delivery_bytes;
    let before = engine.gpu_activity();
    while step(&engine, &mut working, &mut execution, &|| false).unwrap() {
        assert!(execution.future_bytes().is_some());
    }
    assert_eq!(execution.future_bytes(), Some(delivery));
    let activity = engine.gpu_activity().since(before);
    assert!(activity.dispatches > 0);
    let allocated = activity.storage_bytes + activity.upload_bytes + activity.uniform_bytes;
    assert!(
        allocated <= graph_forecast,
        "actual {allocated} exceeds forecast {graph_forecast}"
    );
    let mut retained = crate::recipe::RetainedBytes::default();
    assert!(
        execution.recipe.account(&mut retained),
        "audit every owned graph node, including cached nodes"
    );
    let observed = image(
        execution
            .finish(&engine, &mut working, Ok(()), &mut profile)
            .unwrap(),
    );
    let mut ordinary = session(&engine, &source, &base);
    let expected = image(
        engine
            .render_reduced_photo(
                &mut ordinary,
                &requested,
                &["final"],
                || false,
                &mut Profile::default(),
            )
            .unwrap(),
    );
    assert_eq!(
        engine.readback_image(&observed).unwrap(),
        engine.readback_image(&expected).unwrap()
    );
    assert_eq!(observed.description, expected.description);
    assert_eq!(observed.view, expected.view);
    working.clear().unwrap();
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with Classic, Merrill, Quattro and Quattro H originals"]
fn estimates_exist_for_every_family_and_view_and_cover_measured_peaks() {
    let root = PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    let engine = Engine::new().unwrap();
    let key = PhotoKey {
        photo: 1,
        resolution: Resolution::Native,
    };
    for name in ["sd14", "blue-parking-sign", "dp0-quattro", "sd-quattro-h"] {
        let source = Arc::new(Source::open(root.join(format!("{name}.x3f"))).unwrap());
        let mut views = vec![(Scale::new(4).unwrap(), None)];
        let mut regional = Settings::new("Sunlight", "Standard");
        regional.preview_region = Some([256, 256, 768, 768]);
        if regional.regional_preview_available(&source.metadata) {
            views.push((Scale::NATIVE, regional.preview_region));
        }
        for (scale, region) in views {
            let mut settings = Settings::new("Sunlight", "Standard");
            settings.working_scale = scale;
            settings.preview_region = region;
            let mut estimates = Vec::new();
            for mode in [Mode::Off, Mode::Wall] {
                let mut working = WorkingSet::new();
                working
                    .insert_native(
                        1,
                        crate::interactive::NativePhoto::new(source.clone()),
                        &engine,
                        Purpose::Edit,
                    )
                    .unwrap();
                let mut profile = Profile::new(mode);
                let RenderExecutionStart::Ready(mut execution) = engine
                    .begin_render_execution(
                        &mut working,
                        key,
                        &settings,
                        &["final"],
                        &|| false,
                        &mut profile,
                        None,
                    )
                    .unwrap()
                else {
                    panic!("{name}: retained native session")
                };
                let estimate = execution
                    .recipe
                    .future_bytes()
                    .unwrap_or_else(|| panic!("{name} {scale:?} {region:?}: no estimate"));
                while step(&engine, &mut working, &mut execution, &|| false).unwrap() {}
                // Wall profiling records live buffer bytes at node boundaries.
                let nodes = &execution.recipe.profile.nodes;
                if let Some(first) = nodes.first() {
                    let start = first.memory_before.union_bytes;
                    let grown = nodes
                        .iter()
                        .map(|node| node.memory_after_publication.union_bytes)
                        .max()
                        .unwrap()
                        .saturating_sub(start);
                    assert!(
                        estimate >= grown,
                        "{name} {scale:?} {region:?}: estimate {estimate} below measured {grown}"
                    );
                }
                image(
                    execution
                        .finish(&engine, &mut working, Ok(()), &mut profile)
                        .unwrap(),
                );
                estimates.push(estimate);
            }
            assert_eq!(
                estimates[0], estimates[1],
                "{name}: profiling changed the estimate"
            );
        }
    }
}
