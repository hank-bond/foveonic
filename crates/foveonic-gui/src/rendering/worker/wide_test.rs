//! Measures opt-in warm multi-photo edits through the real worker and coordinator.

#[path = "wide_mixed_test.rs"]
mod mixed;
#[path = "wide_native_test.rs"]
mod native;
#[path = "wide_preview_test.rs"]
mod previews;

use super::{Frame, Request, Update, Worker};
use crate::rendering::delivery::{Destination, PhotoView, ViewState};
use foveonic::{
    image::{Description, View},
    inspection::Content,
    profile::Profile,
    render::{ControlCapabilities, Engine, ProcessingQuality, Scale, Settings},
    source::Source,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::Ordering::Relaxed,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Expected {
    settings: Settings,
    description: Description,
    view: View,
    samples: Vec<u8>,
}

struct Fixture {
    path: PathBuf,
    sha256: String,
    baseline: Settings,
    edited: Settings,
    expected_baseline: Expected,
    expected_edited: Expected,
}

fn integer(name: &str, default: usize) -> usize {
    std::env::var(name).map_or(default, |value| value.parse().expect(name))
}

fn request(path: &Path, settings: &Settings, scale: Scale, revision: u64) -> Request {
    Request {
        auxiliary_preview: false,

        detail: settings.preview_region.is_some(),
        framing_preview: false,
        calibration: None,
        rendition: crate::editor::Rendition::Current,
        revision,
        path: path.to_owned(),
        settings: settings.clone(),
        preview_scale: scale,
        processing_quality: settings.processing_quality,
        output: "final".into(),
        comparison: false,
        export: None,
    }
}

fn expected(engine: &Engine, source: &Source, request: &Request) -> Expected {
    let mut settings = request.settings.clone();
    assert!(crate::export::resolve_camera_settings(&mut settings, &source.metadata).is_empty());
    request
        .apply_preview(&mut settings, &source.metadata)
        .unwrap();
    assert_eq!(settings.working_scale, request.preview_scale);
    let image = engine
        .render(source, &settings, &mut Profile::default())
        .unwrap();
    Expected {
        settings: image.settings().clone(),
        description: image.description(),
        view: image.view(),
        samples: engine.readback_image(&image).unwrap(),
    }
}

fn fixtures(engine: &Engine, count: usize, scale: Scale, case: &str) -> Vec<Fixture> {
    fixtures_for(engine, count, scale, case, None, ProcessingQuality::Quality)
}

fn fixtures_for(
    engine: &Engine,
    count: usize,
    scale: Scale,
    case: &str,
    region: Option<[u32; 4]>,
    policy: ProcessingQuality,
) -> Vec<Fixture> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../scripts/profiling/wide_inputs.json"
    ))
    .unwrap();
    let directory = PathBuf::from(
        std::env::var_os("FOVEONIC_FIXTURE_DIR")
            .expect("set FOVEONIC_FIXTURE_DIR to the declared example photographs"),
    );
    manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .take(count)
        .map(|entry| {
            let path = directory.join(entry["file"].as_str().unwrap());
            let mut file = std::fs::File::open(&path).unwrap();
            let mut hash = Sha256::new();
            let mut bytes = [0_u8; 65536];
            loop {
                let read = file.read(&mut bytes).unwrap();
                if read == 0 {
                    break;
                }
                hash.update(&bytes[..read]);
            }
            let sha256 = format!("{:x}", hash.finalize());
            assert_eq!(
                sha256,
                entry["sha256"].as_str().unwrap(),
                "fixture identity: {}",
                path.display()
            );
            let source = Source::open(&path).unwrap();
            let capabilities = ControlCapabilities::from_metadata(&source.metadata).unwrap();
            assert_eq!(
                capabilities.family,
                foveonic::source::ProcessingFamily::Merrill
            );
            let mut baseline = Settings::new("", "");
            capabilities
                .apply_capture_defaults(&mut baseline, &source.metadata)
                .unwrap();
            baseline.processing_quality = policy;
            baseline.preview_region = region;
            baseline.exposure_auto = false;
            baseline.exposure = 0.;
            baseline.reconstruction = Some(Default::default());
            baseline.reconstruction.as_mut().unwrap().chroma = 0;
            let mut edited = baseline.clone();
            match case {
                "chroma" => edited.reconstruction.as_mut().unwrap().chroma = 1,
                "exposure" => edited.exposure = 0.25,
                "pan" => {
                    let mut region = region.expect("pan requires native regions");
                    region[0] += 576;
                    region[2] += 576;
                    edited.preview_region = Some(region);
                }
                _ => unreachable!(),
            }
            let expected_baseline = expected(engine, &source, &request(&path, &baseline, scale, 0));
            let expected_edited = expected(engine, &source, &request(&path, &edited, scale, 0));
            Fixture {
                path,
                sha256,
                baseline,
                edited,
                expected_baseline,
                expected_edited,
            }
        })
        .collect()
}

struct Batch {
    frames: Vec<Frame>,
    completion_order: Vec<String>,
    first_any_ms: f64,
    first_active_ms: f64,
    complete_set_ms: f64,
    settled_ms: f64,
    progress_before_settle: serde_json::Value,
}

fn wait_settled(worker: &Worker) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while worker.busy() {
        assert!(Instant::now() < deadline, "coordinator did not settle");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn phase_marker(
    markers: &mut Option<std::fs::File>,
    phase: &str,
    edge: &str,
    trial: usize,
    revision: u64,
) {
    if let Some(file) = markers {
        let epoch_ns = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        )
        .unwrap();
        writeln!(file, "{}", json!({"phase":phase,"edge":edge,"epoch_ns":epoch_ns,"trial":trial,"revision":revision,"pid":std::process::id()})).unwrap();
        file.flush().unwrap();
    }
}

fn set_batch_view(worker: &Worker, requests: &[Request]) {
    let active = requests.first().unwrap();
    worker.activate(&active.path);
    worker.set_view(ViewState {
        processing_quality: active.processing_quality,
        active: Some(active.path.clone()),
        rendition: active.rendition,
        output: "final".into(),
        comparison: false,
        framing_preview: false,
        exporting: false,
        calibration: None,
        settings: active.settings.clone(),
        overviews: requests
            .iter()
            .map(|request| PhotoView {
                rendition: request.rendition,
                path: request.path.clone(),
                settings: request.settings.clone(),
                displayed_revision: None,
                detail_revision: None,
            })
            .collect(),
        displayed_revision: None,
        detail_revision: None,
    });
}

fn batch(
    worker: &Worker,
    requests: Vec<Request>,
    phase: &str,
    trial: usize,
    markers: &mut Option<std::fs::File>,
) -> Batch {
    set_batch_view(worker, &requests);
    let first_revision = requests[0].revision;
    let wanted: HashSet<_> = requests
        .iter()
        .map(|request| (request.path.clone(), request.revision))
        .collect();
    let paths: Vec<_> = requests
        .iter()
        .map(|request| request.path.clone())
        .collect();
    phase_marker(markers, phase, "begin", trial, first_revision);
    let started = Instant::now();
    // One coordinator command publishes the complete edit cohort.
    worker.set_overviews(requests, &paths, &paths, started);
    let deadline = started + Duration::from_secs(120);
    let mut frames = Vec::with_capacity(paths.len());
    let mut seen = HashSet::new();
    let mut completion_order = Vec::new();
    let mut first_any_ms = None;
    let mut first_active_ms = None;
    let complete_set_ms;
    loop {
        let update = worker
            .replies
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("complete edit cohort before deadline");
        let received_ms = started.elapsed().as_secs_f64() * 1000.;
        let Update::Image(reply) = update else {
            panic!("unexpected non-image result");
        };
        assert!(
            wanted.contains(&(reply.path.clone(), reply.revision)),
            "unexpected reply revision"
        );
        assert!(
            seen.insert(reply.path.clone()),
            "duplicate photo completion"
        );
        let handling = reply
            .handling
            .expect("real coordinator must classify delivery");
        assert!(handling.current && handling.update_photo);
        assert!(matches!(
            handling.destination,
            Some(Destination::Active | Destination::Overview)
        ));
        first_any_ms.get_or_insert(received_ms);
        if handling.destination == Some(Destination::Active) {
            first_active_ms.get_or_insert(received_ms);
        }
        let frame = reply
            .result
            .expect("worker render succeeds")
            .expect("worker returns complete image");
        assert_eq!(frame.queued_at, started);
        completion_order.push(
            frame
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        );
        // Mirror UI image receipt. No egui paint or display scanout is measured.
        worker.progress.finish(frame.revision, false);
        worker.delivered(frame.revision);
        frames.push(frame);
        if frames.len() == paths.len() {
            complete_set_ms = received_ms;
            break;
        }
    }
    wait_settled(worker);
    let settled_ms = started.elapsed().as_secs_f64() * 1000.;
    phase_marker(markers, phase, "end", trial, first_revision);
    assert!(frames
        .iter()
        .all(|frame| worker.is_settled_and_admitted(frame)));
    let progress_before_settle = worker.progress.snapshot().map(|snapshot| json!({"revision":snapshot.revision,"fraction":snapshot.fraction,"running":snapshot.running(),"failed":snapshot.failed})).unwrap_or(serde_json::Value::Null);
    worker.progress.settle(false, None);
    if let Some(snapshot) = worker.progress.snapshot() {
        assert!(!snapshot.running() && !snapshot.failed && snapshot.fraction == 1.);
    }
    Batch {
        frames,
        completion_order,
        first_any_ms: first_any_ms.unwrap(),
        first_active_ms: first_active_ms.expect("active photo delivery"),
        complete_set_ms,
        settled_ms,
        progress_before_settle,
    }
}

fn verify(worker: &Worker, frames: &[Frame], fixtures: &[Fixture], scale: Scale, edited: bool) {
    let captures = tempfile::tempdir().unwrap();
    for frame in frames {
        let fixture = fixtures
            .iter()
            .find(|fixture| fixture.path == frame.path)
            .unwrap();
        let authored = if edited {
            &fixture.edited
        } else {
            &fixture.baseline
        };
        let expected = if edited {
            &fixture.expected_edited
        } else {
            &fixture.expected_baseline
        };
        assert_eq!(
            frame.requested, *authored,
            "authored edit must not be inferred from actual output"
        );
        assert_eq!(frame.processing_quality, authored.processing_quality);
        assert_eq!(frame.artifact.settings.working_scale, scale);
        assert_eq!(
            frame.artifact.settings.processing_quality,
            authored.processing_quality
        );
        assert_eq!(frame.artifact.settings.exposure, authored.exposure);
        assert_eq!(
            frame.artifact.settings.reconstruction,
            authored.reconstruction
        );
        assert_eq!(frame.artifact.settings, expected.settings);
        let Content::Image(image) = &frame.artifact.content else {
            panic!("final photographic image");
        };
        assert_eq!(image.description(), expected.description);
        assert_eq!(image.view(), expected.view);
        assert_eq!(image.settings(), &expected.settings);
        let captured = captures.path().join(format!("{}.f32", frame.revision));
        worker
            .capture_image(image.as_ref().clone(), captured.clone())
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        let samples = std::fs::read(captured).unwrap();
        assert!(
            samples == expected.samples,
            "complete floating samples differ: {}, actual SHA256 {:x}, expected SHA256 {:x}",
            fixture.path.display(),
            Sha256::digest(&samples),
            Sha256::digest(&expected.samples)
        );
    }
}

#[test]
#[ignore = "requires GPU, pinned FOVEONIC_FIXTURE_DIR and isolated wide-experiment process"]
fn real_worker_warm_edit_experiment() {
    let selected = std::env::var("FOVEONIC_WIDE_RENDER_WIDTH")
        .expect("select serial, 1, 2, 4 or 8 explicitly for same-binary comparison");
    let width: usize = match selected.as_str() {
        "serial" | "0" => 0,
        "1" => 1,
        "2" => 2,
        "4" => 4,
        "8" => 8,
        _ => panic!("FOVEONIC_WIDE_RENDER_WIDTH must be serial, 0, 1, 2, 4 or 8"),
    };
    let execution_mode = if width == 0 {
        "ordinary"
    } else {
        "experimental"
    };
    let task_width = width.max(1);
    let count = integer("FOVEONIC_WIDE_TEST_COUNT", 8);
    assert!(matches!(count, 1 | 2 | 4 | 8));
    let divisor = integer("FOVEONIC_WIDE_TEST_SCALE", 8);
    assert!(matches!(divisor, 1 | 2 | 4 | 8));
    let native = divisor == 1;
    let region = native.then_some([1024, 1024, 1536, 1536]);
    let memory_budget_bytes = native.then(|| {
        let gib = u64::try_from(integer("FOVEONIC_WIDE_TEST_CACHE_GIB", 4)).unwrap();
        assert!(gib > 0, "native experiment cache must be positive");
        gib.checked_mul(1024 * 1024 * 1024)
            .expect("native experiment cache byte count overflow")
    });
    let policy = match std::env::var("FOVEONIC_WIDE_TEST_QUALITY").as_deref() {
        Ok("speed") => ProcessingQuality::Speed,
        Ok("quality") | Err(_) => ProcessingQuality::Quality,
        _ => panic!("FOVEONIC_WIDE_TEST_QUALITY must be quality or speed"),
    };
    let scale = Scale::new(divisor as u32).unwrap();
    let trials = integer("FOVEONIC_WIDE_TEST_TRIALS", 7);
    assert!((1..=31).contains(&trials));
    let case = std::env::var("FOVEONIC_WIDE_TEST_CASE").unwrap_or_else(|_| "chroma".into());
    assert!(matches!(case.as_str(), "chroma" | "exposure") || (native && case == "pan"));
    assert!(!foveonic::diagnostics::enabled());
    assert!(matches!(
        crate::timing::mode(),
        foveonic::profile::Mode::Off
    ));
    let verifier = Engine::new().unwrap();
    let fixtures = fixtures_for(&verifier, count, scale, &case, region, policy);
    let worker = Worker::new(verifier.executor(), eframe::egui::Context::default());
    if let Some(limit) = memory_budget_bytes {
        worker.cache.configure(foveonic::decode_cache::Settings {
            enabled: false,
            ..Default::default()
        });
        worker.set_memory_budget(limit, Default::default());
        worker.set_background_completion(false);
    }
    // Separate memory runs opt into phase markers. Their file writes and
    // external sampling are not part of the latency comparison.
    let mut markers = std::env::var_os("FOVEONIC_WIDE_MEMORY_MARKERS").map(|path| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap()
    });
    println!(
        "{}",
        json!({"kind":"experiment-headless-real-worker-config","execution_mode":execution_mode,"width":width,"task_width":task_width,"photos":count,"scale":divisor,"quality":format!("{policy:?}"),"region":region,"memory_budget_bytes":memory_budget_bytes,"case":case,"memory_markers":markers.is_some(),"measured_trials":trials,"warmup_trials":1,"history":"clear_all and fully warm baseline before each timed edit","timing_endpoint":"coordinator-classified image receipt; no physical UI","control":"fresh ordinary Engine::render before all measured trials","fixtures":fixtures.iter().map(|fixture| json!({"file":fixture.path.file_name().unwrap().to_string_lossy(),"sha256":fixture.sha256})).collect::<Vec<_>>() })
    );
    for trial in 0..=trials {
        worker.clear_all();
        wait_settled(&worker);
        let revision_base = 1 + trial as u64 * 100;
        let baseline = batch(
            &worker,
            fixtures
                .iter()
                .enumerate()
                .map(|(index, fixture)| {
                    request(
                        &fixture.path,
                        &fixture.baseline,
                        scale,
                        revision_base + index as u64,
                    )
                })
                .collect(),
            "baseline",
            trial,
            &mut markers,
        );
        let retained = worker.memory_status();
        assert!(
            native
                || fixtures.iter().all(|fixture| retained
                    .reduced_sources
                    .iter()
                    .any(|(path, _)| path == &fixture.path)),
            "all requested photos must retain reduced inputs before timing"
        );
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let cache = worker.cache.status();
            if !cache.writing && cache.prefetching.is_empty() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "baseline source cache must quiesce before timing"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let source_before = worker.checks.source_loads.load(Relaxed);
        let admitted_before = worker.checks.wide_admitted.load(Relaxed);
        let windows_before = worker.checks.wide_windows.load(Relaxed);
        let activity_before = *worker.checks.gpu_activity.lock().unwrap();
        worker.checks.wide_max_live.store(0, Relaxed);
        worker.checks.wide_max_submitted.store(0, Relaxed);
        let edited = batch(
            &worker,
            fixtures
                .iter()
                .enumerate()
                .map(|(index, fixture)| {
                    request(
                        &fixture.path,
                        &fixture.edited,
                        scale,
                        revision_base + 20 + index as u64,
                    )
                })
                .collect(),
            "edit",
            trial,
            &mut markers,
        );
        let activity = worker
            .checks
            .gpu_activity
            .lock()
            .unwrap()
            .since(activity_before);
        let source_acquisitions = worker.checks.source_loads.load(Relaxed) - source_before;
        let admitted = worker.checks.wide_admitted.load(Relaxed) - admitted_before;
        let windows = worker.checks.wide_windows.load(Relaxed) - windows_before;
        let max_live = worker.checks.wide_max_live.load(Relaxed);
        let max_submitted = worker.checks.wide_max_submitted.load(Relaxed);
        if width == 0 {
            assert_eq!(
                (admitted, windows, max_live, max_submitted),
                (0, 0, 0, 0),
                "ordinary control must not enter the experimental driver"
            );
        }
        if native && width > 0 {
            assert!(
                admitted >= count as u64,
                "every native photo uses the shared path"
            );
            assert!((1..=width.min(count) as u64).contains(&max_live));
            assert!((1..=max_live).contains(&max_submitted));
        }
        assert_eq!(
            source_acquisitions, 0,
            "timed edits must use warm retained processing inputs"
        );
        assert_eq!(activity.readbacks, 0, "no timed sample readback");
        assert_eq!(activity.timestamp_resolves, 0, "profiling must remain Off");
        // Read back both sets after this interval. In particular, verifying the
        // first baseline before taking the activity snapshot would charge those
        // diagnostic reads to the first timed edit's next worker snapshot.
        if trial == 0 {
            verify(&worker, &baseline.frames, &fixtures, scale, false);
        }
        verify(&worker, &edited.frames, &fixtures, scale, true);
        println!(
            "{}",
            json!({"kind":"experiment-headless-real-worker-trial","execution_mode":execution_mode,"width":width,"task_width":task_width,"photos":count,"scale":divisor,"quality":format!("{policy:?}"),"region":region,"case":case,"trial":trial,"warmup":trial==0,"first_any_ms":edited.first_any_ms,"first_active_ms":edited.first_active_ms,"complete_set_ms":edited.complete_set_ms,"coordinator_settled_ms":edited.settled_ms,"completion_order":edited.completion_order,"source_acquisitions":source_acquisitions,"source_payload_reads":"not instrumented separately","wide_admitted":admitted,"wide_windows":windows,"wide_max_live":max_live,"wide_max_submitted":max_submitted,"requested_width_observed":max_submitted==width.min(count) as u64,"gpu_activity":activity,"retained_worker_before":retained.resources,"retained_worker_after":worker.memory_status().resources,"samples_settings_geometry_exact":true,"coordinator_settled":!worker.busy(),"progress_before_receiver_settle":edited.progress_before_settle,"progress_owner":"test receiver mirrors UI finish/settle","physical_ui":false})
        );
    }
    println!(
        "{}",
        json!({"kind":"experiment-headless-real-worker-complete","execution_mode":execution_mode,"width":width,"task_width":task_width,"photos":count,"scale":divisor,"quality":format!("{policy:?}"),"region":region,"case":case,"measured_trials":trials,"warmups":1,"all_exact":true,"physical_ui":false})
    );
}

#[test]
#[ignore = "requires GPU timestamps, pinned fixtures, WIDTH=8 and FOVEONIC_WIDE_CAPTURE_DIR; run alone"]
fn wide_profiled_edit_attribution() {
    profiled_edit_attribution(
        8,
        Scale::new(8).unwrap(),
        None,
        ProcessingQuality::Quality,
        "chroma",
    );
}

#[test]
#[ignore = "requires GPU timestamps, pinned fixtures, WIDTH=2 and FOVEONIC_WIDE_CAPTURE_DIR; run alone"]
fn wide_native_profiled_edit_attribution() {
    let case = std::env::var("FOVEONIC_WIDE_TEST_CASE").unwrap_or_else(|_| "exposure".into());
    assert!(matches!(case.as_str(), "exposure" | "pan"));
    profiled_edit_attribution(
        2,
        Scale::NATIVE,
        Some([1024, 1024, 1536, 1536]),
        ProcessingQuality::Speed,
        &case,
    );
}

fn profiled_edit_attribution(
    count: usize,
    scale: Scale,
    region: Option<[u32; 4]>,
    policy: ProcessingQuality,
    case: &str,
) {
    use foveonic::diagnostics::{CaptureConfig, Mode, Recorder};

    assert_eq!(
        std::env::var("FOVEONIC_WIDE_RENDER_WIDTH").unwrap(),
        count.to_string()
    );
    let native = scale == Scale::NATIVE;
    assert!(
        !foveonic::diagnostics::enabled(),
        "capture test must run alone"
    );
    let directory = PathBuf::from(
        std::env::var_os("FOVEONIC_WIDE_CAPTURE_DIR")
            .expect("set a unique retained capture directory"),
    );
    assert!(
        !directory.join("trace.pftrace").exists(),
        "never overwrite an earlier capture"
    );
    let verifier = Engine::new().unwrap();
    assert!(
        verifier
            .device()
            .features()
            .contains(eframe::wgpu::Features::TIMESTAMP_QUERY),
        "GPU attribution requires TIMESTAMP_QUERY support"
    );
    let fixtures = if native {
        fixtures_for(&verifier, count, scale, case, region, policy)
    } else {
        fixtures(&verifier, count, scale, case)
    };
    let worker = Worker::new(verifier.executor(), eframe::egui::Context::default());
    if native {
        worker.cache.configure(foveonic::decode_cache::Settings {
            enabled: false,
            ..Default::default()
        });
        worker.set_memory_budget(4 * 1024 * 1024 * 1024, Default::default());
        worker.set_background_completion(false);
    }
    let mut markers = None;
    let baseline = batch(
        &worker,
        fixtures
            .iter()
            .enumerate()
            .map(|(index, fixture)| {
                request(&fixture.path, &fixture.baseline, scale, 1 + index as u64)
            })
            .collect(),
        "baseline",
        0,
        &mut markers,
    );
    if !native {
        let retained = worker.memory_status();
        assert!(
            fixtures.iter().all(|fixture| retained
                .reduced_sources
                .iter()
                .any(|(path, _)| path == &fixture.path)),
            "all reduced inputs must be warm"
        );
    }
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let cache = worker.cache.status();
        if !cache.writing && cache.prefetching.is_empty() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "baseline source cache must quiesce before capture"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let source_before = worker.checks.source_loads.load(Relaxed);
    let admitted_before = worker.checks.wide_admitted.load(Relaxed);
    let windows_before = worker.checks.wide_windows.load(Relaxed);
    let activity_before = *worker.checks.gpu_activity.lock().unwrap();
    worker.checks.wide_max_live.store(0, Relaxed);
    worker.checks.wide_max_submitted.store(0, Relaxed);
    let recorder = Recorder::new();
    recorder
        .start(CaptureConfig {
            mode: Mode::Gpu,
            ..Default::default()
        })
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let capture_id = foveonic::diagnostics::capture_id().expect("capture is active");
    let edited = batch(
        &worker,
        fixtures
            .iter()
            .enumerate()
            .map(|(index, fixture)| {
                request(&fixture.path, &fixture.edited, scale, 21 + index as u64)
            })
            .collect(),
        "edit",
        0,
        &mut markers,
    );
    // The verifier shares the worker's device and queue, while image ownership
    // remains separate. Drain timestamp callbacks before closing the capture.
    verifier.wait_for_submitted_work().unwrap();
    let activity = worker
        .checks
        .gpu_activity
        .lock()
        .unwrap()
        .since(activity_before);
    let source_acquisitions = worker.checks.source_loads.load(Relaxed) - source_before;
    let admitted = worker.checks.wide_admitted.load(Relaxed) - admitted_before;
    let windows = worker.checks.wide_windows.load(Relaxed) - windows_before;
    let max_live = worker.checks.wide_max_live.load(Relaxed);
    let max_submitted = worker.checks.wide_max_submitted.load(Relaxed);
    let recipe_activity = worker.checks.recipes.lock().unwrap();
    let photo_dispatches: Vec<_> = edited
        .frames
        .iter()
        .map(|frame| {
            let dispatches = recipe_activity
                .iter()
                .rev()
                .find(|(revision, _)| *revision == frame.revision)
                .expect("dispatch attribution for every delivered photo")
                .1;
            json!({"revision":frame.revision,"dispatches":dispatches})
        })
        .collect();
    drop(recipe_activity);
    let contexts: Vec<_> = edited
        .frames
        .iter()
        .map(|frame| {
            json!({
                "task":frame.diagnostic_task.id,
                "revision":frame.diagnostic_task.revision,
                "photo":foveonic::diagnostics::photo_id(&frame.path),
                "file":frame.path.file_name().unwrap().to_string_lossy(),
                "purpose":frame.diagnostic_task.purpose,
            })
        })
        .collect();
    let saved = recorder.stop_save(directory, json!({
        "application":"foveonic-gui-wide-experiment",
        "scenario":if native { "headless-real-worker-warm-native-region-attribution" } else { "headless-real-worker-warm-chroma-attribution" },
        "physical_ui":false,
        "performance_claim":false,
        "width":count,"photos":count,"scale":scale.divisor(),"quality":format!("{policy:?}"),
        "region":region,"case":case,
        "disk_cache_enabled":!native,"background_completion_enabled":!native,
        "memory_budget_bytes":native.then_some(4_u64 * 1024 * 1024 * 1024),
        "edit":{
            "chroma_before":fixtures[0].baseline.reconstruction.as_ref().unwrap().chroma,
            "chroma_after":fixtures[0].edited.reconstruction.as_ref().unwrap().chroma,
            "exposure_before":fixtures[0].baseline.exposure,"exposure_after":fixtures[0].edited.exposure,
            "region_before":fixtures[0].baseline.preview_region,"region_after":fixtures[0].edited.preview_region,
        },
        "capture_id":capture_id,
        "expected_task_contexts":contexts,
        "source_acquisitions":source_acquisitions,
        "wide_admitted":admitted,"wide_windows":windows,
        "wide_max_live":max_live,"wide_max_submitted":max_submitted,
        "gpu_activity":activity,
        "photo_dispatches":photo_dispatches,
        "gpu_coverage_checked":false,
        "query_reconciliation":"inspect started/resolved/failed/dropped query IDs, reasons and task contexts in trace",
        "verification":"complete samples/settings/geometry checked after capture stop",
        "fixtures":fixtures.iter().map(|fixture| json!({"file":fixture.path.file_name().unwrap().to_string_lossy(),"sha256":fixture.sha256})).collect::<Vec<_>>(),
    })).unwrap().recv_timeout(Duration::from_secs(60)).unwrap().unwrap();
    assert!(!foveonic::diagnostics::enabled());
    // Preserve the capture even when these experiment assertions fail.
    assert_eq!(source_acquisitions, 0);
    assert_eq!(
        admitted, count as u64,
        "every warm photo must enter the shared execution path"
    );
    assert_eq!(max_live, count as u64);
    assert_eq!(
        max_submitted, count as u64,
        "all jobs must submit in one actual window"
    );
    assert!(windows > 0 && activity.timestamp_resolves > 0);
    let graph_dispatches: u64 = photo_dispatches
        .iter()
        .map(|photo| {
            let count = photo["dispatches"].as_u64().unwrap();
            assert!(
                count > 0,
                "wide photo processing must not be attributed as zero work"
            );
            count
        })
        .sum();
    assert_eq!(
        graph_dispatches + count as u64,
        activity.dispatches,
        "one additional view dispatch per delivered photograph"
    );
    assert_eq!(
        activity.readbacks, activity.timestamp_resolves,
        "capture reads timestamps only, photographic sample readback follows stop"
    );
    verify(&worker, &baseline.frames, &fixtures, scale, false);
    verify(&worker, &edited.frames, &fixtures, scale, true);
    println!(
        "{}",
        json!({
            "kind":"experiment-headless-real-worker-profiled-attribution",
            "directory":saved.directory,"trace":saved.trace,"manifest":saved.manifest,
            "summary":saved.summary,"trace_bytes":saved.trace_bytes,
            "capture_id":capture_id,"width":count,"photos":count,"scale":scale.divisor(),
            "quality":format!("{policy:?}"),"region":region,"case":case,
            "source_acquisitions":source_acquisitions,"wide_admitted":admitted,"wide_windows":windows,
            "wide_max_submitted":max_submitted,"gpu_activity":activity,
            "samples_settings_geometry_exact":true,"coordinator_settled":!worker.busy(),
            "gpu_coverage_checked":false,"physical_ui":false,"performance_claim":false,
        })
    );
}
