//! Checks executions sharing ticks: retirement, completed progress and independent native and reduced results.

use super::*;
use crate::render::driver::tests::{fixture, image, session, settings};
use std::cell::RefCell;

struct Running {
    photo: u64,
    execution: Option<RenderExecution>,
    profile: Profile,
    progress: RefCell<Vec<RenderProgress>>,
    outcome: Option<Result<()>>,
}

fn start(engine: &Engine, working: &mut WorkingSet, photo: u64, settings: &Settings) -> Running {
    start_key(
        engine,
        working,
        PhotoKey {
            photo,
            resolution: Resolution::Reduced,
        },
        settings,
    )
}

fn start_key(
    engine: &Engine,
    working: &mut WorkingSet,
    key: PhotoKey,
    settings: &Settings,
) -> Running {
    let mut profile = Profile::default();
    let progress = RefCell::new(Vec::new());
    let RenderExecutionStart::Ready(execution) = engine
        .begin_render_execution(
            working,
            key,
            settings,
            &["final"],
            &|| false,
            &mut profile,
            Some(&|event| progress.borrow_mut().push(event)),
        )
        .unwrap()
    else {
        panic!("retained session execution")
    };
    Running {
        photo: key.photo,
        execution: Some(execution),
        profile,
        progress,
        outcome: None,
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn four_jobs_preserve_exact_outputs_and_report_progress_only_after_shared_completion() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let base = settings();
    let mut working = WorkingSet::new();
    let mut requested = Vec::new();
    let mut expected = Vec::new();
    for photo in 1..=4 {
        let mut next = base.clone();
        next.exposure += photo as f32 * 0.125;
        next.reconstruction.as_mut().unwrap().chroma = (photo % 3) as u32;
        let mut ordinary = session(&engine, &source, &base);
        image(
            engine
                .render_reduced_photo(
                    &mut ordinary,
                    &base,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap(),
        );
        let output = image(
            engine
                .render_reduced_photo(
                    &mut ordinary,
                    &next,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap(),
        );
        expected.push((
            engine.readback_image(&output).unwrap(),
            output.description,
            output.view,
        ));
        let mut warm = session(&engine, &source, &base);
        image(
            engine
                .render_reduced_photo(
                    &mut warm,
                    &base,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap(),
        );
        working
            .insert_reduced(photo, warm, &engine, Purpose::Speculative)
            .unwrap();
        requested.push(next);
    }
    let mut jobs: Vec<_> = requested
        .iter()
        .enumerate()
        .map(|(index, next)| start(&engine, &mut working, index as u64 + 1, next))
        .collect();
    let mut actual = Vec::new();
    let mut rounds = 0;
    while jobs.iter().any(|job| job.execution.is_some()) {
        rounds += 1;
        assert!(rounds < 128);
        let tick = engine.tick();
        for job in &mut jobs {
            let Some(execution) = &mut job.execution else {
                continue;
            };
            let previous = job.progress.borrow().len();
            let result = execution.step(&engine, &mut working, &|| false);
            assert_eq!(
                job.progress.borrow().len(),
                previous,
                "encoding is not completed progress"
            );
            match result {
                Ok(true) => {}
                Ok(false) => job.outcome = Some(Ok(())),
                Err(error) => job.outcome = Some(Err(error)),
            }
        }
        let waited = tick.complete().unwrap();
        for job in &mut jobs {
            let Some(execution) = &mut job.execution else {
                continue;
            };
            execution.complete_tick(&engine, waited, &|event| {
                job.progress.borrow_mut().push(event)
            });
            if let Some(outcome) = job.outcome.take() {
                let output = image(
                    job.execution
                        .take()
                        .unwrap()
                        .finish(&engine, &mut working, outcome, &mut job.profile)
                        .unwrap(),
                );
                actual.push((job.photo, output));
            }
        }
        let mut reservation = MemoryReservation::default();
        for (_, output) in &actual {
            reservation.add_image(output);
        }
        engine.trim_memory(&mut working, 0, &reservation);
        for job in jobs.iter().filter(|job| job.execution.is_some()) {
            assert!(
                working.reduced(job.photo).is_some(),
                "running peers stay protected"
            );
        }
    }
    assert_eq!(actual.len(), 4);
    for (photo, output) in actual {
        let (pixels, description, view) = &expected[photo as usize - 1];
        assert_eq!(engine.readback_image(&output).unwrap(), *pixels);
        assert_eq!(output.description, *description);
        assert_eq!(output.view, *view);
        let stages: Vec<_> = jobs[photo as usize - 1]
            .progress
            .borrow()
            .iter()
            .filter_map(|event| match event {
                RenderProgress::Stage { completed, total } => Some((*completed, *total)),
                _ => None,
            })
            .collect();
        assert!(stages
            .last()
            .is_some_and(|(completed, total)| completed == total));
    }
    working.clear().unwrap();
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn cancelled_peer_retries_without_source_while_unchanged_peer_commits() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let base = settings();
    let mut requested = base.clone();
    requested.reconstruction.as_mut().unwrap().chroma = 1;
    let mut working = WorkingSet::new();
    for photo in 1..=2 {
        let mut warm = session(&engine, &source, &base);
        image(
            engine
                .render_reduced_photo(
                    &mut warm,
                    &base,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap(),
        );
        working
            .insert_reduced(photo, warm, &engine, Purpose::Speculative)
            .unwrap();
    }
    let mut jobs = [
        start(&engine, &mut working, 1, &requested),
        start(&engine, &mut working, 2, &requested),
    ];
    let mut round = 0;
    let mut completed_peer = None;
    while jobs.iter().any(|job| job.execution.is_some()) {
        round += 1;
        assert!(round < 128);
        let tick = engine.tick();
        for job in &mut jobs {
            let Some(execution) = &mut job.execution else {
                continue;
            };
            match execution.step(&engine, &mut working, &|| job.photo == 1 && round > 1) {
                Ok(true) => {}
                Ok(false) => job.outcome = Some(Ok(())),
                Err(error) => job.outcome = Some(Err(error)),
            }
        }
        let waited = tick.complete().unwrap();
        for job in &mut jobs {
            let Some(execution) = &mut job.execution else {
                continue;
            };
            execution.complete_tick(&engine, waited, &|event| {
                job.progress.borrow_mut().push(event)
            });
            if let Some(outcome) = job.outcome.take() {
                let result = job
                    .execution
                    .take()
                    .unwrap()
                    .finish(&engine, &mut working, outcome, &mut job.profile)
                    .unwrap();
                if job.photo == 1 {
                    assert!(matches!(result, PhotoRender::Superseded));
                } else {
                    completed_peer = Some(image(result));
                }
            }
        }
    }
    let retry = image(
        working
            .render_reduced(
                &engine,
                1,
                &requested,
                Purpose::Edit,
                || false,
                &mut Profile::default(),
            )
            .unwrap()
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
    let expected = engine.readback_image(&expected).unwrap();
    assert_eq!(engine.readback_image(&retry).unwrap(), expected);
    assert_eq!(
        engine.readback_image(&completed_peer.unwrap()).unwrap(),
        expected
    );
    working.clear().unwrap();
}

fn native_key(photo: u64) -> PhotoKey {
    PhotoKey {
        photo,
        resolution: Resolution::Native,
    }
}

fn native_base() -> Settings {
    let mut base = settings();
    base.working_scale = crate::render::Scale::NATIVE;
    base.preview_region = Some([600, 600, 800, 800]);
    base
}

fn native_image(
    engine: &Engine,
    session: &mut crate::render::NativePhoto,
    settings: &Settings,
) -> crate::image::RenderedImage {
    image(PhotoRender::Rendered(
        engine
            .render_native_photo(
                session,
                settings,
                &["final"],
                || false,
                &mut Profile::default(),
            )
            .unwrap()
            .unwrap(),
    ))
}

fn warm_native(
    engine: &Engine,
    source: &Arc<crate::source::Source>,
    base: &Settings,
) -> crate::render::NativePhoto {
    let mut session = crate::render::NativePhoto::new(source.clone());
    drop(native_image(engine, &mut session, base));
    session
}

fn finish_jobs(
    engine: &Engine,
    working: &mut WorkingSet,
    jobs: &mut [Running],
) -> Vec<(u64, crate::image::RenderedImage)> {
    let mut images = Vec::new();
    for _ in 0..256 {
        if jobs.iter().all(|job| job.execution.is_none()) {
            return images;
        }
        let tick = engine.tick();
        for job in jobs.iter_mut() {
            if let Some(execution) = &mut job.execution {
                let progress_before = job.progress.borrow().len();
                if !execution.step(engine, working, &|| false).unwrap() {
                    job.outcome = Some(Ok(()));
                }
                assert_eq!(job.progress.borrow().len(), progress_before);
            }
        }
        let waited = tick.complete().unwrap();
        for job in jobs.iter_mut() {
            if let Some(execution) = &mut job.execution {
                execution.complete_tick(engine, waited, &|stage| {
                    job.progress.borrow_mut().push(stage)
                });
            }
            if let Some(outcome) = job.outcome.take() {
                let rendered = job
                    .execution
                    .take()
                    .unwrap()
                    .finish(engine, working, outcome, &mut job.profile)
                    .unwrap();
                images.push((job.photo, image(rendered)));
            }
        }
    }
    panic!("native graph did not settle");
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn native_regions_width_one_and_two_match_ordinary_pixels_and_geometry() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    for quality in [
        crate::render::ProcessingQuality::Quality,
        crate::render::ProcessingQuality::Speed,
    ] {
        let mut base = native_base();
        base.processing_quality = quality;
        let mut same_region = base.clone();
        same_region.exposure = 0.5;
        same_region.reconstruction.as_mut().unwrap().chroma = 1;
        let mut panned = same_region.clone();
        panned.preview_region = Some([900, 700, 1100, 900]);
        let requests = [same_region, panned];
        let expected: Vec<_> = requests
            .iter()
            .map(|request| {
                let mut ordinary = warm_native(&engine, &source, &base);
                let output = native_image(&engine, &mut ordinary, request);
                assert_eq!(output.settings.preview_region, request.preview_region);
                (
                    engine.readback_image(&output).unwrap(),
                    output.description,
                    output.view,
                )
            })
            .collect();
        for width in [1, 2] {
            let mut working = WorkingSet::new();
            for photo in 1..=2 {
                working
                    .insert_native(
                        photo,
                        warm_native(&engine, &source, &base),
                        &engine,
                        Purpose::Speculative,
                    )
                    .unwrap();
            }
            let mut actual = Vec::new();
            for first in (0..2).step_by(width) {
                let mut jobs: Vec<_> = (first..first + width)
                    .map(|index| {
                        start_key(
                            &engine,
                            &mut working,
                            native_key(index as u64 + 1),
                            &requests[index],
                        )
                    })
                    .collect();
                for job in &jobs {
                    assert!(
                        working.with_native(job.photo, |_| Ok(())).is_err(),
                        "claimed cache cannot be borrowed by another execution"
                    );
                    assert!(working.remove_photo(job.photo).is_err());
                }
                actual.extend(finish_jobs(&engine, &mut working, &mut jobs));
                for job in &jobs {
                    assert!(
                        matches!(job.progress.borrow().last(), Some(RenderProgress::Stage { completed, total }) if completed == total)
                    );
                }
            }
            for (photo, output) in actual {
                let index = photo as usize - 1;
                assert_eq!(engine.readback_image(&output).unwrap(), expected[index].0);
                assert_eq!(output.description, expected[index].1);
                assert_eq!(output.view, expected[index].2);
                assert_eq!(
                    output.settings.preview_region,
                    requests[index].preview_region
                );
            }
            working.clear().unwrap();
        }
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn native_cancelled_submission_retires_claim_and_preserves_peer_and_retry() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let base = native_base();
    let mut requested = base.clone();
    requested.reconstruction.as_mut().unwrap().chroma = 2;
    let expected = {
        let mut ordinary = warm_native(&engine, &source, &base);
        let output = native_image(&engine, &mut ordinary, &requested);
        (
            engine.readback_image(&output).unwrap(),
            output.description,
            output.view,
        )
    };
    let mut working = WorkingSet::new();
    for photo in 1..=2 {
        working
            .insert_native(
                photo,
                warm_native(&engine, &source, &base),
                &engine,
                Purpose::Speculative,
            )
            .unwrap();
    }
    let mut jobs = [
        start_key(&engine, &mut working, native_key(1), &requested),
        start_key(&engine, &mut working, native_key(2), &requested),
    ];
    let mut submitted = false;
    let mut cancelled = false;
    let mut peer = None;
    for _ in 0..256 {
        if jobs.iter().all(|job| job.execution.is_none()) {
            break;
        }
        let tick = engine.tick();
        for job in &mut jobs {
            let Some(execution) = &mut job.execution else {
                continue;
            };
            let before = engine.gpu_activity().submissions;
            match execution.step(&engine, &mut working, &|| job.photo == 1 && submitted) {
                Ok(true) => {}
                Ok(false) => job.outcome = Some(Ok(())),
                Err(error) => job.outcome = Some(Err(error)),
            }
            if job.photo == 1 && engine.gpu_activity().submissions > before {
                submitted = true;
                assert!(
                    job.outcome.is_none(),
                    "cancel requires a live submitted cursor"
                );
            }
        }
        let waited = tick.complete().unwrap();
        for job in &mut jobs {
            if let Some(execution) = &mut job.execution {
                execution.complete_tick(&engine, waited, &|event| {
                    job.progress.borrow_mut().push(event)
                });
            }
            if let Some(outcome) = job.outcome.take() {
                let result = job
                    .execution
                    .take()
                    .unwrap()
                    .finish(&engine, &mut working, outcome, &mut job.profile)
                    .unwrap();
                if job.photo == 1 {
                    assert!(matches!(result, PhotoRender::Superseded));
                    cancelled = true;
                    assert!(
                        working
                            .native(1)
                            .unwrap()
                            .changed_parameters(&requested)
                            .is_empty(),
                        "native sessions retain attempted settings after cancellation"
                    );
                } else {
                    peer = Some(image(result));
                }
            }
        }
    }
    assert!(submitted && cancelled);
    assert!(jobs.iter().all(|job| job.execution.is_none()));
    let peer = peer.unwrap();
    assert_eq!(engine.readback_image(&peer).unwrap(), expected.0);
    assert_eq!(peer.description, expected.1);
    assert_eq!(peer.view, expected.2);
    let mut retry = [start_key(&engine, &mut working, native_key(1), &requested)];
    let retry = finish_jobs(&engine, &mut working, &mut retry)
        .pop()
        .unwrap()
        .1;
    assert_eq!(engine.readback_image(&retry).unwrap(), expected.0);
    assert_eq!(retry.description, expected.1);
    assert_eq!(retry.view, expected.2);
    working.clear().unwrap();
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn native_prepare_failure_and_early_cancel_release_claim_without_submission() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let mut working = WorkingSet::new();
    working
        .insert_native(
            1,
            crate::render::NativePhoto::new(source),
            &engine,
            Purpose::Edit,
        )
        .unwrap();
    for invalid in [true, false] {
        let mut requested = native_base();
        requested.exposure = if invalid { f32::NAN } else { 0.75 };
        let mut profile = Profile::new(crate::profile::Mode::Wall);
        profile.host.push(crate::profile::Timing {
            stage: "caller observation",
            milliseconds: 0.,
        });
        let before = engine.gpu_activity().submissions;
        let result = engine.begin_render_execution(
            &mut working,
            native_key(1),
            &requested,
            &["final"],
            &|| !invalid,
            &mut profile,
            None,
        );
        if invalid {
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
            assert!(matches!(result.unwrap(), RenderExecutionStart::Superseded));
            assert_eq!(
                working
                    .native(1)
                    .unwrap()
                    .last_settings
                    .as_ref()
                    .unwrap()
                    .exposure,
                requested.exposure
            );
        }
        assert_eq!(engine.gpu_activity().submissions, before);
        assert!(profile
            .host
            .iter()
            .any(|timing| timing.stage == "caller observation"));
        assert_eq!(working.with_native(1, |_| Ok(42)).unwrap(), Some(42));
    }
    working.clear().unwrap();
}

#[test]
#[ignore = "requires GPU and FOVEONIC_FIXTURE_DIR with the pinned Merrill example"]
fn execution_inventory_deduplicates_two_live_graphs_caches_and_frontend_owners() {
    let engine = Engine::new().unwrap();
    let source = fixture();
    let base = settings();
    let mut working = WorkingSet::new();
    let mut frontend = MemoryReservation::default();
    frontend.exclusive_bytes = 257;
    frontend.add_source(&source);
    let mut displayed = Vec::new();
    for photo in 1..=2 {
        let mut warm = session(&engine, &source, &base);
        let output = image(
            engine
                .render_reduced_photo(
                    &mut warm,
                    &base,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )
                .unwrap(),
        );
        frontend.add_image(&output);
        displayed.push(output);
        working
            .insert_reduced(photo, warm, &engine, Purpose::Speculative)
            .unwrap();
    }
    assert_eq!(
        engine.execution_memory_resources(&working, &frontend, &[]),
        engine.memory_resources(&working, &frontend),
    );
    let mut requested = base.clone();
    requested.reconstruction.as_mut().unwrap().chroma = 1;
    let mut jobs = [
        start(&engine, &mut working, 1, &requested),
        start(&engine, &mut working, 2, &requested),
    ];
    let mut cache_shared = [false; 2];
    let mut cursor_only_seen = false;
    for round in 0..128 {
        let executions = jobs
            .iter()
            .map(|job| job.execution.as_ref().unwrap())
            .collect::<Vec<_>>();
        let baseline = engine.memory_resources(&working, &frontend);
        let resources = engine.execution_memory_resources(&working, &frontend, &executions);
        let mut buffers = working
            .memory_buffers()
            .map(|buffer| (buffer.identity().0, buffer.size()))
            .collect::<std::collections::HashMap<_, _>>();
        for (index, execution) in executions.iter().enumerate() {
            for buffer in execution.recipe.traversal.buffers() {
                let retained = working
                    .memory_buffers()
                    .any(|cached| cached.shares_storage(buffer));
                cache_shared[index] |= retained;
                cursor_only_seen |= !retained;
                buffers.insert(buffer.identity().0, buffer.size());
            }
        }
        for output in &displayed {
            buffers.insert(output.buffer.identity().0, output.buffer.size());
        }
        assert_eq!(resources.gpu_bytes, buffers.values().sum::<u64>());
        assert_eq!(resources.reserved_bytes, 257);
        assert!(
            resources.cpu_bytes > baseline.cpu_bytes,
            "live job containers join the inventory"
        );
        assert_eq!(
            resources.total_bytes,
            resources.cpu_bytes + resources.gpu_bytes + 257
        );
        let repeated = [executions[0], executions[1], executions[0]];
        assert_eq!(
            engine.execution_memory_resources(&working, &frontend, &repeated),
            resources
        );
        let mut repeated_frontend = frontend.clone();
        repeated_frontend.add_source(&source);
        for output in &displayed {
            repeated_frontend.add_image(output);
            repeated_frontend.add_metadata(&source.metadata);
        }
        assert_eq!(
            engine.execution_memory_resources(&working, &repeated_frontend, &executions),
            resources
        );
        assert_eq!(
            engine.execution_memory_resources(&working, &frontend, &[]),
            baseline,
        );
        if jobs.iter().all(|job| job.outcome.is_some()) {
            break;
        }
        assert!(round < 127, "both inventory jobs must complete");
        let tick = engine.tick();
        for job in &mut jobs {
            if job.outcome.is_some() {
                continue;
            }
            match job
                .execution
                .as_mut()
                .unwrap()
                .step(&engine, &mut working, &|| false)
            {
                Ok(true) => {}
                Ok(false) => job.outcome = Some(Ok(())),
                Err(error) => job.outcome = Some(Err(error)),
            }
        }
        let waited = tick.complete().unwrap();
        for job in &mut jobs {
            job.execution
                .as_mut()
                .unwrap()
                .complete_tick(&engine, waited, &|stage| {
                    job.progress.borrow_mut().push(stage)
                });
        }
    }
    assert!(
        cache_shared.into_iter().all(|shared| shared),
        "both graphs borrow cached storage"
    );
    assert!(
        cursor_only_seen,
        "include live graph or scratch storage outside the cache"
    );
    for job in &mut jobs {
        drop(image(
            job.execution
                .take()
                .unwrap()
                .finish(
                    &engine,
                    &mut working,
                    job.outcome.take().unwrap(),
                    &mut job.profile,
                )
                .unwrap(),
        ));
    }
    assert_eq!(
        engine.execution_memory_resources(&working, &frontend, &[]),
        engine.memory_resources(&working, &frontend),
    );
    working.clear().unwrap();
}
