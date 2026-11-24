//! Exercises real coordinator handoffs with a controlled executor and bounded image queues.

use super::*;
use crate::rendering::coordinator::Task;
use crate::rendering::request::RequestId;
use crate::rendering::view_plan::ImageState;
use crate::rendering::worker;
use foveonic::render::ProcessingQuality;
use foveonic::render::Settings;
use std::{path::PathBuf, time::Duration};

struct Rig {
    _sources: super::super::source_loader::Handle,
    handle: Handle,
    slot: Slot,
    results: Sender<Update>,
    updates: Receiver<Update>,
    progress: crate::progress::Tracker,
}
impl Rig {
    fn new() -> Self {
        let slot = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let progress = crate::progress::Tracker::default();
        let (results, input) = crossbeam_channel::bounded(2);
        let cache = Arc::new(Cache::new());
        let (sources, loader, _) =
            super::super::source_loader::start(cache.clone(), progress.clone(), slot.clone());
        let (handle, updates) = Handle::start(
            slot.clone(),
            input,
            cache,
            progress.clone(),
            Default::default(),
            loader,
        );
        Self {
            _sources: sources,
            handle,
            slot,
            results,
            updates,
            progress,
        }
    }
    /// Publish these viewport photos' whole images, all to be rendered.
    fn requests(&self, requests: Vec<RenderRequest>) {
        let photos = requests
            .iter()
            .map(|request| request.path.clone())
            .collect();
        let images = requests
            .into_iter()
            .map(|request| Planned {
                request,
                speculative: false,
                state: ImageState::Render,
                tracked: Some(0),
            })
            .collect();
        self.handle.plan(planned(photos), images, Instant::now());
        self.applied();
    }
    fn applied(&self) {
        let sequence = self.handle.sequence.get();
        wait(|| self.handle.status().processed >= sequence);
    }
    fn tasks(&self) -> Vec<Arc<Task>> {
        self.slot.0.lock().unwrap().plan.tasks.clone()
    }
    fn receive(&self) -> worker::Reply {
        let Update::Image(reply) = self.updates.recv_timeout(Duration::from_secs(5)).unwrap()
        else {
            panic!("expected an image result");
        };
        *reply
    }
}

fn wait(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "coordinator failed to make progress"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn request(path: &str, revision: u64, exposure: f32) -> RenderRequest {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.exposure = exposure;
    RenderRequest {
        detail: false,
        path: path.into(),
        id: RequestId(revision),
        settings,
        framing_preview: false,
        calibration: None,
        rendition: Default::default(),
        preview_scale: foveonic::render::Scale::new(8).unwrap(),
        processing_quality: ProcessingQuality::Speed,
        output: "final".into(),
        comparison: false,
    }
}
fn planned(planned: Vec<PathBuf>) -> Photos {
    Photos {
        planned,
        ..Default::default()
    }
}
fn result(task: &Task) -> Update {
    Update::Image(Box::new(worker::Reply {
        handling: None,
        stamp: task.stamp.clone(),
        request: task.request.clone(),
        metadata: None,
        settings: task.request.settings.clone(),
        numeric_settings: None,
        reset_settings: None,
        result: crate::rendering::worker::RenderOutcome::Failed("test source failure".into()),
    }))
}

#[test]
fn coordinator_replaces_each_photo_and_keeps_siblings_without_ui_receipt() {
    let rig = Rig::new();
    for revision in 1..=12 {
        rig.requests(
            (0..8)
                .map(|i| request(&format!("{i}.x3f"), revision * 8 + i, revision as f32))
                .collect(),
        );
    }
    let tasks = rig.tasks();
    assert_eq!(tasks.len(), 8);
    assert!(tasks
        .iter()
        .all(|task| task.request.settings.exposure == 12.));
    let loading = Photos {
        loading: vec!["0.x3f".into()],
        ..Default::default()
    };
    rig.handle.plan(loading, vec![], Instant::now());
    rig.applied();
    assert_eq!(rig.tasks().len(), 1);
    rig.handle.clear(false);
    rig.applied();
    assert!(rig.tasks().is_empty());
    assert!(!rig.handle.busy());
}

#[test]
fn handled_results_settle_siblings_and_cannot_settle_a_newer_request() {
    let rig = Rig::new();
    rig.requests(vec![request("a.x3f", 1, 0.), request("b.x3f", 2, 0.)]);
    let old = rig.tasks();
    rig.requests(vec![request("a.x3f", 3, 1.), request("b.x3f", 4, 0.)]);
    rig.results.send(result(&old[0])).unwrap();
    assert!(!rig.receive().handling.unwrap().current);
    assert_eq!(rig.tasks().len(), 2);
    rig.results.send(result(&old[1])).unwrap();
    assert!(rig.receive().handling.unwrap().current);
    assert!(rig.handle.busy());
    let latest = rig.tasks();
    assert_eq!(latest.len(), 1);
    assert_eq!(latest[0].request.id, RequestId(3));
    rig.results.send(result(&latest[0])).unwrap();
    assert!(rig.receive().handling.unwrap().current);
    assert!(!rig.handle.busy());
}

#[test]
fn work_stays_busy_until_the_ui_receives_every_finished_image() {
    let rig = Rig::new();
    rig.requests(
        (0..3)
            .map(|i| request(&format!("{i}.x3f"), i + 1, 0.))
            .collect(),
    );
    for task in rig.tasks() {
        rig.results.send(result(&task)).unwrap();
    }
    // Two images fill the UI queue and the coordinator holds the third.
    wait(|| rig.updates.is_full() && rig.tasks().is_empty());
    assert!(rig.handle.busy());
    rig.receive();
    rig.receive();
    assert!(rig.handle.busy());
    rig.receive();
    // The coordinator republishes idle just after the hand-off.
    wait(|| !rig.handle.busy());
}

#[test]
fn full_ui_queue_does_not_prevent_new_intent_or_shutdown() {
    let rig = Rig::new();
    rig.requests(
        (0..8)
            .map(|i| request(&format!("{i}.x3f"), i + 1, 0.))
            .collect(),
    );
    let tasks = rig.tasks();
    for task in &tasks[..4] {
        rig.results.send(result(task)).unwrap();
    }
    wait(|| rig.updates.is_full() && rig.tasks().len() <= 5);
    rig.requests(
        (0..8)
            .map(|i| match i {
                0 => request("0.x3f", 20, 2.),
                _ => request(&format!("{i}.x3f"), i + 1, 0.),
            })
            .collect(),
    );
    rig.applied();
    assert!(rig
        .tasks()
        .iter()
        .any(|task| task.request.id == RequestId(20)));
    rig.handle.clear(true);
    rig.applied();
    assert!(rig.tasks().is_empty());
    // Images already handed to the UI stay busy until it receives them.
    wait(|| {
        while rig.updates.try_recv().is_ok() {}
        !rig.handle.busy()
    });
    let slot = rig.slot.clone();
    drop(rig);
    wait(|| slot.0.lock().unwrap().stop);
}

#[test]
fn coalescing_a_completed_request_preserves_its_finished_elapsed_time() {
    let rig = Rig::new();
    rig.requests(vec![request("a.x3f", 1, 0.)]);
    rig.results.send(result(&rig.tasks()[0])).unwrap();
    rig.receive();
    let finished = rig.progress.snapshot().unwrap();
    assert!(!finished.running());
    assert!(finished.failed, "the request's failure marks the pass");
    rig.requests(vec![request("a.x3f", 2, 0.)]);
    assert!(!rig.handle.busy());
    let coalesced = rig.progress.snapshot().unwrap();
    assert_eq!(finished.request_id, coalesced.request_id);
    assert_eq!(finished.milliseconds(), coalesced.milliseconds());
    rig.requests(vec![request("a.x3f", 3, 1.)]);
    assert!(rig.handle.busy());
    assert_eq!(rig.progress.snapshot().unwrap().request_id, 3);
}

#[test]
fn ui_uses_its_snapshot_while_the_coordinator_is_publishing() {
    let rig = Rig::new();
    rig.requests(vec![request("a.x3f", 1, 0.)]);
    assert!(rig.handle.busy());
    let locked = rig.handle.status.lock().unwrap();
    // A blocking read here would deadlock on the lock held by this thread.
    assert!(rig.handle.busy());
    drop(locked);
}

#[test]
fn idle_preparation_mode_changes_replace_the_config_while_identical_values_coalesce() {
    let rig = Rig::new();
    let sidecars = crate::sidecars::Session::new(
        eframe::egui::Context::default(),
        false,
        Arc::new(Cache::new()),
    );
    let mut config = PreparationConfig {
        processing_quality: ProcessingQuality::Quality,
        recipes: sidecars.cached_recipes(&[]),
        calibration: None,
    };
    rig.handle.preparation(config.clone());
    rig.applied();
    let original = rig.slot.0.lock().unwrap().preparation.clone().unwrap();
    assert_eq!(original.processing_quality, ProcessingQuality::Quality);
    let sequence = rig.handle.sequence.get();
    rig.handle.preparation(config.clone());
    assert_eq!(rig.handle.sequence.get(), sequence);
    config.processing_quality = ProcessingQuality::Speed;
    assert!(original.as_ref() != &config);
    rig.handle.preparation(config);
    rig.applied();
    let changed = rig.slot.0.lock().unwrap().preparation.clone().unwrap();
    assert_eq!(changed.processing_quality, ProcessingQuality::Speed);
    assert!(!Arc::ptr_eq(&original, &changed));
}

fn planned_patch(path: &str) -> Planned {
    let mut patch = request(path, 2, 0.);
    patch.detail = true;
    patch.preview_scale = foveonic::render::Scale::NATIVE;
    patch.settings.preview_region = Some([32, 32, 160, 160]);
    Planned {
        request: patch,
        speculative: false,
        state: ImageState::Render,
        tracked: Some(0),
    }
}

fn has_patch(rig: &Rig) -> bool {
    rig.tasks().iter().any(|task| task.request.detail)
}

#[test]
fn an_image_no_longer_in_use_stops() {
    let rig = Rig::new();
    let patch = planned_patch("a.x3f");
    let photos = vec![PathBuf::from("a.x3f")];
    assert!(rig
        .handle
        .plan(planned(photos.clone()), vec![patch], Instant::now()));
    rig.applied();
    assert!(has_patch(&rig));
    let whole = Planned {
        request: request("a.x3f", 3, 0.),
        speculative: false,
        state: ImageState::Ready,
        tracked: None,
    };
    rig.handle
        .plan(planned(photos), vec![whole], Instant::now());
    rig.applied();
    assert!(
        !has_patch(&rig),
        "a view back inside its current patch stops the patch in flight"
    );
}

#[test]
fn a_photo_that_leaves_the_view_stops_its_work_unless_it_waits_to_be_drawn() {
    let rig = Rig::new();
    let patch = planned_patch("a.x3f");
    let photos = vec![PathBuf::from("a.x3f")];
    rig.handle
        .plan(planned(photos.clone()), vec![patch], Instant::now());
    let loading = Photos {
        loading: photos,
        ..Default::default()
    };
    rig.handle.plan(loading, vec![], Instant::now());
    rig.applied();
    assert!(has_patch(&rig), "a deferred photo keeps its work");
    rig.handle.plan(Photos::default(), vec![], Instant::now());
    rig.applied();
    assert!(!has_patch(&rig));
}

#[test]
fn the_progress_pass_follows_only_the_photos_the_latest_plan_tracks() {
    let rig = Rig::new();
    rig.requests(vec![request("a.x3f", 1, 0.), request("b.x3f", 2, 0.)]);
    assert_eq!(rig.progress.snapshot().unwrap().checkpoints, 2);
    // Moving on to C leaves A and B: their work stops and their sections go.
    rig.requests(vec![request("c.x3f", 3, 0.)]);
    let snapshot = rig.progress.snapshot().unwrap();
    assert_eq!(snapshot.checkpoints, 1);
    assert_eq!(snapshot.request_id, 3);
    assert!(snapshot.running());
}

#[test]
fn work_the_bar_does_not_follow_leaves_the_finished_time_showing() {
    let rig = Rig::new();
    let whole = request("a.x3f", 1, 0.);
    let photos = vec![PathBuf::from("a.x3f")];
    let started = Instant::now() - std::time::Duration::from_millis(40);
    let tracked_whole = |state| Planned {
        request: whole.clone(),
        speculative: false,
        state,
        tracked: Some(0),
    };
    rig.handle.plan(
        planned(photos.clone()),
        vec![tracked_whole(ImageState::Render)],
        started,
    );
    rig.applied();
    rig.results.send(result(&rig.tasks()[0])).unwrap();
    rig.receive();
    wait(|| !rig.progress.snapshot().unwrap().running());
    let finished = rig.progress.snapshot().unwrap().milliseconds();
    assert!(finished >= 40.);
    // Zooming plans work the bar does not follow beside the current whole image.
    let mut patch = planned_patch("a.x3f");
    patch.tracked = None;
    rig.handle.plan(
        planned(photos),
        vec![tracked_whole(ImageState::Ready), patch],
        Instant::now(),
    );
    rig.applied();
    assert_eq!(rig.progress.snapshot().unwrap().milliseconds(), finished);
}
