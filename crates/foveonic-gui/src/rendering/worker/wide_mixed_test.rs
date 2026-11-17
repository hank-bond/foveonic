//! Checks independent warm-peer completion when cached or cold viewport demand arrives.

use super::*;

#[derive(Clone, Copy)]
enum Third {
    Cached,
    Cold,
}

fn receive_frame(worker: &Worker, wanted: &[Request], seen: &mut HashSet<PathBuf>) -> Frame {
    let Update::Image(reply) = worker
        .replies
        .recv_timeout(Duration::from_secs(120))
        .unwrap()
    else {
        panic!("image reply");
    };
    assert!(wanted
        .iter()
        .any(|request| { request.path == reply.path && request.revision == reply.revision }));
    let handling = reply.handling.unwrap();
    assert!(handling.current && handling.update_photo);
    let frame = reply
        .result
        .unwrap()
        .expect("latest target must deliver pixels");
    assert!(seen.insert(frame.path.clone()), "one completion per target");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !worker.is_settled_and_admitted(&frame) {
        assert!(
            Instant::now() < deadline,
            "frame was not settled and admitted"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    worker.progress.finish(frame.revision, false);
    worker.delivered(frame.revision);
    frame
}

fn run_arrival(
    engine: &Engine,
    fixtures: &[Fixture],
    peer_scale: Scale,
    third: Third,
    arrival: bool,
) -> serde_json::Value {
    let reduced_scale = Scale::new(8).unwrap();
    let scale_for = |path: &Path| {
        if path == fixtures[2].path {
            reduced_scale
        } else {
            peer_scale
        }
    };
    let worker = Worker::new(engine.executor(), Default::default());
    worker.cache.configure(foveonic::decode_cache::Settings {
        enabled: false,
        ..Default::default()
    });
    worker.set_memory_budget(4 * 1024 * 1024 * 1024, Default::default());
    worker.set_background_completion(false);
    let paths = fixtures.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
    let warm_count = if matches!(third, Third::Cached) { 3 } else { 2 };
    let baseline = batch(
        &worker,
        fixtures[..warm_count]
            .iter()
            .enumerate()
            .map(|(i, f)| request(&f.path, &f.baseline, scale_for(&f.path), 1 + i as u64))
            .collect(),
        "mixed baseline",
        0,
        &mut None,
    );
    let origins = baseline
        .frames
        .iter()
        .map(|f| (f.path.clone(), f.origin.clone()))
        .collect::<Vec<_>>();
    for frame in &baseline.frames {
        verify(
            &worker,
            std::slice::from_ref(frame),
            fixtures,
            scale_for(&frame.path),
            false,
        );
    }
    drop(baseline);
    let activity_before = *worker.checks.gpu_activity.lock().unwrap();
    let source_loads = worker.checks.source_loads.load(Relaxed);
    let admitted = worker.checks.wide_admitted.load(Relaxed);
    let windows = worker.checks.wide_windows.load(Relaxed);
    let submission_start = worker.checks.wide_submissions.lock().unwrap().len();
    let recipe_start = worker.checks.recipes.lock().unwrap().len();
    let (reached, held) = crossbeam_channel::bounded(1);
    let (release, resume) = crossbeam_channel::bounded(1);
    *worker.checks.wide_window_hold.lock().unwrap() = Some((reached, resume));
    let peers = fixtures[..2]
        .iter()
        .enumerate()
        .map(|(i, f)| request(&f.path, &f.edited, peer_scale, 21 + i as u64))
        .collect::<Vec<_>>();
    set_batch_view(&worker, &peers);
    worker.set_overviews(peers.clone(), &paths, &paths, Instant::now());
    held.recv_timeout(Duration::from_secs(60))
        .expect("both warm peers submit before arrival");
    let first_window = worker.checks.wide_submissions.lock().unwrap()[submission_start].clone();
    assert_eq!(first_window.len(), 2);
    assert!(first_window.iter().all(|stamp| peers
        .iter()
        .any(|r| r.path == stamp.target.path && r.revision == stamp.revision)));
    let mut wanted = peers.clone();
    // Hold only the new cold acquisition. Warm peers must settle without it.
    let cold_hold =
        (arrival && matches!(third, Third::Cold)).then(|| worker.hold_source(paths[2].clone()));
    if arrival {
        let settings = if matches!(third, Third::Cached) {
            &fixtures[2].edited
        } else {
            &fixtures[2].baseline
        };
        wanted.push(request(&fixtures[2].path, settings, reduced_scale, 23));
        set_batch_view(&worker, &wanted);
        worker.set_overviews(wanted.clone(), &paths, &paths, Instant::now());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let plan = worker.test_pending.0.lock().unwrap().plan.clone();
            if plan
                .tasks
                .iter()
                .any(|task| task.request.path == paths[2] && task.request.revision == 23)
            {
                break;
            }
            assert!(Instant::now() < deadline, "third target was not published");
            std::thread::yield_now();
        }
    }
    drop(release);
    if let Some(hold) = &cold_hold {
        hold.reached
            .recv_timeout(Duration::from_secs(60))
            .expect("cold source acquisition starts");
    }
    let mut seen = HashSet::new();
    let mut frames = Vec::new();
    if cold_hold.is_some() {
        for _ in 0..2 {
            let frame = receive_frame(&worker, &wanted, &mut seen);
            assert!(
                peers.iter().any(|r| r.path == frame.path),
                "warm peers must finish while C is held"
            );
            frames.push(frame);
        }
        assert!(
            worker.busy(),
            "cold target remains pending after both peers settle"
        );
    }
    drop(cold_hold);
    while frames.len() < wanted.len() {
        let frame = receive_frame(&worker, &wanted, &mut seen);
        if frames.is_empty() && arrival && matches!(third, Third::Cached) {
            assert_eq!(
                frame.path, paths[2],
                "cached pixels must arrive without waiting for warm peers"
            );
        }
        frames.push(frame);
    }
    wait_settled(&worker);
    let activity = worker
        .checks
        .gpu_activity
        .lock()
        .unwrap()
        .since(activity_before);
    let recipes = worker.checks.recipes.lock().unwrap()[recipe_start..].to_vec();
    let submissions = worker.checks.wide_submissions.lock().unwrap()[submission_start..].iter().map(|window| {
        window.iter().map(|stamp| json!({"file":stamp.target.path.file_name().unwrap().to_string_lossy(),"revision":stamp.revision,"task":stamp.id.0})).collect::<Vec<_>>()
    }).collect::<Vec<_>>();
    let new_loads = worker.checks.source_loads.load(Relaxed) - source_loads;
    assert_eq!(
        new_loads,
        u64::from(arrival && matches!(third, Third::Cold))
    );
    for frame in &frames {
        if let Some((_, origin)) = origins.iter().find(|(path, _)| path == &frame.path) {
            assert_eq!(
                &frame.origin, origin,
                "warm source identity must survive arrival"
            );
        } else {
            assert_eq!(frame.origin, *Source::open(&frame.path).unwrap().origin());
        }
        verify(
            &worker,
            std::slice::from_ref(frame),
            fixtures,
            scale_for(&frame.path),
            frame.revision != 23 || matches!(third, Third::Cached),
        );
    }
    if arrival && matches!(third, Third::Cached) {
        let cached = recipes
            .iter()
            .filter(|(revision, _)| *revision == 23)
            .collect::<Vec<_>>();
        assert!(!cached.is_empty(), "C must reach presentation reuse");
        assert!(
            cached.iter().all(|(_, count)| *count == 0),
            "every C attempt must reuse its photographic pixels"
        );
    }
    let result = json!({
        "third":if matches!(third,Third::Cached) {"cached"} else {"cold-held"},
        "peer_scale":peer_scale.divisor(),"arrival_scale":reduced_scale.divisor(),
        "arrival":arrival,"source_loads":new_loads,
        "admissions":worker.checks.wide_admitted.load(Relaxed)-admitted,
        "completed_windows":worker.checks.wide_windows.load(Relaxed)-windows,
        "submission_windows":submissions,"completed_recipe_dispatches":recipes,"gpu_activity":activity,
        "latest_targets_exact":frames.len(),"settled":!worker.busy(),
    });
    assert!(
        worker.replies.try_recv().is_err(),
        "no duplicate completion after settlement"
    );
    println!(
        "{}",
        json!({"kind":"wide-mixed-arrival-run","observation":result})
    );
    drop(frames);
    worker.clear_all();
    wait_settled(&worker);
    drop(worker);
    result
}

fn assert_peers_not_restarted(control: &serde_json::Value, arrival: &serde_json::Value) {
    assert_eq!(
        control["admissions"], 2,
        "control admits each warm peer once"
    );
    assert_eq!(
        arrival["admissions"], control["admissions"],
        "unrelated viewport work must not retire and readmit useful peers"
    );
    assert_eq!(
        arrival["completed_windows"], control["completed_windows"],
        "unrelated viewport work must not repeat peer submission windows"
    );
    for revision in [21, 22] {
        let dispatches = |run: &serde_json::Value| {
            run["completed_recipe_dispatches"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|recipe| recipe[0] == revision)
                .map(|recipe| recipe[1].as_u64().unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            dispatches(arrival),
            dispatches(control),
            "viewport arrival must preserve peer {revision}'s photographic work"
        );
    }
}

#[test]
#[ignore = "requires GPU, pinned Merrill fixtures and FOVEONIC_WIDE_RENDER_WIDTH=8; run alone"]
fn cached_viewport_arrival_preserves_exact_warm_peers_and_reuses_pixels() {
    assert_eq!(std::env::var("FOVEONIC_WIDE_RENDER_WIDTH").unwrap(), "8");
    let engine = Engine::new().unwrap();
    let mut fixtures = fixtures_for(
        &engine,
        3,
        Scale::new(8).unwrap(),
        "chroma",
        None,
        ProcessingQuality::Speed,
    );
    fixtures[2].edited = fixtures[2].baseline.clone();
    fixtures[2].edited.rotation.turns = 1;
    fixtures[2].expected_edited = expected(
        &engine,
        &Source::open(&fixtures[2].path).unwrap(),
        &request(
            &fixtures[2].path,
            &fixtures[2].edited,
            Scale::new(8).unwrap(),
            23,
        ),
    );
    let control = run_arrival(
        &engine,
        &fixtures,
        Scale::new(8).unwrap(),
        Third::Cached,
        false,
    );
    let arrival = run_arrival(
        &engine,
        &fixtures,
        Scale::new(8).unwrap(),
        Third::Cached,
        true,
    );
    assert_peers_not_restarted(&control, &arrival);
    println!(
        "{}",
        json!({"kind":"wide-mixed-arrival","control":control,"arrival":arrival,"performance_claim":false})
    );
}

#[test]
#[ignore = "requires GPU, pinned Merrill fixtures and FOVEONIC_WIDE_RENDER_WIDTH=8; run alone"]
fn cold_viewport_arrival_settles_warm_peers_before_held_source_and_matches_fresh() {
    assert_eq!(std::env::var("FOVEONIC_WIDE_RENDER_WIDTH").unwrap(), "8");
    let engine = Engine::new().unwrap();
    let fixtures = fixtures_for(
        &engine,
        3,
        Scale::new(8).unwrap(),
        "chroma",
        None,
        ProcessingQuality::Speed,
    );
    let control = run_arrival(
        &engine,
        &fixtures,
        Scale::new(8).unwrap(),
        Third::Cold,
        false,
    );
    let arrival = run_arrival(
        &engine,
        &fixtures,
        Scale::new(8).unwrap(),
        Third::Cold,
        true,
    );
    assert_peers_not_restarted(&control, &arrival);
    println!(
        "{}",
        json!({"kind":"wide-mixed-arrival","control":control,"arrival":arrival,"performance_claim":false})
    );
}

// Keep C reduced; only the two retained peers need native processing and patches.
fn native_peer_fixtures(engine: &Engine) -> Vec<Fixture> {
    let mut fixtures = fixtures_for(
        engine,
        3,
        Scale::new(8).unwrap(),
        "chroma",
        None,
        ProcessingQuality::Speed,
    );
    for fixture in &mut fixtures[..2] {
        fixture.baseline.preview_region = Some([1024, 1024, 1536, 1536]);
        fixture.edited.preview_region = fixture.baseline.preview_region;
        let source = Source::open(&fixture.path).unwrap();
        fixture.expected_baseline = expected(
            engine,
            &source,
            &request(&fixture.path, &fixture.baseline, Scale::NATIVE, 0),
        );
        fixture.expected_edited = expected(
            engine,
            &source,
            &request(&fixture.path, &fixture.edited, Scale::NATIVE, 0),
        );
    }
    fixtures
}

#[test]
#[ignore = "requires GPU, pinned Merrill fixtures and FOVEONIC_WIDE_RENDER_WIDTH=8; run alone"]
fn cached_viewport_arrival_preserves_exact_native_peers_and_reuses_pixels() {
    assert_eq!(std::env::var("FOVEONIC_WIDE_RENDER_WIDTH").unwrap(), "8");
    let engine = Engine::new().unwrap();
    let mut fixtures = native_peer_fixtures(&engine);
    fixtures[2].edited = fixtures[2].baseline.clone();
    fixtures[2].edited.rotation.turns = 1;
    fixtures[2].expected_edited = expected(
        &engine,
        &Source::open(&fixtures[2].path).unwrap(),
        &request(
            &fixtures[2].path,
            &fixtures[2].edited,
            Scale::new(8).unwrap(),
            23,
        ),
    );
    let control = run_arrival(&engine, &fixtures, Scale::NATIVE, Third::Cached, false);
    assert!(
        control["completed_windows"].as_u64().unwrap() > 1,
        "native peers must need further windows after the arrival hold"
    );
    let arrival = run_arrival(&engine, &fixtures, Scale::NATIVE, Third::Cached, true);
    assert_peers_not_restarted(&control, &arrival);
    println!(
        "{}",
        json!({"kind":"wide-mixed-arrival","control":control,"arrival":arrival,"performance_claim":false})
    );
}

#[test]
#[ignore = "requires GPU, pinned Merrill fixtures and FOVEONIC_WIDE_RENDER_WIDTH=8; run alone"]
fn cold_viewport_arrival_settles_native_peers_before_held_source_and_matches_fresh() {
    assert_eq!(std::env::var("FOVEONIC_WIDE_RENDER_WIDTH").unwrap(), "8");
    let engine = Engine::new().unwrap();
    let fixtures = native_peer_fixtures(&engine);
    let control = run_arrival(&engine, &fixtures, Scale::NATIVE, Third::Cold, false);
    let arrival = run_arrival(&engine, &fixtures, Scale::NATIVE, Third::Cold, true);
    assert_peers_not_restarted(&control, &arrival);
    println!(
        "{}",
        json!({"kind":"wide-mixed-arrival","control":control,"arrival":arrival,"performance_claim":false})
    );
}
