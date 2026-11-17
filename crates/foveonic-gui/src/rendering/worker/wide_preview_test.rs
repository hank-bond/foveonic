//! Checks grouped previews, independent native overlays and preemption by viewport demand.

use super::*;
use crate::rendering::{coordinator::Stamp, request::Purpose};
use std::sync::Arc;

struct Warm {
    engine: Engine,
    worker: Worker,
    fixtures: Vec<Fixture>,
    native_requests: Vec<Request>,
    details: Vec<Frame>,
}
impl Warm {
    fn new(policy: ProcessingQuality) -> Self {
        Self::with_count(policy, 3)
    }
    fn with_count(policy: ProcessingQuality, count: usize) -> Self {
        let engine = Engine::new().unwrap();
        let scale = Scale::new(8).unwrap();
        let fixtures = fixtures_for(&engine, count, scale, "exposure", None, policy);
        let worker = Worker::new(engine.executor(), Default::default());
        worker.cache.configure(foveonic::decode_cache::Settings {
            enabled: false,
            ..Default::default()
        });
        worker.set_memory_budget(6 * 1024 * 1024 * 1024, Default::default());
        worker.set_background_completion(false);
        let mut markers = None;
        drop(batch(
            &worker,
            fixtures
                .iter()
                .enumerate()
                .map(|(i, f)| request(&f.path, &f.baseline, scale, 1 + i as u64))
                .collect(),
            "warm reduced",
            0,
            &mut markers,
        ));
        let native_requests = fixtures
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let mut r = request(&f.path, &f.edited, Scale::NATIVE, 21 + i as u64);
                r.detail = true;
                r.settings.preview_region = Some([1024, 1024, 1536, 1536]);
                r.settings.output_size = Some([6000, 4000]);
                r
            })
            .collect::<Vec<_>>();
        let details = batch(
            &worker,
            native_requests.clone(),
            "native detail",
            0,
            &mut markers,
        )
        .frames;
        Self {
            engine,
            worker,
            fixtures,
            native_requests,
            details,
        }
    }
    fn previews(&self) -> Vec<Request> {
        self.native_requests
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut r = r.clone();
                r.auxiliary_preview = true;
                r.detail = false;
                r.preview_scale = Scale::new(8).unwrap();
                r.settings.preview_region = None;
                r.revision = 41 + i as u64;
                r
            })
            .collect()
    }
    fn publish(&self, requests: Vec<Request>) {
        let paths = self
            .fixtures
            .iter()
            .map(|f| f.path.clone())
            .collect::<Vec<_>>();
        self.worker
            .set_overviews(requests, &paths, &paths, Instant::now());
    }
    fn reply(&self) -> crate::rendering::worker::Reply {
        let Update::Image(reply) = self
            .worker
            .replies
            .recv_timeout(Duration::from_secs(120))
            .unwrap()
        else {
            panic!("image reply")
        };
        *reply
    }
    fn verify(&self, frame: &Frame, request: &Request) {
        let source = Source::open(&frame.path).unwrap();
        let expected = expected(&self.engine, &source, request);
        assert_eq!(frame.artifact.settings, expected.settings);
        let Content::Image(image) = &frame.artifact.content else {
            panic!("image")
        };
        assert_eq!(image.description(), expected.description);
        assert_eq!(image.view(), expected.view);
        let capture = tempfile::tempdir().unwrap();
        let path = capture.path().join("preview.f32");
        self.worker
            .capture_image(image.as_ref().clone(), path.clone())
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), expected.samples);
    }
    fn collect(&self, requests: &[Request], count: usize) -> Vec<Frame> {
        (0..count)
            .map(|_| {
                let reply = self.reply();
                let handling = reply.handling.unwrap();
                assert!(handling.current);
                assert!(!handling.update_photo);
                assert_eq!(handling.destination, Some(Destination::Preview));
                let frame = reply
                    .result
                    .unwrap()
                    .expect("wanted preview must complete rather than settle on a yield");
                assert!(frame.auxiliary_preview);
                assert!(!frame.detail);
                assert_eq!(frame.requested.output_size, Some([6000, 4000]));
                assert_eq!(frame.artifact.settings.output_size, None);
                let r = requests
                    .iter()
                    .find(|r| r.revision == frame.revision)
                    .unwrap();
                self.verify(&frame, r);
                frame
            })
            .collect()
    }
}

fn wait_preview_plan(worker: &Worker) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while worker
        .test_pending
        .0
        .lock()
        .unwrap()
        .plan
        .tasks
        .iter()
        .filter(|t| t.stamp.target.purpose == Purpose::Preview)
        .count()
        != 3
    {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
fn check_preview_windows(windows: &[Vec<Stamp>]) {
    assert!(
        windows
            .iter()
            .any(|w| w.len() == 2 && w.iter().all(|s| s.target.purpose == Purpose::Preview)),
        "two browser previews must share actual submissions"
    );
    assert!(
        windows
            .iter()
            .all(|w| w.iter().all(|s| s.target.purpose == Purpose::Preview)
                || w.iter().all(|s| s.target.purpose != Purpose::Preview)),
        "preview and viewport operations cannot share a window"
    );
}

#[test]
#[ignore = "requires GPU and pinned Merrill fixtures; run alone"]
fn warm_previews_group_below_navigator_and_preserve_native_overlays() {
    for policy in [ProcessingQuality::Quality, ProcessingQuality::Speed] {
        let warm = Warm::new(policy);
        let loads = warm.worker.checks.source_loads.load(Relaxed);
        let mut photos = crate::multi_edit_runtime::Runtime::default();
        let mut sidecars =
            crate::sidecars::Session::new(Default::default(), false, warm.worker.cache.clone());
        sidecars.set_memory_only(true);
        let paths = warm
            .fixtures
            .iter()
            .map(|f| f.path.clone())
            .collect::<Vec<_>>();
        photos.sync(&paths, paths.len(), &mut sidecars);
        let Warm {
            engine,
            worker,
            fixtures,
            native_requests,
            details,
        } = warm;
        for detail in details {
            photos.retain(detail);
        }
        let textures = paths
            .iter()
            .map(|p| photos.detail(p).unwrap().texture.clone())
            .collect::<Vec<_>>();
        let warm = Warm {
            engine,
            worker,
            fixtures,
            native_requests,
            details: Vec::new(),
        };
        let progress = warm.worker.progress.snapshot().unwrap();
        warm.worker.checks.wide_submissions.lock().unwrap().clear();
        let requests = warm.previews();
        warm.publish(requests.clone());
        let frames = warm.collect(&requests, 3);
        assert_eq!(
            frames[0].path, paths[0],
            "navigator preview precedes the browser cohort"
        );
        for frame in frames {
            photos.retain_preview(frame).unwrap();
        }
        for (path, texture) in paths.iter().zip(textures) {
            assert!(Arc::ptr_eq(&photos.detail(path).unwrap().texture, &texture));
        }
        assert_eq!(warm.worker.checks.source_loads.load(Relaxed), loads);
        assert_eq!(
            warm.worker.progress.snapshot().unwrap().revision,
            progress.revision
        );
        check_preview_windows(&warm.worker.checks.wide_submissions.lock().unwrap());
    }
}

#[test]
#[ignore = "requires GPU and pinned Merrill fixtures; run alone"]
fn preview_preparation_waits_for_viewport_and_browser_waits_for_auxiliary() {
    let warm = Warm::new(ProcessingQuality::Speed);
    let requests = warm.previews();
    let foreground = warm.worker.cache.foreground();
    let auxiliary = warm.worker.cache.auxiliary_demand();
    let before = warm.worker.checks.wide_admitted.load(Relaxed);
    let recipes = warm.worker.checks.recipes.lock().unwrap().len();
    warm.publish(requests.clone());
    wait_preview_plan(&warm.worker);
    assert_eq!(warm.worker.checks.wide_admitted.load(Relaxed), before);
    assert_eq!(warm.worker.checks.recipes.lock().unwrap().len(), recipes);
    assert!(warm.worker.replies.try_recv().is_err());
    drop(foreground);
    let frames = warm.collect(&requests, 1);
    assert_eq!(frames[0].path, warm.fixtures[0].path);
    assert_eq!(
        warm.worker.checks.wide_admitted.load(Relaxed),
        before + 1,
        "browser preparation stays below auxiliary demand"
    );
    assert!(warm.worker.replies.try_recv().is_err());
    drop(auxiliary);
    warm.collect(&requests, 2);
}

#[test]
#[ignore = "requires GPU and pinned Merrill fixtures; run alone"]
fn submitted_preview_batch_yields_without_settlement_then_retries_after_viewport() {
    let warm = Warm::new(ProcessingQuality::Speed);
    let requests = warm.previews();
    let loads = warm.worker.checks.source_loads.load(Relaxed);
    warm.worker.checks.wide_submissions.lock().unwrap().clear();
    let (reached, arrival) = crossbeam_channel::bounded(1);
    let (release, resume) = crossbeam_channel::bounded(1);
    *warm.worker.checks.wide_window_hold.lock().unwrap() = Some((reached, resume));
    warm.publish(requests.clone());
    arrival
        .recv_timeout(Duration::from_secs(60))
        .expect("grouped browser window");
    let before = warm.worker.checks.wide_submissions.lock().unwrap().len();
    let mut viewport = warm.native_requests[0].clone();
    viewport.revision = 91;
    viewport.settings.exposure += 0.5;
    viewport.settings.output_size = None;
    let mut view = warm.native_requests.clone();
    view[0] = viewport.clone();
    set_batch_view(&warm.worker, &view);
    warm.worker.submit(viewport.clone());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !warm
        .worker
        .test_pending
        .0
        .lock()
        .unwrap()
        .plan
        .tasks
        .iter()
        .any(|t| t.viewport() && t.stamp.revision == 91)
    {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    drop(release);
    let mut viewport_seen = false;
    let mut previews = HashSet::new();
    while !viewport_seen || previews.len() != 3 {
        let reply = warm.reply();
        assert!(reply.handling.unwrap().current);
        let frame = reply
            .result
            .unwrap()
            .expect("yielded previews must retry before delivery");
        if frame.revision == 91 {
            assert!(!viewport_seen);
            viewport_seen = true;
            warm.verify(&frame, &viewport);
            warm.worker.progress.finish(frame.revision, false);
            warm.worker.delivered(frame.revision);
        } else {
            assert!(frame.auxiliary_preview);
            if frame.path != warm.fixtures[0].path {
                assert!(
                    viewport_seen,
                    "browser cannot complete ahead of new viewport demand"
                );
            }
            assert!(previews.insert(frame.path.clone()));
            warm.verify(
                &frame,
                requests
                    .iter()
                    .find(|r| r.revision == frame.revision)
                    .unwrap(),
            );
        }
    }
    let windows = warm.worker.checks.wide_submissions.lock().unwrap();
    assert!(
        windows[before]
            .iter()
            .all(|s| s.target.purpose != Purpose::Preview),
        "the next GPU window belongs to the viewport"
    );
    check_preview_windows(&windows);
    assert_eq!(warm.worker.checks.source_loads.load(Relaxed), loads);
}

#[test]
#[ignore = "requires GPU, pinned Merrill fixtures and an explicit serial/automatic comparison"]
fn warm_browser_preview_cohort_experiment() {
    assert!(!foveonic::diagnostics::enabled());
    assert!(matches!(
        crate::timing::mode(),
        foveonic::profile::Mode::Off
    ));
    let mode = std::env::var("FOVEONIC_WIDE_PREVIEW_TEST_MODE").unwrap();
    assert!(matches!(mode.as_str(), "serial" | "automatic"));
    let warm = Warm::with_count(ProcessingQuality::Speed, 4);
    warm.worker
        .set_render_width(if mode == "serial" { 1 } else { 8 });
    let loads = warm.worker.checks.source_loads.load(Relaxed);
    let mut results = Vec::new();
    for trial in 0..4 {
        let mut requests = warm.previews();
        for request in &mut requests {
            request.revision += 20 * trial;
            request.settings.photographic.brightness = [0.2, -0.2, 0.3, -0.3][trial as usize];
        }
        // Isolate the whole-photo preview phase after viewport work. No egui
        // drawing or physical screen presentation is measured by this probe.
        set_batch_view(&warm.worker, &requests);
        let before = *warm.worker.checks.gpu_activity.lock().unwrap();
        warm.worker.checks.wide_max_live.store(0, Relaxed);
        warm.worker.checks.wide_max_submitted.store(0, Relaxed);
        let began = Instant::now();
        warm.publish(requests.clone());
        let mut frames = Vec::new();
        let mut seen = HashSet::new();
        while frames.len() != requests.len() {
            let reply = warm.reply();
            let handling = reply.handling.unwrap();
            assert!(handling.current && !handling.update_photo);
            assert_eq!(handling.destination, Some(Destination::Preview));
            let frame = reply.result.unwrap().unwrap();
            assert!(seen.insert(frame.path.clone()));
            frames.push(frame);
        }
        let complete_set_ms = began.elapsed().as_secs_f64() * 1000.;
        let activity = warm
            .worker
            .checks
            .gpu_activity
            .lock()
            .unwrap()
            .since(before);
        assert!(activity.dispatches > 0, "observe worker-side GPU work");
        let max_live = warm.worker.checks.wide_max_live.load(Relaxed);
        let max_submitted = warm.worker.checks.wide_max_submitted.load(Relaxed);
        assert_eq!(warm.worker.checks.source_loads.load(Relaxed), loads);
        assert_eq!(activity.readbacks, 0);
        assert_eq!(activity.timestamp_resolves, 0);
        if mode == "serial" {
            assert_eq!(max_live, 0);
        } else {
            assert!(max_submitted >= 2, "actual grouped browser work");
        }
        let record = json!({"kind":"warm-browser-preview-cohort","mode":mode,"trial":trial,
            "warmup":trial==0,"photos":requests.len(),"complete_set_ms":complete_set_ms,
            "max_live":max_live,"max_submitted":max_submitted,"source_acquisitions":0,
            "all_exact":true,"quality":"Speed","memory_budget_bytes":6_u64*1024*1024*1024,
            "gpu_activity":activity,
            "timing_endpoint":"coordinator-classified preview receipt; no physical UI"});
        results.push((requests, frames, record));
    }
    // Hold the same bounded outputs in both modes and verify only after every
    // timed interval. Checks snapshots then contain no diagnostic readbacks.
    for (requests, frames, record) in results {
        for frame in &frames {
            warm.verify(
                frame,
                requests
                    .iter()
                    .find(|r| r.revision == frame.revision)
                    .unwrap(),
            );
        }
        println!("{record}");
    }
}
