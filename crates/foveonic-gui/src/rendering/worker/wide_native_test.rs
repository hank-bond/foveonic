//! Checks native cohort lifetimes, adaptive admission and retained warm-edit work.

use super::*;

#[test]
#[ignore = "requires GPU, pinned Merrill fixtures and FOVEONIC_WIDE_RENDER_WIDTH=2; run alone"]
fn submitted_native_replacement_preserves_peer_and_latest_pixels() {
    assert_eq!(std::env::var("FOVEONIC_WIDE_RENDER_WIDTH").unwrap(), "2");
    let scale = Scale::NATIVE;
    let verifier = Engine::new().unwrap();
    let fixtures = fixtures_for(
        &verifier,
        2,
        scale,
        "pan",
        Some([1024, 1024, 1536, 1536]),
        ProcessingQuality::Speed,
    );
    let worker = Worker::new(verifier.executor(), Default::default());
    worker.cache.configure(foveonic::decode_cache::Settings {
        enabled: false,
        ..Default::default()
    });
    worker.set_memory_budget(4 * 1024 * 1024 * 1024, Default::default());
    worker.set_background_completion(false);
    let mut markers = None;
    let baseline = batch(
        &worker,
        fixtures
            .iter()
            .enumerate()
            .map(|(i, f)| request(&f.path, &f.baseline, scale, 1 + i as u64))
            .collect(),
        "baseline",
        0,
        &mut markers,
    );
    verify(&worker, &baseline.frames, &fixtures, scale, false);
    let loads = worker.checks.source_loads.load(Relaxed);
    let (reached, arrival) = crossbeam_channel::bounded(1);
    let (release, resume) = crossbeam_channel::bounded(1);
    *worker.checks.wide_window_hold.lock().unwrap() = Some((reached, resume));
    let edits = fixtures
        .iter()
        .enumerate()
        .map(|(i, f)| request(&f.path, &f.edited, scale, 21 + i as u64))
        .collect::<Vec<_>>();
    set_batch_view(&worker, &edits);
    let paths = fixtures.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
    worker.set_overviews(edits.clone(), &paths, &paths, Instant::now());
    arrival
        .recv_timeout(Duration::from_secs(60))
        .expect("two photos submitted before replacement");
    assert_eq!(worker.checks.wide_max_live.load(Relaxed), 2);
    assert_eq!(worker.checks.wide_max_submitted.load(Relaxed), 2);
    let mut latest = edits.clone();
    latest[0].revision = 31;
    // Return to the original region after replacement-region work has submitted.
    latest[0].settings = fixtures[0].baseline.clone();
    set_batch_view(&worker, &latest);
    worker.set_overviews(latest, &paths, &paths, Instant::now());
    // Source and target identities in the shared plan establish that the
    // coordinator processed replacement before the test releases GPU progress.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let plan = worker.test_pending.0.lock().unwrap().plan.clone();
        if plan
            .tasks
            .iter()
            .any(|task| task.request.path == paths[0] && task.stamp.revision == 31)
        {
            break;
        }
        assert!(Instant::now() < deadline, "replacement was not published");
        std::thread::yield_now();
    }
    drop(release);
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut received = HashSet::new();
    while received.len() < 2 {
        let Update::Image(reply) = worker
            .replies
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap()
        else {
            panic!("image reply");
        };
        if reply.revision == 21 {
            assert!(
                reply.result.unwrap().is_none(),
                "obsolete submitted job must not deliver pixels"
            );
            continue;
        }
        assert!(matches!(reply.revision, 22 | 31));
        assert!(reply.handling.unwrap().current);
        let frame = reply.result.unwrap().unwrap();
        assert!(received.insert(frame.path.clone()));
        assert_eq!(
            frame.path,
            if frame.revision == 31 {
                paths[0].clone()
            } else {
                paths[1].clone()
            }
        );
        verify(
            &worker,
            std::slice::from_ref(&frame),
            &fixtures,
            scale,
            frame.revision == 22,
        );
        worker.progress.finish(frame.revision, false);
        worker.delivered(frame.revision);
    }
    wait_settled(&worker);
    assert_eq!(
        worker.checks.source_loads.load(Relaxed),
        loads,
        "replacement and peer must retain their own native sources"
    );
    worker.clear_all();
    wait_settled(&worker);
}

#[test]
#[ignore = "requires GPU and pinned Merrill fixtures; run alone"]
fn native_admission_limits_extra_work_then_recovers_after_headroom_returns() {
    let verifier = Engine::new().unwrap();
    let scale = Scale::NATIVE;
    let fixtures = fixtures_for(
        &verifier,
        4,
        scale,
        "chroma",
        Some([1024, 1024, 1536, 1536]),
        ProcessingQuality::Speed,
    );
    let worker = Worker::new(verifier.executor(), Default::default());
    worker.cache.configure(foveonic::decode_cache::Settings {
        enabled: false,
        ..Default::default()
    });
    worker.set_render_width(4);
    worker.set_memory_budget(4 * 1024 * 1024 * 1024, Default::default());
    worker.set_background_completion(false);
    let mut markers = None;
    let requests = |edited: bool, revision| {
        fixtures
            .iter()
            .enumerate()
            .map(|(i, f)| {
                request(
                    &f.path,
                    if edited { &f.edited } else { &f.baseline },
                    scale,
                    revision + i as u64,
                )
            })
            .collect()
    };
    let baseline = batch(&worker, requests(false, 1), "baseline", 0, &mut markers);
    verify(&worker, &baseline.frames, &fixtures, scale, false);
    let loads = worker.checks.source_loads.load(Relaxed);
    worker.set_memory_budget(3 * 1024 * 1024 * 1024, Default::default());
    worker.checks.wide_max_live.store(0, Relaxed);
    worker.checks.wide_max_submitted.store(0, Relaxed);
    let edited = batch(&worker, requests(true, 21), "edit", 0, &mut markers);
    verify(&worker, &edited.frames, &fixtures, scale, true);
    let limited = worker.checks.wide_max_live.load(Relaxed);
    assert!(
        (1..4).contains(&limited),
        "three-photo capacity must leave fewer admission slots: {limited}"
    );
    assert_eq!(worker.checks.wide_max_submitted.load(Relaxed), limited);
    assert_eq!(
        worker.checks.source_loads.load(Relaxed),
        loads,
        "admission must not evict warm inputs to force a wider group"
    );
    drop(baseline);
    drop(edited);
    worker.set_memory_budget(6 * 1024 * 1024 * 1024, Default::default());
    let reverted = batch(&worker, requests(false, 41), "baseline", 1, &mut markers);
    verify(&worker, &reverted.frames, &fixtures, scale, false);
    drop(reverted);
    worker.checks.wide_max_live.store(0, Relaxed);
    worker.checks.wide_max_submitted.store(0, Relaxed);
    let recovered = batch(&worker, requests(true, 61), "edit", 1, &mut markers);
    verify(&worker, &recovered.frames, &fixtures, scale, true);
    assert_eq!(worker.checks.wide_max_live.load(Relaxed), 4);
    assert_eq!(worker.checks.wide_max_submitted.load(Relaxed), 4);
    assert_eq!(worker.checks.source_loads.load(Relaxed), loads);
}

#[test]
#[ignore = "requires GPU and pinned Merrill fixtures; run alone"]
fn live_width_and_budget_reduction_retires_peers_and_settles_latest_work() {
    let verifier = Engine::new().unwrap();
    let scale = Scale::NATIVE;
    let fixtures = fixtures_for(
        &verifier,
        2,
        scale,
        "chroma",
        Some([1024, 1024, 1536, 1536]),
        ProcessingQuality::Speed,
    );
    let worker = Worker::new(verifier.executor(), Default::default());
    worker.cache.configure(foveonic::decode_cache::Settings {
        enabled: false,
        ..Default::default()
    });
    worker.set_render_width(2);
    worker.set_memory_budget(4 * 1024 * 1024 * 1024, Default::default());
    worker.set_background_completion(false);
    let mut markers = None;
    let requests = |edited: bool, revision| {
        fixtures
            .iter()
            .enumerate()
            .map(|(i, f)| {
                request(
                    &f.path,
                    if edited { &f.edited } else { &f.baseline },
                    scale,
                    revision + i as u64,
                )
            })
            .collect::<Vec<_>>()
    };
    batch(&worker, requests(false, 1), "baseline", 0, &mut markers);
    let (reached, arrival) = crossbeam_channel::bounded(1);
    let (release, resume) = crossbeam_channel::bounded(1);
    *worker.checks.wide_window_hold.lock().unwrap() = Some((reached, resume));
    let edits = requests(true, 21);
    let paths = fixtures.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
    set_batch_view(&worker, &edits);
    worker.set_overviews(edits, &paths, &paths, Instant::now());
    arrival
        .recv_timeout(Duration::from_secs(60))
        .expect("two live jobs at a completed window");
    worker.set_render_width(1);
    worker.set_memory_budget(1024 * 1024 * 1024, Default::default());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = worker.test_pending.0.lock().unwrap();
        if state.render_width.is_none()
            && state
                .memory
                .as_ref()
                .is_some_and(|b| b.limit == 1024 * 1024 * 1024)
        {
            break;
        }
        drop(state);
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    drop(release);
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut seen = HashSet::new();
    while seen.len() < fixtures.len() {
        let Update::Image(reply) = worker
            .replies
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap()
        else {
            panic!("image reply");
        };
        assert!(reply.handling.unwrap().current);
        let frame = reply
            .result
            .unwrap()
            .expect("yielded work must retry before delivery");
        assert!(seen.insert(frame.path.clone()));
        verify(
            &worker,
            std::slice::from_ref(&frame),
            &fixtures,
            scale,
            true,
        );
        worker.progress.finish(frame.revision, false);
        worker.delivered(frame.revision);
    }
    wait_settled(&worker);
    let admitted = worker.checks.wide_admitted.load(Relaxed);
    let baseline = batch(&worker, requests(false, 41), "serial", 1, &mut markers);
    verify(&worker, &baseline.frames, &fixtures, scale, false);
    assert_eq!(
        worker.checks.wide_admitted.load(Relaxed),
        admitted,
        "width one must use serial completion"
    );
}

#[test]
#[ignore = "requires GPU and pinned Merrill fixtures; run alone"]
fn constrained_noise_to_tone_edits_keep_warm_inputs_and_reduce_concurrency() {
    use foveonic::render::{ProcessingWorkingSet, RenderSession, SessionUse};
    use std::sync::Arc;

    let verifier = Engine::new().unwrap();
    let scale = Scale::NATIVE;
    let fixtures = fixtures_for(
        &verifier,
        2,
        scale,
        "chroma",
        Some([1024, 1024, 1536, 1536]),
        ProcessingQuality::Speed,
    );
    let phases = fixtures
        .iter()
        .map(|fixture| {
            let mut settings = fixture.baseline.clone();
            let mut phases = Vec::new();
            for chroma in [2, 1, 3] {
                settings.reconstruction.as_mut().unwrap().chroma = chroma;
                phases.push(settings.clone());
            }
            for shadows in [-0.4, 0.5, -0.2, 0.3] {
                settings.tonal_ranges.shadows = shadows;
                phases.push(settings.clone());
            }
            phases
        })
        .collect::<Vec<_>>();
    // Establish warm work independently, one photo at a time without pressure.
    // The pressured worker must reuse the same upstream inputs, regardless of
    // how many peers it can admit. These are work counts, not latency limits.
    let control = fixtures
        .iter()
        .zip(&phases)
        .map(|(fixture, phases)| {
            let source = Arc::new(Source::open(&fixture.path).unwrap());
            let mut working = ProcessingWorkingSet::new();
            working
                .insert_native(
                    1,
                    RenderSession::new(source.clone()),
                    &verifier,
                    SessionUse::Foreground,
                )
                .unwrap();
            phases
                .iter()
                .map(|settings| {
                    let request = request(&fixture.path, settings, scale, 0);
                    let mut resolved = settings.clone();
                    assert!(crate::export::resolve_camera_settings(
                        &mut resolved,
                        &source.metadata
                    )
                    .is_empty());
                    request
                        .apply_preview(&mut resolved, &source.metadata)
                        .unwrap();
                    let before = verifier.gpu_activity();
                    drop(
                        verifier
                            .render_native(
                                &mut working,
                                1,
                                &resolved,
                                &["final"],
                                SessionUse::Foreground,
                                || false,
                                &mut Profile::default(),
                                |_| {},
                                None,
                            )
                            .unwrap()
                            .unwrap(),
                    );
                    verifier.gpu_activity().since(before).dispatches
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let worker = Worker::new(verifier.executor(), Default::default());
    worker.cache.configure(foveonic::decode_cache::Settings {
        enabled: false,
        ..Default::default()
    });
    worker.set_render_width(8);
    worker.set_memory_budget(4 * 1024 * 1024 * 1024, Default::default());
    worker.set_background_completion(false);
    let mut markers = None;
    let mut loads = 0;
    for phase in 0..phases[0].len() {
        if phase == 4 {
            let actual = worker.memory_status().resources.total_bytes;
            let mut frontend = crate::memory::Inventory::default();
            // Retained pixels fit. There is deliberately too little spare
            // space to justify a second workspace. This pool owns no pixels.
            frontend.cpu(
                (4_u64 * 1024 * 1024 * 1024)
                    .checked_sub(actual + 64 * 1024 * 1024)
                    .unwrap(),
            );
            worker.set_memory_budget(4 * 1024 * 1024 * 1024, frontend);
            worker.checks.wide_max_live.store(0, Relaxed);
        }
        let requests = fixtures
            .iter()
            .enumerate()
            .map(|(i, fixture)| {
                request(
                    &fixture.path,
                    &phases[i][phase],
                    scale,
                    (phase * 20 + i + 1) as u64,
                )
            })
            .collect();
        let rendered = batch(&worker, requests, "noise-to-tone", phase, &mut markers);
        if phase == 0 {
            loads = worker.checks.source_loads.load(Relaxed);
        }
        if phase >= 4 {
            assert_eq!(worker.checks.source_loads.load(Relaxed), loads);
            let work = worker.checks.recipes.lock().unwrap().clone();
            for (i, fixture) in fixtures.iter().enumerate() {
                let revision = (phase * 20 + i + 1) as u64;
                let dispatches: u64 = work
                    .iter()
                    .filter(|(r, _)| *r == revision)
                    .map(|(_, n)| n)
                    .sum();
                assert_eq!(
                    dispatches,
                    control[i][phase],
                    "warm tone work for {} phase {phase}",
                    fixture.path.display()
                );
                let frame = rendered
                    .frames
                    .iter()
                    .find(|f| f.path == fixture.path)
                    .unwrap();
                let expected = expected(
                    &verifier,
                    &Source::open(&fixture.path).unwrap(),
                    &request(&fixture.path, &phases[i][phase], scale, revision),
                );
                assert_eq!(frame.artifact.settings, expected.settings);
                let Content::Image(image) = &frame.artifact.content else {
                    panic!("final photographic image");
                };
                assert_eq!(image.description(), expected.description);
                assert_eq!(image.view(), expected.view);
                let capture = tempfile::tempdir().unwrap();
                let path = capture.path().join("tone.f32");
                worker
                    .capture_image(image.as_ref().clone(), path.clone())
                    .recv_timeout(Duration::from_secs(30))
                    .unwrap()
                    .unwrap();
                assert_eq!(std::fs::read(path).unwrap(), expected.samples);
            }
        }
        if phase == 4 {
            assert_eq!(
                worker.checks.wide_max_live.load(Relaxed),
                1,
                "retain warm inputs instead of making room for a peer"
            );
        }
        drop(rendered);
    }
}
